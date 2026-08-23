use std::fmt;

use aimer_reload_protocol::{DevelopmentHostConfig, ProtocolError, SessionCredentials};
use zeroize::Zeroizing;

use crate::config::{ApplicationRuntime, ExecutionPolicy, ReloadPolicy};

/// Private launch variable carrying the public session identifier.
pub const SESSION_ID_VARIABLE: &str = "AIMER_RELOAD_SESSION_ID";
/// Private launch variable carrying the ephemeral session token.
pub const SESSION_TOKEN_VARIABLE: &str = "AIMER_RELOAD_SESSION_TOKEN";
/// Private launch variable carrying the app-listener port owned by this run.
pub const LISTENER_PORT_VARIABLE: &str = "AIMER_RELOAD_LISTENER_PORT";
/// Private launch variable carrying the versioned, secret-free host policy.
pub const HOST_CONFIG_VARIABLE: &str = "AIMER_RELOAD_HOST_CONFIG";
/// Development-only Cargo feature that compiles the app-side reload listener.
pub const HOST_RELOAD_FEATURE: &str = "aimer/wasm-hot-reload";

/// Cargo features added to the host build for one execution policy.
///
/// Native ahead-of-time runs must never compile the listener, so they add no
/// features at all rather than a disabled one.
#[inline]
pub const fn host_features(policy: ExecutionPolicy) -> &'static [&'static str] {
    if requires_reload_session(policy) {
        &[HOST_RELOAD_FEATURE]
    } else {
        &[]
    }
}

/// Returns whether this run compiles a portable guest module.
#[inline]
pub const fn requires_guest_build(policy: ExecutionPolicy) -> bool {
    requires_reload_session(policy)
}

/// Returns whether this run starts a source watcher.
#[inline]
pub const fn requires_watcher(policy: ExecutionPolicy) -> bool {
    requires_reload_session(policy)
}

/// Returns whether this run reserves a target transport route.
#[inline]
pub const fn requires_route(policy: ExecutionPolicy) -> bool {
    requires_reload_session(policy)
}

#[inline]
const fn requires_reload_session(policy: ExecutionPolicy) -> bool {
    matches!(
        (policy.runtime(), policy.reload()),
        (ApplicationRuntime::Wasmi, ReloadPolicy::HotReload)
    )
}

/// The ephemeral identity shared by one CLI invocation and the app it launches.
///
/// The token exists only in this process and in the private launch channel of
/// the target adapter. It is never written to project files, command
/// arguments, or diagnostics, and every rendered value keeps it redacted.
pub struct DevelopmentSession {
    credentials: SessionCredentials,
    listener_port: u16,
    host_config: DevelopmentHostConfig,
}

impl DevelopmentSession {
    /// Creates the session required by the resolved execution policy.
    ///
    /// Returns `Ok(None)` for native ahead-of-time runs, which must not create
    /// a session, watcher, guest build, listener feature, or route. A reload
    /// run draws a fresh 128-bit session identifier and 256-bit token from the
    /// operating-system CSPRNG.
    pub fn for_policy(
        policy: ExecutionPolicy,
        listener_port: u16,
    ) -> Result<Option<Self>, ProtocolError> {
        if !requires_reload_session(policy) {
            return Ok(None);
        }
        Ok(Some(Self {
            credentials: SessionCredentials::generate()?,
            listener_port,
            host_config: DevelopmentHostConfig::cli_safe_profile(),
        }))
    }

    /// Creates a session from fixed credentials for deterministic tests.
    #[inline]
    pub const fn from_parts(credentials: SessionCredentials, listener_port: u16) -> Self {
        Self {
            credentials,
            listener_port,
            host_config: DevelopmentHostConfig::cli_safe_profile(),
        }
    }

    /// Returns the credentials used to authenticate every reload connection.
    #[inline]
    pub const fn credentials(&self) -> &SessionCredentials {
        &self.credentials
    }

    /// Returns the app-listener port owned by this run.
    #[inline]
    pub const fn listener_port(&self) -> u16 {
        self.listener_port
    }

    /// Returns the explicit resource policy generated for the native host.
    #[inline]
    pub const fn host_config(&self) -> DevelopmentHostConfig {
        self.host_config
    }

    /// Enables verbose Widget IR diagnostics in the generated native host policy.
    #[inline]
    pub const fn widget_ir_diagnostics(mut self, enabled: bool) -> Self {
        self.host_config = self.host_config.widget_ir_diagnostics(enabled);
        self
    }

