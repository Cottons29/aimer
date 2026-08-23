use std::fmt;

use aimer_reload_protocol::{ReloadResult, ReloadStage};

/// Compatibility identity announced by a development host after authentication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeIdentity {
    application_id: [u8; 16],
    process_id: u64,
    abi_major: u16,
    abi_minor: u16,
    capability_manifest_digest: [u8; 32],
}

impl RuntimeIdentity {
    /// Creates an identity from host launch/session metadata.
    #[inline]
    pub const fn new(
        application_id: [u8; 16],
        process_id: u64,
        abi_major: u16,
        abi_minor: u16,
        capability_manifest_digest: [u8; 32],
    ) -> Self {
        Self {
            application_id,
            process_id,
            abi_major,
            abi_minor,
            capability_manifest_digest,
        }
    }
}

/// Result of attaching an authenticated connection to the client session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionState {
    /// First connection to the launched process.
    Connected,
    /// Reconnection to the same process; outstanding result recovery is valid.
    Reconnected,
    /// The app restarted; an outstanding request belongs to the old process.
    ProcessRestarted,
}

/// A stable compatibility failure rendered before any module upload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompatibilityError {
    /// The listener belongs to a different application.
    Application,
    /// The runtime cannot execute this guest ABI.
    Abi,
    /// The host capability contract differs from the built guest.
    Capabilities,
}

/// Connection identity and one recoverable outstanding request.
#[derive(Debug)]
pub struct ClientSession {
    expected: RuntimeIdentity,
    connected_process: Option<u64>,
    outstanding_request: Option<u64>,
}

impl ClientSession {
    /// Creates a disconnected session from deterministic build metadata.
    #[inline]
    pub const fn new(expected: RuntimeIdentity) -> Self {
        Self {
            expected,
            connected_process: None,
            outstanding_request: None,
        }
    }

    /// Validates a host identity and classifies reconnect versus process restart.
    pub fn connect(
        &mut self,
        actual: RuntimeIdentity,
    ) -> Result<ConnectionState, CompatibilityError> {
        if actual.application_id != self.expected.application_id {
            return Err(CompatibilityError::Application);
        }
        if (actual.abi_major, actual.abi_minor)
            != (self.expected.abi_major, self.expected.abi_minor)
        {
            return Err(CompatibilityError::Abi);
        }
        if actual.capability_manifest_digest != self.expected.capability_manifest_digest {
            return Err(CompatibilityError::Capabilities);
        }

        match self.connected_process.replace(actual.process_id) {
            None => Ok(ConnectionState::Connected),
            Some(process_id) if process_id == actual.process_id => Ok(ConnectionState::Reconnected),
            Some(_) => {
                self.outstanding_request = None;
                Ok(ConnectionState::ProcessRestarted)
            }
        }
    }

    /// Records the request whose terminal result may need reconnect recovery.
    #[inline]
    pub fn begin_request(&mut self, request_id: u64) {
        self.outstanding_request = Some(request_id);
    }

    /// Returns the request valid for the currently connected process.
    #[inline]
    pub const fn outstanding_request(&self) -> Option<u64> {
        self.outstanding_request
    }
}

/// Console-independent status updates emitted by the hot-reload workflow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReloadStatus {
    /// Compile one source generation.
    Compiling { build: u64 },
    /// Transfer a bounded number of module bytes.
    Uploading { sent: usize, total: usize },
    /// Wait for the host safe point after upload acceptance.
    WaitingForCommit { request: u64 },
    /// Terminal protocol outcome.
    Terminal(ReloadResult),
    /// A watch-time build or transfer failed while the active app remains live.
    RecoverableFailure { diagnostic: String },
    /// A native provider or capability contract changed.
    ///
    /// The reason names the exact contract or dependency boundary that moved,
    /// because a hot reload cannot replace permanent native code.
    NativeRestartRequired { reason: String },
}

