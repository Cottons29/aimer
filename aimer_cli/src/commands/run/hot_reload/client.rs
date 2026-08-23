use std::fmt;
use std::io;
use std::net::{SocketAddr, TcpStream};

use aimer_reload_protocol::{
    ModuleMetadata, ProtocolError, ProtocolLimits, ReloadResult, SessionCredentials,
    TransferAcknowledgement, query_reload_result, send_module, send_reload_command,
};

/// Failure while connecting to a debug app or pushing a guest module.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("failed to connect to the Aimer reload listener: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
}

/// CLI-side client for one authenticated Aimer reload session.
///
/// The client owns private credentials but its diagnostic representation never
/// includes their bytes. Each upload establishes a fresh authenticated
/// connection, which gives reconnects independent nonces and directional keys.
pub struct ReloadClient {
    address: SocketAddr,
    credentials: SessionCredentials,
    limits: ProtocolLimits,
}

impl ReloadClient {
    /// Creates a client for the route selected by a target adapter.
    #[inline]
    pub const fn new(
        address: SocketAddr,
        credentials: SessionCredentials,
        limits: ProtocolLimits,
    ) -> Self {
        Self {
            address,
            credentials,
            limits,
        }
    }

    /// Connects and transfers one complete module to the app listener.
    pub fn push_module(
        &self,
        request_id: u64,
        module: &[u8],
    ) -> Result<TransferAcknowledgement, ClientError> {
        let mut stream = self.connect()?;
        Ok(send_module(
            &mut stream,
            &self.credentials,
            self.limits,
            request_id,
            module,
        )?)
    }

    /// Pushes one metadata-bound module and waits for the host safe-point result.
    pub fn push_reload(
        &self,
        request_id: u64,
        metadata: ModuleMetadata,
        module: &[u8],
    ) -> Result<ReloadResult, ClientError> {
        let mut stream = self.connect()?;
        Ok(send_reload_command(
            &mut stream,
            &self.credentials,
            self.limits,
            request_id,
            metadata,
            module,
        )?)
    }

    /// Recovers an outstanding terminal result after reconnecting.
    pub fn query_result(&self, request_id: u64) -> Result<Option<ReloadResult>, ClientError> {
        let mut stream = self.connect()?;
        Ok(query_reload_result(
            &mut stream,
            &self.credentials,
            self.limits,
            request_id,
        )?)
    }

    fn connect(&self) -> io::Result<TcpStream> {
        let stream = TcpStream::connect_timeout(&self.address, self.limits.io_timeout())?;
        stream.set_read_timeout(Some(self.limits.io_timeout()))?;
        stream.set_write_timeout(Some(self.limits.io_timeout()))?;
        Ok(stream)
    }
}

impl fmt::Debug for ReloadClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReloadClient")
            .field("address", &self.address)
            .field("credentials", &self.credentials)
            .field("limits", &self.limits)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use aimer_reload_protocol::ReloadStage;
    use aimer_reload_server::{ListenerSecurity, ReloadCommandListener, ReloadListener};

    use super::*;

    #[test]
    fn cli_client_pushes_a_module_to_the_authenticated_app_listener() {
        let credentials = SessionCredentials::from_parts([0x11; 16], [0xA5; 32]);
        let limits = ProtocolLimits::new(1024, Duration::from_secs(1));
        let received = Arc::new(Mutex::new(Vec::new()));
        let sink_received = Arc::clone(&received);
        let listener = ReloadListener::bind(
            "127.0.0.1:0",
            credentials.clone(),
            limits,
            move |module| {
                *sink_received.lock().unwrap() = module;
                Ok(())
            },
        )
        .unwrap();
        let client = ReloadClient::new(listener.local_addr().unwrap(), credentials, limits);
        let server = thread::spawn(move || listener.accept_once().unwrap());
        let module = b"\0asm\x01\0\0\0";

        let acknowledgement = client.push_module(77, module).unwrap();
        let server_acknowledgement = server.join().unwrap();

        assert_eq!(acknowledgement, server_acknowledgement);
        assert_eq!(*received.lock().unwrap(), module);
    }

    #[test]
    fn cli_client_observes_a_committed_generation_from_the_app_listener() {
        let credentials = SessionCredentials::from_parts([0x21; 16], [0xC5; 32]);
        let limits = ProtocolLimits::new(1024, Duration::from_secs(1));
        let expected = ReloadResult::Committed {
            active_generation: 4,
            reset_state_entries: 0,
            cleanup_warnings: 0,
        };
        let listener = ReloadCommandListener::bind(
            "127.0.0.1:0",
            credentials.clone(),
            limits,
            ListenerSecurity::new(Duration::from_secs(30), 4, Duration::from_secs(5)),
            {
                let expected = expected.clone();
                move |_| expected.clone()
            },
        )
        .unwrap();
        let client = ReloadClient::new(listener.local_addr().unwrap(), credentials, limits);
        let metadata = ModuleMetadata::new([1; 16], [2; 16], 1, 0, [3; 32]);
        let server = thread::spawn(move || listener.accept_connection().unwrap());

        assert_eq!(
            client
                .push_reload(44, metadata, b"\0asm\x01\0\0\0")
                .unwrap(),
            expected
        );
        server.join().unwrap();
    }

    #[test]
    fn cli_client_diagnostics_redact_session_credentials() {
        let credentials = SessionCredentials::from_parts([0x11; 16], [0xA5; 32]);
        let limits = ProtocolLimits::new(1024, Duration::from_secs(1));
        let client = ReloadClient::new("127.0.0.1:37654".parse().unwrap(), credentials, limits);

        let diagnostic = format!("{client:?}");

        assert!(!diagnostic.contains(&hex::encode([0x11; 16])));
        assert!(!diagnostic.contains(&hex::encode([0xA5; 32])));
        assert!(diagnostic.contains("[REDACTED]"));
    }

    #[test]
    fn cli_client_retains_rejection_and_recovers_it_after_reconnect() {
        let credentials = SessionCredentials::from_parts([0x31; 16], [0xB7; 32]);
        let limits = ProtocolLimits::new(1024, Duration::from_secs(1))
            .max_diagnostic_bytes(128)
            .max_terminal_results(4);
        let expected = ReloadResult::Rejected {
            stage: ReloadStage::Validation,
            error_code: 17,
            active_generation: 6,
            diagnostic: "unknown required widget".to_owned(),
        };
        let sink_result = expected.clone();
        let listener = ReloadCommandListener::bind(
            "127.0.0.1:0",
            credentials.clone(),
            limits,
            ListenerSecurity::new(Duration::from_secs(30), 4, Duration::from_secs(5)),
            move |_| sink_result.clone(),
        )
        .unwrap();
        let client = ReloadClient::new(listener.local_addr().unwrap(), credentials, limits);
        let metadata = ModuleMetadata::new([1; 16], [2; 16], 1, 0, [3; 32]);
        let server = thread::spawn(move || {
            listener.accept_connection().unwrap();
            listener.accept_connection().unwrap();
        });

        assert_eq!(client.push_reload(91, metadata, b"\0asm\x01\0\0\0").unwrap(), expected);
        assert_eq!(client.query_result(91).unwrap(), Some(expected));
        server.join().unwrap();
    }
}
