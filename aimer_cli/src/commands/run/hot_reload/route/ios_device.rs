use std::net::ToSocketAddrs;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::super::launch::{LaunchConfiguration, LaunchStep};
use super::super::readiness::ListenerReadiness;
use super::super::session::DevelopmentSession;
use super::super::targets::TargetFamily;
use super::{CommandOutput, CommandSpec, Endpoint, RouteError};

/// Prefix `devicectl` uses to pass a variable to the launched app.
pub const CHILD_ENVIRONMENT_PREFIX: &str = "DEVICECTL_CHILD_";
/// Bonjour service type the development app advertises while it listens.
pub const SERVICE_TYPE: &str = "_aimer-reload._tcp";
/// Private variable that tells the app which service instance to advertise.
pub const SERVICE_NAME_VARIABLE: &str = "AIMER_RELOAD_SERVICE_NAME";

const SERVICE_NAME_DOMAIN: &[u8] = b"AIMER-RELOAD-SERVICE-V1";
const RESOLVED_MARKER: &str = " can be reached at ";

/// Runs a development app on a physical iOS device over the proven LAN route.
///
/// Supported Apple device tooling exposes no operation that forwards an
/// arbitrary host port to an app-owned device port, so the physical-device route
/// is the encrypted local-network route recorded in the transport proof: the app
/// advertises a non-secret Bonjour instance, and the CLI authenticates and
/// encrypts every byte with the session secret it injected privately through
/// `devicectl`.
#[derive(Clone, Debug)]
pub struct IosDeviceRouteAdapter {
    device_id: String,
    bundle_id: String,
}

impl IosDeviceRouteAdapter {
    /// Creates the adapter for one explicitly selected device and bundle.
    #[inline]
    pub fn new(device_id: impl Into<String>, bundle_id: impl Into<String>) -> Self {
        Self {
            device_id: device_id.into(),
            bundle_id: bundle_id.into(),
        }
    }

    /// Derives the advertised instance name from the public session identifier.
    ///
    /// The advertisement must let one CLI find one app among several devices and
    /// apps without publishing anything an attacker can use, so the instance
    /// carries a one-way digest prefix of the session identifier rather than the
    /// identifier itself.
    pub fn service_name(session_id: [u8; 16]) -> String {
        let mut digest = Sha256::new();
        digest.update(SERVICE_NAME_DOMAIN);
        digest.update(session_id);
        format!("Aimer Reload {}", hex::encode(&digest.finalize()[..4]))
    }

    /// Builds the launch step that starts the app with its private session.
    pub fn launch(&self, session: &DevelopmentSession) -> LaunchConfiguration {
        let mut step = LaunchStep::new(CommandSpec::new(
            "xcrun",
            vec![
                "devicectl".into(),
                "device".into(),
                "process".into(),
                "launch".into(),
                "--device".into(),
                self.device_id.clone(),
                "--terminate-existing".into(),
                "--console".into(),
                self.bundle_id.clone(),
            ],
        ));
        for (name, value) in session.launch_environment() {
            step = step.private_environment(format!("{CHILD_ENVIRONMENT_PREFIX}{name}"), value);
        }

        LaunchConfiguration::new(vec![
            step.private_environment(
                format!("{CHILD_ENVIRONMENT_PREFIX}{SERVICE_NAME_VARIABLE}"),
                Zeroizing::new(Self::service_name(*session.credentials().session_id())),
            ),
        ])
    }

    /// Returns the command that resolves the advertised instance.
    pub fn resolution_command(service_name: &str) -> CommandSpec {
        CommandSpec::new(
            "dns-sd",
            vec![
                "-L".into(),
                service_name.to_owned(),
                SERVICE_TYPE.into(),
                "local.".into(),
            ],
        )
    }

    /// Resolves the advertised instance into the endpoint the app announced.
    ///
    /// Resolution output is streamed into `resolutions` because the discovery
    /// tool runs until it is stopped. The advertised port must equal the port the
    /// app announced on its launch console; otherwise another app owns this
    /// instance name and the route is refused.
    pub fn resolve(
        resolutions: &Receiver<String>,
        readiness: &ListenerReadiness,
        service_name: &str,
        timeout: Duration,
    ) -> Result<Endpoint, RouteError> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let line = match resolutions.recv_timeout(remaining) {
                Ok(line) => line,
                Err(RecvTimeoutError::Timeout) => {
                    return Err(RouteError::DiscoveryTimeout {
                        service: service_name.to_owned(),
                        waited: timeout,
                    });
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(RouteError::HostUnreachable(format!(
                        "discovery of '{service_name}' stopped before the app was resolved"
                    )));
                }
            };
            if !line.contains(service_name) {
                continue;
            }
            let Some((host, advertised)) = parse_resolved_endpoint(&line) else {
                continue;
            };
            if advertised != readiness.port() {
                return Err(RouteError::ServiceMismatch {
                    announced: readiness.port(),
                    advertised,
                });
            }