    /// Encodes the session for a target adapter's private launch channel.
    ///
    /// The returned token allocation zeroizes on drop, so callers must pass it
    /// straight into the launch environment and must never place it in process
    /// arguments, logs, or user-visible status text.
    pub fn launch_environment(&self) -> [(&'static str, Zeroizing<String>); 4] {
        let (session_id, token) = self.credentials.launch_environment_hex();
        [
            (SESSION_ID_VARIABLE, session_id),
            (SESSION_TOKEN_VARIABLE, token),
            (
                LISTENER_PORT_VARIABLE,
                Zeroizing::new(self.listener_port.to_string()),
            ),
            (
                HOST_CONFIG_VARIABLE,
                Zeroizing::new(
                    self.host_config
                        .to_text()
                        .expect("the built-in host policy must remain valid"),
                ),
            ),
        ]
    }
}

impl fmt::Debug for DevelopmentSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DevelopmentSession")
            .field("credentials", &self.credentials)
            .field("listener_port", &self.listener_port)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::config::BuildProfile;

    use super::*;

    fn reload_policy() -> ExecutionPolicy {
        ExecutionPolicy::new(
            BuildProfile::Debug,
            ApplicationRuntime::Wasmi,
            ReloadPolicy::HotReload,
        )
        .unwrap()
    }

    fn native_policies() -> [ExecutionPolicy; 2] {
        [
            ExecutionPolicy::new(
                BuildProfile::Debug,
                ApplicationRuntime::NativeAot,
                ReloadPolicy::Disabled,
            )
            .unwrap(),
            ExecutionPolicy::new(
                BuildProfile::Release,
                ApplicationRuntime::NativeAot,
                ReloadPolicy::Disabled,
            )
            .unwrap(),
        ]
    }

    #[test]
    fn native_runs_create_no_session_watcher_guest_build_feature_or_route() {
        for policy in native_policies() {
            assert!(
                DevelopmentSession::for_policy(policy, 37654)
                    .unwrap()
                    .is_none()
            );
            assert!(host_features(policy).is_empty());
            assert!(!requires_guest_build(policy));
            assert!(!requires_watcher(policy));
            assert!(!requires_route(policy));
        }
    }

    #[test]
    fn a_reload_run_owns_a_fresh_session_and_the_listener_feature() {
        let policy = reload_policy();

        let first = DevelopmentSession::for_policy(policy, 37654)
            .unwrap()
            .expect("a reload run creates a session");
        let second = DevelopmentSession::for_policy(policy, 37654)
            .unwrap()
            .expect("a reload run creates a session");

        assert_eq!(host_features(policy), [HOST_RELOAD_FEATURE]);
        assert!(requires_guest_build(policy));
        assert!(requires_watcher(policy));
        assert!(requires_route(policy));
        assert_eq!(first.listener_port(), 37654);
        assert_ne!(
            first.credentials().session_id(),
            second.credentials().session_id()
        );
    }

    #[test]
    fn widget_ir_diagnostics_are_opt_in_and_use_the_private_host_policy() {
        let quiet = DevelopmentSession::for_policy(reload_policy(), 37654)
            .unwrap()
            .unwrap();
        assert!(!quiet.host_config().widget_ir_diagnostics_enabled());

        let verbose = quiet.widget_ir_diagnostics(true);
        assert!(verbose.host_config().widget_ir_diagnostics_enabled());
        let environment = verbose.launch_environment();
        let decoded = DevelopmentHostConfig::from_text(environment[3].1.as_str()).unwrap();
        assert!(decoded.widget_ir_diagnostics_enabled());
    }

    #[test]
    fn launch_injection_carries_the_secret_but_diagnostics_never_do() {
        let session = DevelopmentSession::from_parts(
            SessionCredentials::from_parts([0x11; 16], [0xA5; 32]),
            37654,
        );

        let environment = session.launch_environment();

        assert_eq!(environment[0].0, SESSION_ID_VARIABLE);
        assert_eq!(environment[0].1.as_str(), hex::encode([0x11; 16]));
        assert_eq!(environment[1].0, SESSION_TOKEN_VARIABLE);
        assert_eq!(environment[1].1.as_str(), hex::encode([0xA5; 32]));
        assert_eq!(environment[2].0, LISTENER_PORT_VARIABLE);
        assert_eq!(environment[2].1.as_str(), "37654");
        assert_eq!(environment[3].0, HOST_CONFIG_VARIABLE);
        assert_eq!(
            DevelopmentHostConfig::from_text(environment[3].1.as_str()).unwrap(),
            DevelopmentHostConfig::cli_safe_profile()
        );
        let diagnostic = format!("{session:?}");
        assert!(!diagnostic.contains(&hex::encode([0x11; 16])));
        assert!(!diagnostic.contains(&hex::encode([0xA5; 32])));
        assert!(diagnostic.contains("[REDACTED]"));
    }
}
