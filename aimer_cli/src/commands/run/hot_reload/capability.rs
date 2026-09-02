 use std::fmt;

use sha2::{Digest, Sha256};

use super::watch::ChangeImpact;

const MANIFEST_DOMAIN: &[u8] = b"AIMER-CAPABILITY-MANIFEST-V1";

/// One host capability contract required by a portable guest program.
///
/// The fingerprint is the canonical digest of the generated wire contract, so
/// two builds agree on a capability only when the host and guest were compiled
/// against the exact same request/response shape.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityContract {
    name: String,
    abi_major: u32,
    fingerprint: [u8; 32],
}

impl CapabilityContract {
    /// Creates one contract entry from generated capability metadata.
    #[inline]
    pub fn new(name: impl Into<String>, abi_major: u32, fingerprint: [u8; 32]) -> Self {
        Self {
            name: name.into(),
            abi_major,
            fingerprint,
        }
    }

    /// Returns the stable capability name used in developer diagnostics.
    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// A capability set that could not be canonicalized.
#[derive(Debug, Eq, PartialEq)]
pub struct CapabilityError(String);

impl fmt::Display for CapabilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for CapabilityError {}

/// The canonical capability contract set of one application build.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CapabilityManifest {
    contracts: Vec<CapabilityContract>,
}

impl CapabilityManifest {
    /// Creates a manifest whose entries are sorted by capability name.
    ///
    /// Duplicate names are rejected instead of being merged, because a build
    /// that declares one capability twice cannot produce a single authoritative
    /// contract for the host to bind.
    pub fn new(mut contracts: Vec<CapabilityContract>) -> Result<Self, CapabilityError> {
        contracts.sort_unstable_by(|left, right| left.name.cmp(&right.name));
        if let Some(duplicate) = contracts
            .windows(2)
            .find(|pair| pair[0].name == pair[1].name)
        {
            return Err(CapabilityError(format!(
                "capability '{}' is declared more than once",
                duplicate[0].name
            )));
        }
        Ok(Self { contracts })
    }

    /// Returns the canonical digest announced to the app before any upload.
    pub fn digest(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(MANIFEST_DOMAIN);
        hasher.update((self.contracts.len() as u64).to_le_bytes());
        for contract in &self.contracts {
            hasher.update((contract.name.len() as u64).to_le_bytes());
            hasher.update(contract.name.as_bytes());
            hasher.update(contract.abi_major.to_le_bytes());
            hasher.update(contract.fingerprint);
        }
        hasher.finalize().into()
    }

    /// Reports every contract difference against the previously built manifest.
    ///
    /// Differences are returned in capability-name order so a diagnostic stays
    /// identical across runs of the same edit.
    pub fn changes_since(&self, previous: &Self) -> Vec<String> {
        let mut changes = Vec::new();
        let mut current = self.contracts.iter().peekable();
        let mut earlier = previous.contracts.iter().peekable();
        loop {
            match (current.peek(), earlier.peek()) {
                (None, None) => break,
                (Some(added), None) => {
                    changes.push(format!("capability '{}' was added", added.name));
                    current.next();
                }
                (None, Some(removed)) => {
                    changes.push(format!("capability '{}' was removed", removed.name));
                    earlier.next();
                }
                (Some(left), Some(right)) if left.name < right.name => {
                    changes.push(format!("capability '{}' was added", left.name));
                    current.next();
                }
                (Some(left), Some(right)) if left.name > right.name => {
                    changes.push(format!("capability '{}' was removed", right.name));
                    earlier.next();
                }
                (Some(left), Some(right)) => {
                    if left.abi_major != right.abi_major {
                        changes.push(format!(
                            "capability '{}' now requires host ABI {} instead of {}",
                            left.name, left.abi_major, right.abi_major
                        ));
                    } else if left.fingerprint != right.fingerprint {
                        changes.push(format!(
                            "capability '{}' contract fingerprint changed",
                            left.name
                        ));
                    }
                    current.next();
                    earlier.next();
                }
            }
        }
        changes
    }
}

/// The canonical inputs of one hot-reload build.
///
/// Guest and native dependency digests are kept apart so a dependency edit can
/// be classified without rebuilding: only portable guest inputs can be replaced
/// in the running process, while native provider code is compiled into the
/// permanent host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildInputs {
    guest_dependencies: [u8; 32],
    native_dependencies: [u8; 32],
    capabilities: CapabilityManifest,
}

impl BuildInputs {
    /// Creates the resolved inputs of one build.
    #[inline]
    pub const fn new(
        guest_dependencies: [u8; 32],
        native_dependencies: [u8; 32],
        capabilities: CapabilityManifest,
    ) -> Self {
        Self {
            guest_dependencies,
            native_dependencies,
            capabilities,
        }
    }

    /// Returns the capability digest announced to the app before an upload.
    #[inline]
    pub fn capability_digest(&self) -> [u8; 32] {
        self.capabilities.digest()
    }

