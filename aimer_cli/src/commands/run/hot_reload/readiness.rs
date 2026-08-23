use std::fmt;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

/// Prefix of the one line a development app writes when its listener is bound.
pub const READINESS_PREFIX: &str = "AIMER_RELOAD_LISTENER_READY";

/// The launch-control announcement of a bound app-side reload listener.
///
/// The announcement is deliberately non-secret: it carries only the public
/// session identifier, the port the app actually bound, the process identity the
/// client uses to distinguish a reconnect from a restart, and the protocol
/// version the app speaks. Authentication still happens on the stream, so a
/// forged announcement cannot authorize a module.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ListenerReadiness {
    session_id: [u8; 16],
    port: u16,
    process_id: u64,
    protocol: (u16, u16),
}

impl ListenerReadiness {
    /// Creates an announcement from app-side listener facts.
    #[inline]
    pub const fn new(
        session_id: [u8; 16],
        port: u16,
        process_id: u64,
        protocol: (u16, u16),
    ) -> Self {
        Self {
            session_id,
            port,
            process_id,
            protocol,
        }
    }

    /// Returns the public session identifier of the announcing app.
    #[inline]
    pub const fn session_id(&self) -> [u8; 16] {
        self.session_id
    }

    /// Returns the port the app listener actually bound.
    #[inline]
    pub const fn port(&self) -> u16 {
        self.port
    }

    /// Returns the announcing process identity.
    #[inline]
    pub const fn process_id(&self) -> u64 {
        self.process_id
    }

    /// Returns the announced protocol major and minor versions.
    #[inline]
    pub const fn protocol(&self) -> (u16, u16) {
        self.protocol
    }

    /// Renders the canonical announcement line.
    pub fn render(&self) -> String {
        format!(
            "{READINESS_PREFIX} session={session} port={port} pid={process} protocol={major}.{minor}",
            session = hex::encode(self.session_id),
            port = self.port,
            process = self.process_id,
            major = self.protocol.0,
            minor = self.protocol.1,
        )
    }

    /// Parses one canonical announcement line.
    ///
    /// Parsing is strict: the field order is fixed, unknown fields are rejected
    /// instead of ignored, and a field whose name suggests private session data
    /// fails closed so a leaking app cannot be silently accepted.
    pub fn parse(line: &str) -> Result<Self, ReadinessError> {
        let body = line
            .trim()
            .strip_prefix(READINESS_PREFIX)
            .ok_or_else(|| ReadinessError::Malformed(line.trim().to_owned()))?;
        let mut fields = body.split_whitespace();
        let session_id = parse_session(next_field(&mut fields, "session")?)?;
        let port = parse_number(next_field(&mut fields, "port")?, "port")?;
        let process_id = parse_number(next_field(&mut fields, "pid")?, "pid")?;
        let protocol = parse_protocol(next_field(&mut fields, "protocol")?)?;
        if let Some(extra) = fields.next() {
            let name = extra.split_once('=').map_or(extra, |(name, _)| name);
            return Err(if is_secret_field(name) {
                ReadinessError::SecretField(name.to_owned())
            } else {
                ReadinessError::UnknownField(name.to_owned())
            });
        }
        if port == 0 {
            return Err(ReadinessError::Malformed(
                "the app announced an unbound port".to_owned(),
            ));
        }

        Ok(Self {
            session_id,
            port,
            process_id,
            protocol,
        })
    }
}

fn next_field<'a>(
    fields: &mut impl Iterator<Item = &'a str>,
    expected: &str,
) -> Result<&'a str, ReadinessError> {
    let field = fields
        .next()
        .ok_or_else(|| ReadinessError::Malformed(format!("missing '{expected}' field")))?;
    let (name, value) = field
        .split_once('=')
        .ok_or_else(|| ReadinessError::Malformed(format!("field '{field}' has no value")))?;
    if name != expected {
        return Err(if is_secret_field(name) {
            ReadinessError::SecretField(name.to_owned())
        } else {
            ReadinessError::UnknownField(name.to_owned())
        });
    }
    Ok(value)
}

fn is_secret_field(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.contains("token") || name.contains("secret") || name.contains("key")
}