impl fmt::Display for ReloadStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Compiling { build } => write!(formatter, "building guest #{build}"),
            Self::Uploading { sent, total } => write!(formatter, "uploading {sent}/{total} bytes"),
            Self::WaitingForCommit { request } => {
                write!(formatter, "request #{request} waiting for app safe point")
            }
            Self::Terminal(ReloadResult::Committed {
                active_generation, ..
            }) => write!(formatter, "committed generation {active_generation}"),
            Self::Terminal(ReloadResult::Rejected {
                stage, diagnostic, ..
            }) => write!(formatter, "rejected at {}: {diagnostic}", stage_name(*stage)),
            Self::Terminal(ReloadResult::Cancelled { active_generation }) => {
                write!(formatter, "cancelled; generation {active_generation} remains active")
            }
            Self::RecoverableFailure { diagnostic } => {
                write!(formatter, "reload failed; active app retained: {diagnostic}")
            }
            Self::NativeRestartRequired { reason } => {
                write!(formatter, "native app restart required: {reason}")
            }
        }
    }
}

fn stage_name(stage: ReloadStage) -> &'static str {
    match stage {
        ReloadStage::Preflight => "preflight",
        ReloadStage::Instantiate => "instantiate",
        ReloadStage::Initialize => "initialize",
        ReloadStage::StateExport => "state export",
        ReloadStage::Migration => "migration",
        ReloadStage::StateImport => "state import",
        ReloadStage::Build => "build",
        ReloadStage::Validation => "validation",
        ReloadStage::Materialization => "materialization",
        ReloadStage::Reconciliation => "reconciliation",
        ReloadStage::CommitWait => "commit wait",
        ReloadStage::Cancellation => "cancellation",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(process_id: u64) -> RuntimeIdentity {
        RuntimeIdentity::new([1; 16], process_id, 1, 0, [2; 32])
    }

    #[test]
    fn client_reconnects_only_to_compatible_identity_and_detects_process_change() {
        let mut session = ClientSession::new(identity(10));

        assert_eq!(session.connect(identity(10)), Ok(ConnectionState::Connected));
        session.begin_request(44);
        assert_eq!(
            session.connect(identity(10)),
            Ok(ConnectionState::Reconnected)
        );
        assert_eq!(session.outstanding_request(), Some(44));
        assert_eq!(
            session.connect(identity(11)),
            Ok(ConnectionState::ProcessRestarted)
        );
        assert_eq!(session.outstanding_request(), None);

        let mut wrong_app = identity(11);
        wrong_app.application_id = [9; 16];
        assert_eq!(
            session.connect(wrong_app),
            Err(CompatibilityError::Application)
        );
        let mut wrong_abi = identity(11);
        wrong_abi.abi_major = 2;
        assert_eq!(session.connect(wrong_abi), Err(CompatibilityError::Abi));
        let mut wrong_capabilities = identity(11);
        wrong_capabilities.capability_manifest_digest = [8; 32];
        assert_eq!(
            session.connect(wrong_capabilities),
            Err(CompatibilityError::Capabilities)
        );
    }

    #[test]
    fn statuses_render_progress_rejection_and_restart_without_secrets() {
        assert_eq!(
            ReloadStatus::RecoverableFailure {
                diagnostic: "guest compilation exited with status 1".to_owned(),
            }
            .to_string(),
            "reload failed; active app retained: guest compilation exited with status 1"
        );
        assert_eq!(
            ReloadStatus::Uploading {
                sent: 512,
                total: 1024,
            }
            .to_string(),
            "uploading 512/1024 bytes"
        );
        assert_eq!(
            ReloadStatus::Terminal(ReloadResult::Rejected {
                stage: ReloadStage::Migration,
                error_code: 7,
                active_generation: 3,
                diagnostic: "state schema mismatch".to_owned(),
            })
            .to_string(),
            "rejected at migration: state schema mismatch"
        );
        assert_eq!(
            ReloadStatus::NativeRestartRequired {
                reason: "capability 'haptics' contract fingerprint changed".to_owned(),
            }
            .to_string(),
            "native app restart required: capability 'haptics' contract fingerprint changed"
        );
    }
}