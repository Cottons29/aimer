use std::fmt;

use crate::config::{ApplicationRuntime, BuildProfile, ExecutionPolicy, ReloadPolicy};

/// One native target family a development app can run on.
///
/// The family selects the transport adapter, the listener binding rule, and the
/// build configurations a maintained target builder must compile. `Web` is part
/// of the enumeration because the ordinary run pipeline supports it, and the
/// configuration matrix must reject hot reload for it explicitly instead of
/// silently omitting it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TargetFamily {
    Macos,
    Windows,
    Linux,
    IosSimulator,
    IosDevice,
    Android,
    Web,
}

impl TargetFamily {
    /// Every family the run pipeline can select.
    pub const ALL: [Self; 7] = [
        Self::Macos,
        Self::Windows,
        Self::Linux,
        Self::IosSimulator,
        Self::IosDevice,
        Self::Android,
        Self::Web,
    ];

    /// Returns the stable name used in developer diagnostics.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Macos => "macOS",
            Self::Windows => "Windows",
            Self::Linux => "Linux",
            Self::IosSimulator => "iOS Simulator",
            Self::IosDevice => "iOS device",
            Self::Android => "Android",
            Self::Web => "web",
        }
    }

    /// Returns whether this family can host the interpreted guest runtime.
    #[inline]
    pub const fn supports_hot_reload(self) -> bool {
        !matches!(self, Self::Web)
    }

    /// Returns whether the app listener of this family must bind loopback only.
    ///
    /// Every family except a physical iOS device reaches the app through a
    /// loopback or forwarded loopback route. The physical device has no
    /// supported forwarding operation, so its listener uses the authenticated
    /// and encrypted local-network route proven for that target.
    #[inline]
    pub const fn requires_loopback_listener(self) -> bool {
        !matches!(self, Self::IosDevice)
    }
}

impl fmt::Display for TargetFamily {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// Whether a maintained builder must compile one target configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigurationOutcome {
    /// The configuration is supported and must compile.
    Allowed,
    /// The configuration is refused with a stable reason.
    Rejected(&'static str),
}

/// Stable reason a web target cannot host the interpreted guest runtime.
pub const WEB_REJECTION: &str =
    "hot reload requires a native target; the web target already runs WebAssembly";

/// Classifies one target family and resolved execution policy.
///
/// The resolved policy already rejects every forbidden profile and runtime
/// combination, so this classification adds exactly the target dimension the
/// policy cannot see.
pub fn configuration_outcome(
    family: TargetFamily,
    policy: ExecutionPolicy,
) -> ConfigurationOutcome {
    if policy.reload() == ReloadPolicy::HotReload && !family.supports_hot_reload() {
        return ConfigurationOutcome::Rejected(WEB_REJECTION);
    }
    ConfigurationOutcome::Allowed
}

/// Returns every configuration a maintained target builder must compile.
///
/// A target builder that compiles this matrix proves the reload subsystem stays
/// optional: every family builds both native ahead-of-time profiles, and every
/// native family additionally builds the development host with its listener.
pub fn build_matrix() -> Vec<(TargetFamily, ExecutionPolicy)> {
    let native_debug = native_policy(BuildProfile::Debug);
    let native_release = native_policy(BuildProfile::Release);
    let reload = ExecutionPolicy::new(
        BuildProfile::Debug,
        ApplicationRuntime::Wasmi,
        ReloadPolicy::HotReload,
    )
    .expect("the debug interpreted reload configuration is allowed");

    let mut matrix = Vec::with_capacity(TargetFamily::ALL.len() * 3);
    for family in TargetFamily::ALL {
        matrix.push((family, native_debug));
        matrix.push((family, native_release));
        if family.supports_hot_reload() {
            matrix.push((family, reload));
        }
    }
    matrix
}

fn native_policy(profile: BuildProfile) -> ExecutionPolicy {
    ExecutionPolicy::new(
        profile,
        ApplicationRuntime::NativeAot,
        ReloadPolicy::Disabled,
    )
    .expect("native ahead-of-time configurations are allowed")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reload() -> ExecutionPolicy {
        ExecutionPolicy::new(
            BuildProfile::Debug,
            ApplicationRuntime::Wasmi,
            ReloadPolicy::HotReload,
        )
        .unwrap()
    }

    fn native(profile: BuildProfile) -> ExecutionPolicy {
        ExecutionPolicy::new(profile, ApplicationRuntime::NativeAot, ReloadPolicy::Disabled)
            .unwrap()
    }

    #[test]
    fn every_native_family_allows_hot_reload_and_the_web_target_rejects_it() {
        for family in TargetFamily::ALL {
            assert_eq!(
                configuration_outcome(family, native(BuildProfile::Debug)),
                ConfigurationOutcome::Allowed
            );
            assert_eq!(
                configuration_outcome(family, native(BuildProfile::Release)),
                ConfigurationOutcome::Allowed
            );
        }

        for family in TargetFamily::ALL {
            let outcome = configuration_outcome(family, reload());
            if family == TargetFamily::Web {
                assert_eq!(
                    outcome,
                    ConfigurationOutcome::Rejected(
                        "hot reload requires a native target; the web target already runs WebAssembly",
                    )
                );
            } else {
                assert_eq!(outcome, ConfigurationOutcome::Allowed);
            }
        }
    }

    #[test]
    fn the_build_matrix_covers_every_allowed_configuration_exactly_once() {
        let matrix = build_matrix();

        assert_eq!(matrix.len(), TargetFamily::ALL.len() * 2 + 6);
        for (family, policy) in &matrix {
            assert_eq!(
                configuration_outcome(*family, *policy),
                ConfigurationOutcome::Allowed
            );
        }
        for family in TargetFamily::ALL {
            let reload_entries = matrix
                .iter()
                .filter(|(entry, policy)| {
                    *entry == family && policy.reload() == ReloadPolicy::HotReload
                })
                .count();
            assert_eq!(reload_entries, usize::from(family.supports_hot_reload()));
        }
        assert!(
            matrix
                .iter()
                .all(|(_, policy)| policy.runtime() != ApplicationRuntime::Wasmi
                    || policy.reload() == ReloadPolicy::HotReload)
        );
    }

    #[test]
    fn only_the_physical_ios_listener_leaves_loopback() {
        for family in TargetFamily::ALL {
            assert_eq!(
                family.requires_loopback_listener(),
                family != TargetFamily::IosDevice
            );
        }
    }
}
