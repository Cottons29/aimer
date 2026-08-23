#[path = "route/android.rs"]
pub mod android;
#[path = "route/desktop.rs"]
pub mod desktop;
#[path = "route/ios_device.rs"]
pub mod ios_device;
#[path = "route/simulator.rs"]
pub mod simulator;

use std::fmt;
use std::io;
use std::net::SocketAddr;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use super::readiness::ReadinessError;
use super::targets::TargetFamily;

pub use android::{AndroidRouteAdapter, RouteReservation};
pub use desktop::{DesktopRouteAdapter, LoopbackFamily};
pub use ios_device::IosDeviceRouteAdapter;
pub use simulator::SimulatorRouteAdapter;

/// A platform command whose diagnostic representation is safe to display.
///
/// Route commands never contain development-session secrets. Keeping the
/// executable and arguments in an owned value lets target adapters be tested
/// without spawning platform tools and provides one place to enforce redacted
/// diagnostics when private launch data is added later.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    program: String,
    arguments: Vec<String>,
}

impl CommandSpec {
    /// Creates one platform-tool invocation with non-secret arguments.
    #[inline]
    pub fn new(program: impl Into<String>, arguments: Vec<String>) -> Self {
        Self {
            program: program.into(),
            arguments,
        }
    }

    /// Returns the platform-tool executable name.
    #[inline]
    pub fn program(&self) -> &str {
        &self.program
    }

    /// Returns the non-secret arguments passed to the platform tool.
    #[inline]
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }
}

/// Executes target-management commands as child processes.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemCommandExecutor;

impl CommandExecutor for SystemCommandExecutor {
    fn output(&self, command: &CommandSpec) -> io::Result<CommandOutput> {
        let output = Command::new(&command.program)
            .args(&command.arguments)
            .output()?;
        Ok(CommandOutput {
            success: output.status.success(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

/// Captured output from a target-management command.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommandOutput {
    pub success: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl CommandOutput {
    /// Returns the combined diagnostic text of a failed platform command.
    pub fn diagnostic(&self) -> String {
        let stderr = String::from_utf8_lossy(&self.stderr);
        let trimmed = stderr.trim();
        if trimmed.is_empty() {
            String::from_utf8_lossy(&self.stdout).trim().to_owned()
        } else {
            trimmed.to_owned()
        }
    }
}

/// Executes target-management commands for a route adapter.
///
/// The production implementation uses `std::process::Command`; deterministic
/// tests provide an in-memory executor and observe behavior only through the
/// adapter's public reservation interface.
pub trait CommandExecutor: Send + Sync + 'static {
    fn output(&self, command: &CommandSpec) -> io::Result<CommandOutput>;
}

/// A platform command this session owns and runs exactly once on shutdown.
///
/// Cleanup must remove only what this run created, so the guard captures the
/// complete command when the resource is created rather than reconstructing it
/// during shutdown from state that may already have changed.
pub struct OwnedCommandGuard<E>
where
    E: CommandExecutor,
{
    executor: Arc<E>,
    command: CommandSpec,
}

impl<E> OwnedCommandGuard<E>
where
    E: CommandExecutor,
{
    /// Creates a guard that runs `command` when it is dropped.
    #[inline]
    pub const fn new(executor: Arc<E>, command: CommandSpec) -> Self {
        Self { executor, command }
    }

    /// Returns the cleanup command this guard owns.
    #[inline]
    pub const fn command(&self) -> &CommandSpec {
        &self.command
    }
}

impl<E> fmt::Debug for OwnedCommandGuard<E>
where
    E: CommandExecutor,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OwnedCommandGuard")
            .field("command", &self.command)
            .finish()
    }
}

impl<E> Drop for OwnedCommandGuard<E>
where
    E: CommandExecutor,
{
    fn drop(&mut self) {
        let _ = self.executor.output(&self.command);
    }
}

/// The authenticated address the CLI connects to for one target family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Endpoint {
    family: TargetFamily,
    address: SocketAddr,
}

impl Endpoint {
    /// Creates the endpoint of a prepared route.
    ///
    /// Every family except a physical iOS device is reachable only through
    /// loopback, so a non-loopback address means the app bound a public
    /// interface and the route is refused before any module is uploaded.
    pub fn new(family: TargetFamily, address: SocketAddr) -> Result<Self, RouteError> {
        if family.requires_loopback_listener() && !address.ip().is_loopback() {
            return Err(RouteError::PublicBinding { family, address });
        }
        Ok(Self { family, address })
    }

    /// Returns the family this endpoint belongs to.
    #[inline]
    pub const fn target(&self) -> TargetFamily {
        self.family
    }

