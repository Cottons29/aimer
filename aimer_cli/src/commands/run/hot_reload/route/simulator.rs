use std::net::{Ipv4Addr, SocketAddr};

use super::super::launch::{LaunchConfiguration, LaunchStep};
use super::super::readiness::ListenerReadiness;
use super::super::session::DevelopmentSession;
use super::super::targets::TargetFamily;
use super::{CommandSpec, Endpoint, RouteError};

/// Prefix `simctl` uses to pass a variable to the launched app instead of itself.
pub const CHILD_ENVIRONMENT_PREFIX: &str = "SIMCTL_CHILD_";

/// Runs a development app on a booted iOS Simulator over loopback.
///
/// The Simulator shares the host network stack, so the app binds host loopback
/// and needs no forwarding tool. `simctl` documents `SIMCTL_CHILD_` variables as
/// the private channel to the launched process, which keeps the session token
/// out of the argument list that any local user can read.
#[derive(Clone, Debug)]
pub struct SimulatorRouteAdapter {
    device_id: String,
    bundle_id: String,
}

impl SimulatorRouteAdapter {
    /// Creates the adapter for one booted Simulator and installed bundle.
    #[inline]
    pub fn new(device_id: impl Into<String>, bundle_id: impl Into<String>) -> Self {
        Self {
            device_id: device_id.into(),
            bundle_id: bundle_id.into(),
        }
    }

    /// Selects the single booted Simulator from `simctl list devices booted`.
    ///
    /// A development session installs an app and injects a secret, so several
    /// booted Simulators are refused instead of picking one silently.
    pub fn select_booted(listing: &str) -> Result<String, RouteError> {
        let mut booted: Vec<String> = listing
            .lines()
            .filter_map(|line| {
                let head = line.trim().strip_suffix("(Booted)")?.trim_end();
                let identifier = head.strip_suffix(')')?.rsplit_once('(')?.1;
                Some(identifier.to_owned())
            })
            .collect();

        match booted.len() {
            1 => Ok(booted.remove(0)),
            0 => Err(RouteError::NoTarget(TargetFamily::IosSimulator)),
            _ => Err(RouteError::AmbiguousTargets {
                family: TargetFamily::IosSimulator,
                candidates: booted,
            }),
        }
    }

    /// Builds the launch step that starts the app with its private session.
    pub fn launch(&self, session: &DevelopmentSession) -> LaunchConfiguration {
        let mut step = LaunchStep::new(CommandSpec::new(
            "xcrun",
            vec![
                "simctl".into(),
                "launch".into(),
                "--console-pty".into(),
                "--terminate-running-process".into(),
                self.device_id.clone(),
                self.bundle_id.clone(),
            ],
        ));
        for (name, value) in session.launch_environment() {
            step = step.private_environment(format!("{CHILD_ENVIRONMENT_PREFIX}{name}"), value);
        }

        LaunchConfiguration::new(vec![step])
    }

    /// Returns the loopback endpoint the app announced.
    ///
    /// The Simulator shares the host network namespace, so the announced port is
    /// reachable on host loopback without any forwarding step.
    pub fn endpoint(&self, readiness: &ListenerReadiness) -> Result<Endpoint, RouteError> {
        Endpoint::new(
            TargetFamily::IosSimulator,
            SocketAddr::from((Ipv4Addr::LOCALHOST, readiness.port())),
        )
    }
}

#[cfg(test)]
mod tests {
    use aimer_reload_protocol::SessionCredentials;

    use super::super::super::session::{
        HOST_CONFIG_VARIABLE, LISTENER_PORT_VARIABLE, SESSION_ID_VARIABLE, SESSION_TOKEN_VARIABLE,
    };
    use super::*;

    const SESSION_ID: [u8; 16] = [0x11; 16];
    const TOKEN: [u8; 32] = [0xA5; 32];
    const DEVICE: &str = "5D1F0A4E-1D0B-4E7C-9A5D-3F0C21D8E9AB";

    fn session() -> DevelopmentSession {
        DevelopmentSession::from_parts(SessionCredentials::from_parts(SESSION_ID, TOKEN), 37654)
    }

    #[test]
    fn simulator_launch_uses_the_private_child_environment_of_simctl() {
        let adapter = SimulatorRouteAdapter::new(DEVICE, "dev.aimers.demo");

        let launch = adapter.launch(&session());

        assert_eq!(launch.steps().len(), 1);
        assert_eq!(
            launch.app().command(),
            &CommandSpec::new(
                "xcrun",
                vec![
                    "simctl".into(),
                    "launch".into(),
                    "--console-pty".into(),
                    "--terminate-running-process".into(),
                    DEVICE.into(),
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
            ]
        );
        assert!(
            !launch
                .public_text()
                .any(|text| text.contains(&hex::encode(TOKEN)))
        );
    }

    #[test]
    fn the_simulator_endpoint_is_host_loopback_on_the_announced_port() {
        let adapter = SimulatorRouteAdapter::new(DEVICE, "dev.aimers.demo");

        let endpoint = adapter
            .endpoint(&ListenerReadiness::new(SESSION_ID, 43127, 4711, (1, 0)))
            .unwrap();

        assert_eq!(endpoint.target(), TargetFamily::IosSimulator);
        assert_eq!(endpoint.address(), "127.0.0.1:43127".parse().unwrap());
    }

    #[test]
    fn simulator_selection_requires_exactly_one_booted_device() {
        let one = format!(
            "== Devices ==\n-- iOS 27.0 --\n    iPhone 16 Pro ({DEVICE}) (Booted)\n    iPad (0A1B2C3D-4E5F-6071-8293-A4B5C6D7E8F9) (Shutdown)\n"
        );
        let none = "== Devices ==\n-- iOS 27.0 --\n    iPhone 16 Pro (5D1F0A4E-1D0B-4E7C-9A5D-3F0C21D8E9AB) (Shutdown)\n";
        let several = format!(
            "== Devices ==\n-- iOS 27.0 --\n    iPhone 16 Pro ({DEVICE}) (Booted)\n    iPad (0A1B2C3D-4E5F-6071-8293-A4B5C6D7E8F9) (Booted)\n"
        );

        assert_eq!(SimulatorRouteAdapter::select_booted(&one).unwrap(), DEVICE);
        assert!(matches!(
            SimulatorRouteAdapter::select_booted(none),
            Err(RouteError::NoTarget(TargetFamily::IosSimulator))
        ));
        assert!(matches!(
            SimulatorRouteAdapter::select_booted(&several),
            Err(RouteError::AmbiguousTargets { family, candidates })
                if family == TargetFamily::IosSimulator && candidates.len() == 2
        ));
    }
}
