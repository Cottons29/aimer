use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};

use super::super::launch::{LaunchConfiguration, LaunchStep};
use super::super::readiness::ListenerReadiness;
use super::super::session::DevelopmentSession;
use super::super::targets::TargetFamily;
use super::{CommandSpec, Endpoint, RouteError};

/// The loopback address family a desktop session connects through.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LoopbackFamily {
    /// `127.0.0.1`, the default because every supported desktop host has it.
    #[default]
    V4,
    /// `::1`, used when the developer runs an IPv6-only loopback configuration.
    V6,
}

/// Runs a development app on the local desktop over loopback.
///
/// macOS, Windows, and Linux need no external routing tool: the CLI starts the
/// host binary itself, so the session travels through the child environment,
/// which the operating system exposes only to the same user. The app answers on
/// loopback and announces the port it bound through the launch console.
#[derive(Clone, Debug)]
pub struct DesktopRouteAdapter {
    family: TargetFamily,
    binary: PathBuf,
    loopback: LoopbackFamily,
}

impl DesktopRouteAdapter {
    /// Creates the adapter for one desktop family and host binary.
    ///
    /// # Panics
    ///
    /// Panics when `family` is not a desktop family, because the caller then
    /// selected the wrong adapter and no route could be prepared.
    #[inline]
    pub fn new(family: TargetFamily, binary: impl Into<PathBuf>) -> Self {
        assert!(
            matches!(
                family,
                TargetFamily::Macos | TargetFamily::Windows | TargetFamily::Linux
            ),
            "{family} is not a desktop target"
        );
        Self {
            family,
            binary: binary.into(),
            loopback: LoopbackFamily::V4,
        }
    }

    /// Selects the loopback address family of this session.
    #[inline]
    pub const fn loopback(mut self, loopback: LoopbackFamily) -> Self {
        self.loopback = loopback;
        self
    }

    /// Returns the desktop family this adapter drives.
    #[inline]
    pub const fn target(&self) -> TargetFamily {
        self.family
    }

    /// Returns the host binary this adapter launches.
    #[inline]
    pub fn binary(&self) -> &Path {
        &self.binary
    }

    /// Builds the launch step that starts the app with its private session.
    pub fn launch(&self, session: &DevelopmentSession) -> LaunchConfiguration {
        let mut step = LaunchStep::new(CommandSpec::new(
            self.binary.to_string_lossy().into_owned(),
            Vec::new(),
        ));
        for (name, value) in session.launch_environment() {
            step = step.private_environment(name, value);
        }

        LaunchConfiguration::new(vec![step])
    }

    /// Returns the loopback endpoint the app announced.
    pub fn endpoint(&self, readiness: &ListenerReadiness) -> Result<Endpoint, RouteError> {
        let address = match self.loopback {
            LoopbackFamily::V4 => IpAddr::V4(Ipv4Addr::LOCALHOST),
            LoopbackFamily::V6 => IpAddr::V6(Ipv6Addr::LOCALHOST),
        };
        Endpoint::new(self.family, SocketAddr::new(address, readiness.port()))
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

    fn session() -> DevelopmentSession {
        DevelopmentSession::from_parts(SessionCredentials::from_parts(SESSION_ID, TOKEN), 37654)
    }

    fn desktop_families() -> [TargetFamily; 3] {
        [
            TargetFamily::Macos,
            TargetFamily::Windows,
            TargetFamily::Linux,
        ]
    }

    #[test]
    fn desktop_launch_injects_the_session_through_the_private_child_environment() {
        for family in desktop_families() {
            let adapter = DesktopRouteAdapter::new(family, "target/debug/demo");

            let launch = adapter.launch(&session());

            assert_eq!(launch.steps().len(), 1);
            assert_eq!(
                launch.app().command(),
                &CommandSpec::new("target/debug/demo", Vec::new())
            );
            assert_eq!(
                launch
                    .app()
                    .environment()
                    .iter()
                    .map(|(name, value)| (name.as_str(), value.as_str()))
                    .collect::<Vec<_>>(),
                [
                    (SESSION_ID_VARIABLE, hex::encode(SESSION_ID).as_str()),
                    (SESSION_TOKEN_VARIABLE, hex::encode(TOKEN).as_str()),
                    (LISTENER_PORT_VARIABLE, "37654"),
                    (
                        HOST_CONFIG_VARIABLE,
                        session().host_config().to_text().unwrap().as_str(),
                    ),
                ]
            );
            assert!(launch.app().stdin().is_none());
            assert!(
                !launch
                    .public_text()
                    .any(|text| text.contains(&hex::encode(TOKEN)))
            );
        }
    }

    #[test]
    fn desktop_endpoints_use_the_announced_port_on_either_loopback_family() {
        let readiness = ListenerReadiness::new(SESSION_ID, 43127, 4711, (1, 0));

        for family in desktop_families() {
            let ipv4 = DesktopRouteAdapter::new(family, "demo")
                .endpoint(&readiness)
                .unwrap();
            let ipv6 = DesktopRouteAdapter::new(family, "demo")
                .loopback(LoopbackFamily::V6)
                .endpoint(&readiness)
                .unwrap();

            assert_eq!(ipv4.target(), family);
            assert_eq!(ipv4.address(), "127.0.0.1:43127".parse().unwrap());
            assert_eq!(ipv6.address(), "[::1]:43127".parse().unwrap());
        }
    }

    #[test]
    #[should_panic(expected = "Android is not a desktop target")]
    fn a_mobile_family_cannot_use_the_desktop_adapter() {
        DesktopRouteAdapter::new(TargetFamily::Android, "demo");
    }
}