    /// Returns the socket address the reload client connects to.
    #[inline]
    pub const fn address(&self) -> SocketAddr {
        self.address
    }
}

/// Failure while preparing or maintaining a hot-reload target route.
#[derive(Debug)]
pub enum RouteError {
    Command(io::Error),
    ForwardRejected(String),
    InvalidAllocatedPort(String),
    /// No device of this family is connected.
    NoTarget(TargetFamily),
    /// More than one device is connected and no explicit selection was made.
    AmbiguousTargets {
        family: TargetFamily,
        candidates: Vec<String>,
    },
    /// A device is connected but cannot host a development session.
    UnusableTarget {
        id: String,
        state: String,
    },
    /// The app announced a listener outside the allowed binding scope.
    PublicBinding {
        family: TargetFamily,
        address: SocketAddr,
    },
    /// The app never announced a usable listener.
    Readiness(ReadinessError),
    /// The advertised service was not resolved inside the discovery timeout.
    DiscoveryTimeout {
        service: String,
        waited: Duration,
    },
    /// The resolved advertisement disagrees with the launch announcement.
    ServiceMismatch {
        announced: u16,
        advertised: u16,
    },
    /// The device is locked, so it cannot start a development session.
    DeviceLocked,
    /// The user denied the local-network permission the route depends on.
    LocalNetworkPermissionDenied,
    /// The device is not paired or not trusted by this host.
    PairingFailed(String),
    /// The installed platform tooling cannot serve this route.
    UnsupportedTooling(String),
    /// The device is reachable by tooling but not by the reload transport.
    HostUnreachable(String),
}

impl fmt::Display for RouteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command(error) => {
                write!(formatter, "failed to execute target route command: {error}")
            }
            Self::ForwardRejected(message) => {
                write!(formatter, "target rejected hot-reload route: {message}")
            }
            Self::InvalidAllocatedPort(port) => {
                write!(formatter, "target returned invalid allocated port '{port}'")
            }
            Self::NoTarget(family) => write!(
                formatter,
                "no {family} target is available for a hot-reload session"
            ),
            Self::AmbiguousTargets { family, candidates } => write!(
                formatter,
                "several {family} targets are available ({}); select one explicitly",
                candidates.join(", ")
            ),
            Self::UnusableTarget { id, state } => write!(
                formatter,
                "target '{id}' is '{state}' and cannot host a hot-reload session"
            ),
            Self::PublicBinding { family, address } => write!(
                formatter,
                "the {family} app announced non-loopback listener {address}; refusing to connect"
            ),
            Self::Readiness(error) => error.fmt(formatter),
            Self::DiscoveryTimeout { service, waited } => write!(
                formatter,
                "service '{service}' was not resolved within {waited:?}"
            ),
            Self::ServiceMismatch {
                announced,
                advertised,
            } => write!(
                formatter,
                "the app announced port {announced} but advertised port {advertised}"
            ),
            Self::DeviceLocked => formatter.write_str(
                "the device is locked; unlock it and keep it unlocked for the development session",
            ),
            Self::LocalNetworkPermissionDenied => formatter.write_str(
                "the app was denied local-network access; allow it in Settings and relaunch, because the physical-device route needs it",
            ),
            Self::PairingFailed(message) => write!(
                formatter,
                "the device is not paired or trusted for development: {message}"
            ),
            Self::UnsupportedTooling(message) => write!(
                formatter,
                "the installed platform tooling cannot serve this route: {message}"
            ),
            Self::HostUnreachable(message) => write!(
                formatter,
                "the app listener is unreachable from this host: {message}"
            ),
        }
    }
}

impl std::error::Error for RouteError {}

impl From<ReadinessError> for RouteError {
    #[inline]
    fn from(error: ReadinessError) -> Self {
        Self::Readiness(error)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    pub(super) struct RecordingExecutor {
        pub(super) commands: Mutex<Vec<CommandSpec>>,
        pub(super) stdout: Vec<u8>,
        pub(super) success: bool,
    }

    impl RecordingExecutor {
        pub(super) fn successful(stdout: &[u8]) -> Self {
            Self {
                commands: Mutex::default(),
                stdout: stdout.to_vec(),
                success: true,
            }
        }

        pub(super) fn recorded(&self) -> Vec<CommandSpec> {
            self.commands.lock().unwrap().clone()
        }
    }

    impl CommandExecutor for RecordingExecutor {
        fn output(&self, command: &CommandSpec) -> io::Result<CommandOutput> {
            self.commands.lock().unwrap().push(command.clone());
            Ok(CommandOutput {
                success: self.success,
                stdout: self.stdout.clone(),
                stderr: Vec::new(),
            })
        }
    }

    #[test]
    fn loopback_families_reject_a_publicly_bound_app_listener() {
        let public: SocketAddr = "192.168.7.5:37654".parse().unwrap();

        for family in TargetFamily::ALL {
            let endpoint = Endpoint::new(family, public);
            if family.requires_loopback_listener() {
                assert!(matches!(
                    endpoint,
                    Err(RouteError::PublicBinding { .. })
                ));
            } else {
                assert_eq!(endpoint.unwrap().address(), public);
            }
        }
    }

    #[test]
    fn every_family_accepts_its_own_loopback_endpoint() {
        for family in TargetFamily::ALL {
            for address in ["127.0.0.1:37654", "[::1]:37654"] {
                let address: SocketAddr = address.parse().unwrap();
                let endpoint = Endpoint::new(family, address).unwrap();

                assert_eq!(endpoint.target(), family);
                assert_eq!(endpoint.address(), address);
            }
        }
    }

    #[test]
    fn an_owned_cleanup_command_runs_exactly_once_when_dropped() {
        let executor = Arc::new(RecordingExecutor::successful(b""));
        let command = CommandSpec::new("adb", vec!["forward".into(), "--remove-all".into()]);

        let guard = OwnedCommandGuard::new(Arc::clone(&executor), command.clone());
        assert_eq!(guard.command(), &command);
        assert!(executor.recorded().is_empty());
        drop(guard);

        assert_eq!(executor.recorded(), [command]);
    }
}