    /// Classifies what changed since the previously resolved build inputs.
    pub fn compare(&self, previous: &Self) -> DependencyVerdict {
        let mut reasons = self.capabilities.changes_since(&previous.capabilities);
        if self.native_dependencies != previous.native_dependencies {
            reasons.push("native provider dependencies changed".to_owned());
        }
        if !reasons.is_empty() {
            return DependencyVerdict {
                impact: ChangeImpact::RestartNativeHost,
                reason: Some(reasons.join("; ")),
            };
        }
        if self.guest_dependencies != previous.guest_dependencies {
            return DependencyVerdict {
                impact: ChangeImpact::RebuildGuest,
                reason: None,
            };
        }
        DependencyVerdict {
            impact: ChangeImpact::Ignored,
            reason: None,
        }
    }
}

/// The classification of one dependency or capability contract change.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DependencyVerdict {
    impact: ChangeImpact,
    reason: Option<String>,
}

impl DependencyVerdict {
    /// Returns the effect this change has on the running application.
    #[inline]
    pub const fn impact(&self) -> ChangeImpact {
        self.impact
    }

    /// Returns why the native application must restart, when it must.
    #[inline]
    pub fn restart_reason(&self) -> Option<&str> {
        match self.impact {
            ChangeImpact::RestartNativeHost => self.reason.as_deref(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(contracts: Vec<CapabilityContract>) -> CapabilityManifest {
        CapabilityManifest::new(contracts).unwrap()
    }

    fn haptics() -> CapabilityContract {
        CapabilityContract::new("haptics", 1, [0x11; 32])
    }

    fn clipboard() -> CapabilityContract {
        CapabilityContract::new("clipboard", 1, [0x22; 32])
    }

    fn inputs(
        guest: [u8; 32],
        native: [u8; 32],
        capabilities: CapabilityManifest,
    ) -> BuildInputs {
        BuildInputs::new(guest, native, capabilities)
    }

    #[test]
    fn a_capability_manifest_is_canonical_and_rejects_duplicates() {
        let ordered = manifest(vec![clipboard(), haptics()]);
        let reversed = manifest(vec![haptics(), clipboard()]);

        assert_eq!(ordered.digest(), reversed.digest());
        assert_ne!(ordered.digest(), manifest(vec![haptics()]).digest());
        assert_ne!(
            ordered.digest(),
            manifest(vec![
                clipboard(),
                CapabilityContract::new("haptics", 2, [0x11; 32]),
            ])
            .digest()
        );
        assert_eq!(
            CapabilityManifest::new(vec![haptics(), haptics()]).unwrap_err(),
            CapabilityError("capability 'haptics' is declared more than once".to_owned())
        );
    }

    #[test]
    fn portable_guest_dependency_changes_only_rebuild_the_guest() {
        let previous = inputs([1; 32], [2; 32], manifest(vec![haptics()]));
        let current = inputs([9; 32], [2; 32], manifest(vec![haptics()]));

        let unchanged = previous.compare(&previous);
        let verdict = current.compare(&previous);

        assert_eq!(unchanged.impact(), ChangeImpact::Ignored);
        assert_eq!(unchanged.restart_reason(), None);
        assert_eq!(verdict.impact(), ChangeImpact::RebuildGuest);
        assert_eq!(verdict.restart_reason(), None);
    }

    #[test]
    fn native_provider_and_contract_changes_require_a_native_restart() {
        let previous = inputs([1; 32], [2; 32], manifest(vec![clipboard(), haptics()]));

        let native_only = inputs([1; 32], [7; 32], manifest(vec![clipboard(), haptics()]))
            .compare(&previous);
        let contract_changed = inputs(
            [1; 32],
            [2; 32],
            manifest(vec![
                clipboard(),
                CapabilityContract::new("haptics", 1, [0xEE; 32]),
            ]),
        )
        .compare(&previous);
        let abi_changed = inputs(
            [1; 32],
            [2; 32],
            manifest(vec![
                clipboard(),
                CapabilityContract::new("haptics", 2, [0x11; 32]),
            ]),
        )
        .compare(&previous);
        let removed = inputs([1; 32], [2; 32], manifest(vec![clipboard()])).compare(&previous);
        let added = inputs(
            [1; 32],
            [2; 32],
            manifest(vec![
                clipboard(),
                haptics(),
                CapabilityContract::new("storage", 1, [0x33; 32]),
            ]),
        )
        .compare(&previous);

        assert_eq!(native_only.impact(), ChangeImpact::RestartNativeHost);
        assert_eq!(
            native_only.restart_reason(),
            Some("native provider dependencies changed")
        );
        assert_eq!(
            contract_changed.restart_reason(),
            Some("capability 'haptics' contract fingerprint changed")
        );
        assert_eq!(
            abi_changed.restart_reason(),
            Some("capability 'haptics' now requires host ABI 2 instead of 1")
        );
        assert_eq!(
            removed.restart_reason(),
            Some("capability 'haptics' was removed")
        );
        assert_eq!(
            added.restart_reason(),
            Some("capability 'storage' was added")
        );
    }
}
