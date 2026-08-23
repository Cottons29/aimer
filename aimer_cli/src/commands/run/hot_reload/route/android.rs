use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use zeroize::Zeroizing;

use super::super::launch::{LaunchConfiguration, LaunchStep};
use super::super::session::DevelopmentSession;
use super::super::targets::TargetFamily;
use super::{CommandExecutor, CommandSpec, Endpoint, OwnedCommandGuard, RouteError};

/// Activity the Android template starts for an Aimer application.
pub const LAUNCH_ACTIVITY: &str = "com.aimer.AimerActivity";
/// Session file inside the app's private data directory.
///
/// The path is relative because `run-as` starts in `/data/data/<package>`, and
/// the app removes the file as soon as it has read the session.
pub const SESSION_FILE: &str = "files/aimer_reload_session";

/// Prepares device-scoped Android loopback forwarding for app-listening reload.
pub struct AndroidRouteAdapter<E> {
    executor: Arc<E>,
}

impl<E> AndroidRouteAdapter<E>
where
    E: CommandExecutor,
{
    #[inline]
    pub fn new(executor: Arc<E>) -> Self {
        Self { executor }
    }

    /// Selects the single usable device from `adb devices -l` output.
    ///
    /// Ambiguity is a hard error: a development session installs an app and
    /// injects a secret, so guessing between devices is never acceptable. A
    /// device in any other state produces its exact `adb` state so the developer
    /// can authorize, wake, or reconnect it.
    pub fn select_device(listing: &str) -> Result<String, RouteError> {
        let mut usable = Vec::new();
        let mut unusable = None;
        for line in listing.lines().skip(1) {
            let mut fields = line.split_whitespace();
            let Some(id) = fields.next() else {
                continue;
            };
            let Some(state) = fields.next() else {
                continue;
            };
            if state == "device" {
                usable.push(id.to_owned());
            } else if unusable.is_none() {
                unusable = Some((id.to_owned(), state.to_owned()));
            }
        }

        match usable.len() {
            1 => Ok(usable.remove(0)),
            0 => match unusable {
                Some((id, state)) => Err(RouteError::UnusableTarget { id, state }),
                None => Err(RouteError::NoTarget(TargetFamily::Android)),
            },
            _ => Err(RouteError::AmbiguousTargets {
                family: TargetFamily::Android,
                candidates: usable,
            }),
        }
    }

    /// Builds the launch sequence that provisions the session and starts the app.
    ///
    /// The session never appears in a command argument, because `adb` arguments
    /// are visible in the host process list and `am start` arguments are copied
    /// into the device log. The credentials travel through the standard input of
    /// `run-as`, which writes them into the app's private data directory.
    pub fn launch(
        &self,
        device_id: &str,
        package: &str,
        session: &DevelopmentSession,
    ) -> LaunchConfiguration {
        let (session_id, token) = session.credentials().launch_environment_hex();
        let provision = LaunchStep::new(CommandSpec::new(
            "adb",
            vec![
                "-s".into(),
                device_id.into(),
                "shell".into(),
                "run-as".into(),
                package.into(),
                "sh".into(),
                "-c".into(),
                format!("mkdir -p files && cat > {SESSION_FILE}"),
            ],
        ))
        .private_stdin(Zeroizing::new(format!(
            "{}\n{}\n{}\n{}",
            session_id.as_str(),
            token.as_str(),
            session.listener_port(),
            session
                .host_config()
                .to_text()
                .expect("the built-in host policy must remain valid"),
        )));
        let start = LaunchStep::new(CommandSpec::new(
            "adb",
            vec![
                "-s".into(),
                device_id.into(),
                "shell".into(),
                "am".into(),
                "start".into(),
                "-n".into(),
                format!("{package}/{LAUNCH_ACTIVITY}"),
            ],
        ));

        LaunchConfiguration::new(vec![provision, start])
    }

    /// Returns the guard that removes this session's provisioned credentials.
    pub fn session_file_guard(&self, device_id: &str, package: &str) -> OwnedCommandGuard<E> {
        OwnedCommandGuard::new(
            Arc::clone(&self.executor),
            CommandSpec::new(
                "adb",
                vec![
                    "-s".into(),
                    device_id.into(),
                    "shell".into(),
                    "run-as".into(),
                    package.into(),
                    "rm".into(),
                    "-f".into(),
                    SESSION_FILE.into(),
                ],
            ),
        )
    }

    /// Returns the loopback endpoint of an owned forward.
    pub fn endpoint(&self, reservation: &RouteReservation<E>) -> Result<Endpoint, RouteError> {
        Endpoint::new(
            TargetFamily::Android,
            SocketAddr::from((Ipv4Addr::LOCALHOST, reservation.host_port())),
        )
    }

    /// Allocates an owned host-loopback route to `listener_port` on `device_id`.
    pub fn prepare(
        &self,
        device_id: &str,
        listener_port: u16,
    ) -> Result<RouteReservation<E>, RouteError> {
        let command = CommandSpec::new(
            "adb",
            vec![
                "-s".into(),
                device_id.into(),
                "forward".into(),
                "--no-rebind".into(),
                "tcp:0".into(),
                format!("tcp:{listener_port}"),
            ],
        );
        let output = self.executor.output(&command).map_err(RouteError::Command)?;
        if !output.success {
            return Err(RouteError::ForwardRejected(output.diagnostic()));
        }

        let allocated_port = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let host_port = allocated_port
            .parse()
            .map_err(|_| RouteError::InvalidAllocatedPort(allocated_port))?;

        Ok(RouteReservation {
            executor: Arc::clone(&self.executor),
            device_id: device_id.into(),
            host_port,
            listener_port,
        })
    }
}