            let address = (host.as_str(), advertised)
                .to_socket_addrs()
                .map_err(|error| {
                    RouteError::HostUnreachable(format!("'{host}' did not resolve: {error}"))
                })?
                .next()
                .ok_or_else(|| {
                    RouteError::HostUnreachable(format!("'{host}' resolved no address"))
                })?;
            return Endpoint::new(TargetFamily::IosDevice, address);
        }
    }

    /// Maps failed device-tooling output to an actionable route diagnostic.
    pub fn diagnose(output: &CommandOutput) -> RouteError {
        let diagnostic = output.diagnostic();
        let text = diagnostic.to_ascii_lowercase();
        if text.contains("lock") {
            RouteError::DeviceLocked
        } else if text.contains("local network")
            || text.contains("localnetwork")
            || text.contains("denied")
        {
            RouteError::LocalNetworkPermissionDenied
        } else if text.contains("pair") || text.contains("trust") {
            RouteError::PairingFailed(diagnostic)
        } else if text.contains("not supported")
            || text.contains("unsupported")
            || text.contains("requires xcode")
        {
            RouteError::UnsupportedTooling(diagnostic)
        } else {
            RouteError::HostUnreachable(diagnostic)
        }
    }
}

/// Parses one resolved Bonjour line into its host and port.
///
/// The discovery tool prints one human-readable resolution per line and keeps
/// running, so unrelated progress lines simply yield `None`.
pub fn parse_resolved_endpoint(line: &str) -> Option<(String, u16)> {
    let (_, resolved) = line.split_once(RESOLVED_MARKER)?;
    let endpoint = resolved
        .split_once(" (")
        .map_or(resolved.trim(), |(endpoint, _)| endpoint.trim());
    let (host, port) = endpoint.rsplit_once(':')?;
    Some((host.trim_end_matches('.').to_owned(), port.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use aimer_reload_protocol::SessionCredentials;

    use super::super::super::session::{
        HOST_CONFIG_VARIABLE, LISTENER_PORT_VARIABLE, SESSION_ID_VARIABLE, SESSION_TOKEN_VARIABLE,
    };
    use super::*;

    const SESSION_ID: [u8; 16] = [0x11; 16];
    const TOKEN: [u8; 32] = [0xA5; 32];
    const DEVICE: &str = "00008120-000A1B2C3D4E5F60";

    fn session() -> DevelopmentSession {
        DevelopmentSession::from_parts(SessionCredentials::from_parts(SESSION_ID, TOKEN), 37654)
    }

    fn failed(stderr: &str) -> CommandOutput {
        CommandOutput {
            success: false,
            stdout: Vec::new(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    #[test]
    fn the_advertised_instance_is_a_one_way_hint_of_the_session() {
        let name = IosDeviceRouteAdapter::service_name(SESSION_ID);

        assert_eq!(name, "Aimer Reload 397143ca");
        assert!(!name.contains(&hex::encode(SESSION_ID)));
        assert_ne!(name, IosDeviceRouteAdapter::service_name([0x22; 16]));
    }

    #[test]
    fn physical_device_launch_uses_the_private_child_environment_of_devicectl() {
        let adapter = IosDeviceRouteAdapter::new(DEVICE, "dev.aimers.demo");

        let launch = adapter.launch(&session());

        assert_eq!(launch.steps().len(), 1);
        assert_eq!(
            launch.app().command(),
            &CommandSpec::new(
                "xcrun",
                vec![
                    "devicectl".into(),
                    "device".into(),
                    "process".into(),
                    "launch".into(),
                    "--device".into(),
                    DEVICE.into(),
                    "--terminate-existing".into(),
                    "--console".into(),
                    "dev.aimers.demo".into(),
                ],
            )
        );
        assert_eq!(
            launch
                .app()
                .environment()
                .iter()
                .map(|(name, value)| (name.clone(), value.to_string()))
                .collect::<Vec<_>>(),
            [
                (
                    format!("{CHILD_ENVIRONMENT_PREFIX}{SESSION_ID_VARIABLE}"),
                    hex::encode(SESSION_ID)
                ),
                (
                    format!("{CHILD_ENVIRONMENT_PREFIX}{SESSION_TOKEN_VARIABLE}"),
                    hex::encode(TOKEN)
                ),
                (
                    format!("{CHILD_ENVIRONMENT_PREFIX}{LISTENER_PORT_VARIABLE}"),
                    "37654".to_owned()
                ),
                (
                    format!("{CHILD_ENVIRONMENT_PREFIX}{HOST_CONFIG_VARIABLE}"),
                    session().host_config().to_text().unwrap()
                ),
                (
                    format!("{CHILD_ENVIRONMENT_PREFIX}{SERVICE_NAME_VARIABLE}"),
                    IosDeviceRouteAdapter::service_name(SESSION_ID)
                ),
            ]
        );
        assert!(
            !launch
                .public_text()
                .any(|text| text.contains(&hex::encode(TOKEN)))
        );
    }

    #[test]
    fn resolution_asks_the_discovery_tool_for_exactly_this_instance() {
        let name = IosDeviceRouteAdapter::service_name(SESSION_ID);

        assert_eq!(
            IosDeviceRouteAdapter::resolution_command(&name),
            CommandSpec::new(
                "dns-sd",
                vec![
                    "-L".into(),
                    name,
                    SERVICE_TYPE.into(),
                    "local.".into(),
                ],
            )
        );
    }

    #[test]
    fn a_resolved_advertisement_becomes_the_authenticated_lan_endpoint() {
        let (sender, receiver) = mpsc::channel();
        let name = IosDeviceRouteAdapter::service_name(SESSION_ID);
        sender.send("Lookup Aimer Reload".to_owned()).unwrap();
        sender
            .send(format!(
                "16:04:11.001 {name}.{SERVICE_TYPE}.local. can be reached at 192.168.7.42:37654 (interface 4)"
            ))
            .unwrap();

        let endpoint = IosDeviceRouteAdapter::resolve(
            &receiver,
            &ListenerReadiness::new(SESSION_ID, 37654, 4711, (1, 0)),
            &name,
            Duration::from_secs(1),
        )
        .unwrap();

        assert_eq!(endpoint.target(), TargetFamily::IosDevice);
        assert_eq!(endpoint.address(), "192.168.7.42:37654".parse().unwrap());
    }

    #[test]
    fn resolution_rejects_a_foreign_advertisement_and_reports_a_timeout() {
        let name = IosDeviceRouteAdapter::service_name(SESSION_ID);
        let readiness = ListenerReadiness::new(SESSION_ID, 37654, 4711, (1, 0));
        let timeout = Duration::from_millis(20);
        let (sender, receiver) = mpsc::channel();
        sender
            .send(format!(
                "16:04:11.001 {name}.{SERVICE_TYPE}.local. can be reached at 192.168.7.42:40000 (interface 4)"
            ))
            .unwrap();

        assert!(matches!(
            IosDeviceRouteAdapter::resolve(&receiver, &readiness, &name, timeout),
            Err(RouteError::ServiceMismatch {
                announced: 37654,
                advertised: 40000,
            })
        ));

        let (_sender, empty) = mpsc::channel();
        assert!(matches!(
            IosDeviceRouteAdapter::resolve(&empty, &readiness, &name, timeout),
            Err(RouteError::DiscoveryTimeout { service, waited })
                if service == name && waited == timeout
        ));
    }

    #[test]
    fn device_tooling_failures_map_to_actionable_diagnostics() {
        assert!(matches!(
            IosDeviceRouteAdapter::diagnose(&failed(
                "The device is locked and must be unlocked to continue."
            )),
            RouteError::DeviceLocked
        ));
        assert!(matches!(
            IosDeviceRouteAdapter::diagnose(&failed(
                "NSLocalNetworkUsageDescription request was denied by the user"
            )),
            RouteError::LocalNetworkPermissionDenied
        ));
        assert!(matches!(
            IosDeviceRouteAdapter::diagnose(&failed("The device is not paired with this host.")),
            RouteError::PairingFailed(_)
        ));
        assert!(matches!(
            IosDeviceRouteAdapter::diagnose(&failed(
                "This operation is not supported by the installed Xcode."
            )),
            RouteError::UnsupportedTooling(_)
        ));
        assert!(matches!(
            IosDeviceRouteAdapter::diagnose(&failed("Could not reach the device.")),
            RouteError::HostUnreachable(_)
        ));
        assert!(
            IosDeviceRouteAdapter::diagnose(&failed("The device is locked."))
                .to_string()
                .contains("unlock it")
        );
    }
}