fn parse_session(value: &str) -> Result<[u8; 16], ReadinessError> {
    let mut session_id = [0_u8; 16];
    hex::decode_to_slice(value, &mut session_id)
        .map_err(|_| ReadinessError::Malformed("the session field is not 16 hex bytes".to_owned()))?;
    Ok(session_id)
}

fn parse_number<T: std::str::FromStr>(value: &str, field: &str) -> Result<T, ReadinessError> {
    value
        .parse()
        .map_err(|_| ReadinessError::Malformed(format!("field '{field}' is not a number")))
}

fn parse_protocol(value: &str) -> Result<(u16, u16), ReadinessError> {
    let (major, minor) = value
        .split_once('.')
        .ok_or_else(|| ReadinessError::Malformed("the protocol field is not 'major.minor'".to_owned()))?;
    Ok((
        parse_number(major, "protocol major")?,
        parse_number(minor, "protocol minor")?,
    ))
}

/// Failure while waiting for an app listener to announce readiness.
#[derive(Debug, Eq, PartialEq)]
pub enum ReadinessError {
    /// The announcement did not follow the canonical form.
    Malformed(String),
    /// The announcement carried a field this CLI version does not know.
    UnknownField(String),
    /// The announcement carried a field that must never leave the app.
    SecretField(String),
    /// The announcement belongs to a different development session.
    ForeignSession,
    /// The app speaks a protocol version this CLI cannot drive.
    IncompatibleProtocol { major: u16, minor: u16 },
    /// The launch console closed before the listener was ready.
    Exited { target: &'static str },
    /// The listener did not announce readiness inside the adapter timeout.
    Timeout {
        target: &'static str,
        waited: Duration,
    },
}

impl fmt::Display for ReadinessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(detail) => {
                write!(formatter, "the app announced an invalid listener: {detail}")
            }
            Self::UnknownField(name) => write!(
                formatter,
                "the app announced unknown listener field '{name}'; rebuild the app with this CLI version"
            ),
            Self::SecretField(name) => write!(
                formatter,
                "the app announced private session field '{name}' on its console; refusing to continue"
            ),
            Self::ForeignSession => formatter.write_str(
                "the app announced a different development session; another app is using this launch console",
            ),
            Self::IncompatibleProtocol { major, minor } => write!(
                formatter,
                "the app speaks reload protocol {major}.{minor}, which this CLI cannot drive"
            ),
            Self::Exited { target } => {
                write!(formatter, "the {target} app exited before its reload listener was ready")
            }
            Self::Timeout { target, waited } => write!(
                formatter,
                "the {target} app did not announce its reload listener within {waited:?}"
            ),
        }
    }
}

impl std::error::Error for ReadinessError {}