/// An Android forward owned exclusively by one hot-reload session.
pub struct RouteReservation<E>
where
    E: CommandExecutor,
{
    executor: Arc<E>,
    device_id: String,
    host_port: u16,
    listener_port: u16,
}

impl<E> RouteReservation<E>
where
    E: CommandExecutor,
{
    /// Returns the CLI loopback port allocated by `adb`.
    #[inline]
    pub const fn host_port(&self) -> u16 {
        self.host_port
    }

    /// Recreates this session's exact forward after a device reconnect.
    pub fn reconnect(&self) -> Result<(), RouteError> {
        let command = CommandSpec::new(
            "adb",
            vec![
                "-s".into(),
                self.device_id.clone(),
                "forward".into(),
                "--no-rebind".into(),
                format!("tcp:{}", self.host_port),
                format!("tcp:{}", self.listener_port),
            ],
        );
        let output = self.executor.output(&command).map_err(RouteError::Command)?;
        if !output.success {
            return Err(RouteError::ForwardRejected(output.diagnostic()));
        }
        Ok(())
    }
}

impl<E> Drop for RouteReservation<E>
where
    E: CommandExecutor,
{
    fn drop(&mut self) {
        let command = CommandSpec::new(
            "adb",
            vec![
                "-s".into(),
                self.device_id.clone(),
                "forward".into(),
                "--remove".into(),
                format!("tcp:{}", self.host_port),
            ],
        );
        let _ = self.executor.output(&command);
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::Mutex;

    use aimer_reload_protocol::SessionCredentials;

    use super::super::{CommandOutput, CommandSpec};
    use super::*;

    const SESSION_ID: [u8; 16] = [0x11; 16];
    const TOKEN: [u8; 32] = [0xA5; 32];

    #[derive(Default)]
    struct RecordingExecutor {
        commands: Mutex<Vec<CommandSpec>>,
    }

    impl CommandExecutor for RecordingExecutor {
        fn output(&self, command: &CommandSpec) -> io::Result<CommandOutput> {
            self.commands.lock().unwrap().push(command.clone());
            Ok(CommandOutput {
                success: true,
                stdout: b"43127\n".to_vec(),
                stderr: Vec::new(),
            })
        }
    }

    fn adapter() -> (Arc<RecordingExecutor>, AndroidRouteAdapter<RecordingExecutor>) {
        let executor = Arc::new(RecordingExecutor::default());
        (Arc::clone(&executor), AndroidRouteAdapter::new(executor))
    }

    fn session() -> DevelopmentSession {
        DevelopmentSession::from_parts(SessionCredentials::from_parts(SESSION_ID, TOKEN), 37654)
    }

    fn forward_command() -> CommandSpec {
        CommandSpec::new(
            "adb",
            vec![
                "-s".into(),
                "device-42".into(),
                "forward".into(),
                "--no-rebind".into(),
                "tcp:0".into(),
                "tcp:37654".into(),
            ],
        )
    }

    #[test]
    fn android_route_allocates_a_device_scoped_forward_without_rebinding() {
        let (executor, adapter) = adapter();

        let route = adapter.prepare("device-42", 37654).unwrap();

        assert_eq!(route.host_port(), 43127);
        assert_eq!(*executor.commands.lock().unwrap(), [forward_command()]);
    }

    #[test]
    fn android_route_removes_exactly_its_owned_forward_when_dropped() {
        let (executor, adapter) = adapter();

        let route = adapter.prepare("device-42", 37654).unwrap();
        drop(route);

        assert_eq!(
            *executor.commands.lock().unwrap(),
            [
                forward_command(),
                CommandSpec::new(
                    "adb",
                    vec![
                        "-s".into(),
                        "device-42".into(),
                        "forward".into(),
                        "--remove".into(),
                        "tcp:43127".into(),
                    ],
                ),
            ]
        );
    }

    #[test]
    fn android_route_reconnects_without_rebinding_another_owner() {
        let (executor, adapter) = adapter();
        let route = adapter.prepare("device-42", 37654).unwrap();

        route.reconnect().unwrap();

        assert_eq!(
            *executor.commands.lock().unwrap(),
            [
                forward_command(),
                CommandSpec::new(
                    "adb",
                    vec![
                        "-s".into(),
                        "device-42".into(),
                        "forward".into(),
                        "--no-rebind".into(),
                        "tcp:43127".into(),
                        "tcp:37654".into(),
                    ],
                ),
            ]
        );
    }

    #[test]
    fn android_selects_exactly_one_usable_device() {
        let single = "List of devices attached\nemulator-5554\tdevice product:sdk_gphone64_arm64\n";

        assert_eq!(
            AndroidRouteAdapter::<RecordingExecutor>::select_device(single).unwrap(),
            "emulator-5554"
        );
    }

    #[test]
    fn android_rejects_missing_ambiguous_and_unusable_devices() {
        let empty = "List of devices attached\n\n";
        let ambiguous =
            "List of devices attached\nemulator-5554\tdevice\nZY227H8QLM\tdevice product:raven\n";
        let unauthorized = "List of devices attached\nZY227H8QLM\tunauthorized\n";

        assert!(matches!(
            AndroidRouteAdapter::<RecordingExecutor>::select_device(empty),
            Err(RouteError::NoTarget(TargetFamily::Android))
        ));
        let error = AndroidRouteAdapter::<RecordingExecutor>::select_device(ambiguous).unwrap_err();
        assert!(matches!(
            &error,
            RouteError::AmbiguousTargets { family, candidates }
                if *family == TargetFamily::Android
                    && candidates == &["emulator-5554".to_owned(), "ZY227H8QLM".to_owned()]
        ));
        assert!(matches!(
            AndroidRouteAdapter::<RecordingExecutor>::select_device(unauthorized),
            Err(RouteError::UnusableTarget { id, state })
                if id == "ZY227H8QLM" && state == "unauthorized"
        ));
    }

    #[test]
    fn android_launch_delivers_the_session_through_standard_input_only() {
        let (_executor, adapter) = adapter();

        let launch = adapter.launch("device-42", "com.acme.demo", &session());

        assert_eq!(launch.steps().len(), 2);
        assert_eq!(
            launch.steps()[0].command(),
            &CommandSpec::new(
                "adb",
                vec![
                    "-s".into(),
                    "device-42".into(),
                    "shell".into(),
                    "run-as".into(),
                    "com.acme.demo".into(),
                    "sh".into(),
                    "-c".into(),
                    format!("mkdir -p files && cat > {SESSION_FILE}"),
                ],
            )
        );
        assert_eq!(
            launch.steps()[0].stdin(),
            Some(
                format!(
                    "{}\n{}\n37654\n{}",
                    hex::encode(SESSION_ID),
                    hex::encode(TOKEN),
                    session().host_config().to_text().unwrap(),
                )
                .as_str()
            )
        );
        assert_eq!(
            launch.app().command(),
            &CommandSpec::new(
                "adb",
                vec![
                    "-s".into(),
                    "device-42".into(),
                    "shell".into(),
                    "am".into(),
                    "start".into(),
                    "-n".into(),
                    format!("com.acme.demo/{LAUNCH_ACTIVITY}"),
                ],
            )
        );
        assert!(launch.app().environment().is_empty());
        assert!(launch.app().stdin().is_none());
        assert!(
            !launch
                .public_text()
                .any(|text| text.contains(&hex::encode(TOKEN)))
        );
    }

    #[test]
    fn android_removes_the_provisioned_session_file_when_the_session_ends() {
        let (executor, adapter) = adapter();

        let guard = adapter.session_file_guard("device-42", "com.acme.demo");
        assert!(executor.commands.lock().unwrap().is_empty());
        drop(guard);

        assert_eq!(
            *executor.commands.lock().unwrap(),
            [CommandSpec::new(
                "adb",
                vec![
                    "-s".into(),
                    "device-42".into(),
                    "shell".into(),
                    "run-as".into(),
                    "com.acme.demo".into(),
                    "rm".into(),
                    "-f".into(),
                    SESSION_FILE.into(),
                ],
            )]
        );
    }

    #[test]
    fn the_android_endpoint_is_the_allocated_host_loopback_port() {
        let (_executor, adapter) = adapter();
        let route = adapter.prepare("device-42", 37654).unwrap();

        let endpoint = adapter.endpoint(&route).unwrap();

        assert_eq!(endpoint.target(), TargetFamily::Android);
        assert_eq!(endpoint.address(), "127.0.0.1:43127".parse().unwrap());
    }
}