/// Waits for the announcement of the app this session launched.
///
/// The adapter streams launch-console lines into `announcements`; unrelated
/// lines are skipped so ordinary application logging cannot block startup. The
/// wait is bounded by `timeout` across the whole call, a closed channel means
/// the app exited, and a mismatched session or protocol fails closed with a
/// stable diagnostic instead of connecting to the wrong process.
pub fn await_listener_readiness(
    announcements: &Receiver<String>,
    expected_session: [u8; 16],
    expected_protocol: (u16, u16),
    target: &'static str,
    timeout: Duration,
) -> Result<ListenerReadiness, ReadinessError> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(ReadinessError::Timeout {
                target,
                waited: timeout,
            });
        }
        let line = match announcements.recv_timeout(remaining) {
            Ok(line) => line,
            Err(RecvTimeoutError::Timeout) => {
                return Err(ReadinessError::Timeout {
                    target,
                    waited: timeout,
                });
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(ReadinessError::Exited { target });
            }
        };
        if !line.trim().starts_with(READINESS_PREFIX) {
            continue;
        }
        let readiness = ListenerReadiness::parse(&line)?;
        if readiness.session_id() != expected_session {
            return Err(ReadinessError::ForeignSession);
        }
        if readiness.protocol() != expected_protocol {
            let (major, minor) = readiness.protocol();
            return Err(ReadinessError::IncompatibleProtocol { major, minor });
        }
        return Ok(readiness);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    const SESSION: [u8; 16] = [0x11; 16];
    const PROTOCOL: (u16, u16) = (1, 0);

    fn announcement() -> String {
        ListenerReadiness::new(SESSION, 37654, 4711, PROTOCOL).render()
    }

    #[test]
    fn a_canonical_announcement_round_trips_without_private_data() {
        let line = announcement();

        assert_eq!(
            line,
            format!(
                "AIMER_RELOAD_LISTENER_READY session={} port=37654 pid=4711 protocol=1.0",
                hex::encode(SESSION)
            )
        );
        assert_eq!(
            ListenerReadiness::parse(&line).unwrap(),
            ListenerReadiness::new(SESSION, 37654, 4711, PROTOCOL)
        );
    }

    #[test]
    fn malformed_unknown_and_secret_announcements_fail_closed() {
        let secret = format!("{}={}", "token", hex::encode([0xA5; 32]));

        assert_eq!(
            ListenerReadiness::parse("hello"),
            Err(ReadinessError::Malformed("hello".to_owned()))
        );
        assert_eq!(
            ListenerReadiness::parse(&format!("{READINESS_PREFIX} session=zz port=1 pid=1 protocol=1.0")),
            Err(ReadinessError::Malformed(
                "the session field is not 16 hex bytes".to_owned()
            ))
        );
        assert_eq!(
            ListenerReadiness::parse(&format!(
                "{READINESS_PREFIX} session={} port=0 pid=1 protocol=1.0",
                hex::encode(SESSION)
            )),
            Err(ReadinessError::Malformed(
                "the app announced an unbound port".to_owned()
            ))
        );
        assert_eq!(
            ListenerReadiness::parse(&format!("{} extra=1", announcement())),
            Err(ReadinessError::UnknownField("extra".to_owned()))
        );
        assert_eq!(
            ListenerReadiness::parse(&format!("{} {secret}", announcement())),
            Err(ReadinessError::SecretField("token".to_owned()))
        );
    }

    #[test]
    fn waiting_skips_app_logging_and_returns_the_announced_listener() {
        let (sender, receiver) = mpsc::channel();
        sender.send("app started".to_owned()).unwrap();
        sender.send(announcement()).unwrap();

        let readiness = await_listener_readiness(
            &receiver,
            SESSION,
            PROTOCOL,
            "macOS",
            Duration::from_secs(1),
        )
        .unwrap();

        assert_eq!(readiness.port(), 37654);
        assert_eq!(readiness.process_id(), 4711);
    }

    #[test]
    fn waiting_reports_timeout_exit_foreign_session_and_protocol_mismatch() {
        let (sender, receiver) = mpsc::channel();
        let timeout = Duration::from_millis(20);

        let timed_out =
            await_listener_readiness(&receiver, SESSION, PROTOCOL, "Linux", timeout).unwrap_err();
        drop(sender);
        let exited =
            await_listener_readiness(&receiver, SESSION, PROTOCOL, "Linux", timeout).unwrap_err();

        assert_eq!(
            timed_out,
            ReadinessError::Timeout {
                target: "Linux",
                waited: timeout,
            }
        );
        assert_eq!(exited, ReadinessError::Exited { target: "Linux" });
        assert!(
            timed_out
                .to_string()
                .starts_with("the Linux app did not announce its reload listener within")
        );

        let (sender, receiver) = mpsc::channel();
        sender
            .send(ListenerReadiness::new([0x22; 16], 1, 1, PROTOCOL).render())
            .unwrap();
        assert_eq!(
            await_listener_readiness(&receiver, SESSION, PROTOCOL, "Android", timeout),
            Err(ReadinessError::ForeignSession)
        );

        let (sender, receiver) = mpsc::channel();
        sender
            .send(ListenerReadiness::new(SESSION, 1, 1, (2, 3)).render())
            .unwrap();
        assert_eq!(
            await_listener_readiness(&receiver, SESSION, PROTOCOL, "iOS Simulator", timeout),
            Err(ReadinessError::IncompatibleProtocol { major: 2, minor: 3 })
        );
    }
}
