use std::process::Command;

/// A rustup target triple the CLI compiles with, and the Aimer platform(s) that
/// require it.
pub struct RustTarget {
    /// The rustup target triple, e.g. `aarch64-linux-android`.
    pub triple: &'static str,
    /// Human-readable platform(s) needing it, e.g. `"android (arm64-v8a)"`.
    pub required_by: &'static str,
}

/// Every target triple the CLI can ask cargo for on this host.
///
/// This table is the single place the triples are *reported* from; the
/// invocations themselves still name them at their use sites —
/// [`MACOS_RUST_TARGET`](crate::commands::assemble::MACOS_RUST_TARGET),
/// [`IosPlan::resolve`](crate::commands::assemble::IosPlan::resolve),
/// [`AndroidPlan::for_abi`](crate::commands::assemble::AndroidPlan::for_abi)
/// and the `trunk` web path — so any triple added there belongs here too.
///
/// Apple triples are gated behind `target_vendor = "apple"` so a Linux or
/// Windows user is never told to install a target they cannot build for.
/// Windows and Linux desktop need no entry at all: they are host builds and use
/// the triple the toolchain already ships with.
pub const RUST_TARGETS: &[RustTarget] = &[
    RustTarget {
        triple: "wasm32-unknown-unknown",
        required_by: "web",
    },
    RustTarget {
        triple: "aarch64-linux-android",
        required_by: "android (arm64-v8a)",
    },
    RustTarget {
        triple: "armv7-linux-androideabi",
        required_by: "android (armeabi-v7a)",
    },
    RustTarget {
        triple: "i686-linux-android",
        required_by: "android (x86)",
    },
    RustTarget {
        triple: "x86_64-linux-android",
        required_by: "android (x86_64)",
    },
    #[cfg(target_vendor = "apple")]
    RustTarget {
        triple: "aarch64-apple-darwin",
        required_by: "macos",
    },
    #[cfg(target_vendor = "apple")]
    RustTarget {
        triple: "aarch64-apple-ios",
        required_by: "ios (device)",
    },
    #[cfg(target_vendor = "apple")]
    RustTarget {
        triple: "aarch64-apple-ios-sim",
        required_by: "ios (simulator)",
    },
    #[cfg(all(target_vendor = "apple", target_arch = "x86_64"))]
    RustTarget {
        triple: "x86_64-apple-ios",
        required_by: "ios (intel simulator)",
    },
];

/// Ask rustup which targets are installed.
///
/// Returns `None` when rustup is not on the PATH or fails, in which case the
/// caller skips the whole section rather than reporting every target as
/// missing.
pub fn installed_targets() -> Option<String> {
    let output = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// Parse the output of `rustup target list --installed` into triples.
///
/// Blank lines, CRLF endings, surrounding whitespace and the `(installed)`
/// suffix some rustup versions append are all tolerated.
///
/// # Examples
///
/// ```ignore
/// let installed = parse_installed("aarch64-apple-darwin\r\nwasm32-unknown-unknown (installed)\n");
/// assert_eq!(installed, ["aarch64-apple-darwin", "wasm32-unknown-unknown"]);
/// ```
pub fn parse_installed(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|triple| !triple.is_empty())
        .collect()
}

/// The one-line remediation command for `missing`, or `None` when nothing is
/// missing.
///
/// # Examples
///
/// ```ignore
/// assert_eq!(
///     install_hint(&["aarch64-linux-android"]).as_deref(),
///     Some("rustup target add aarch64-linux-android")
/// );
/// assert_eq!(install_hint(&[]), None);
/// ```
pub fn install_hint(missing: &[&str]) -> Option<String> {
    if missing.is_empty() {
        return None;
    }
    Some(format!("rustup target add {}", missing.join(" ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A realistic payload, with the quirks the parser has to survive.
    const PAYLOAD: &str = "aarch64-apple-darwin\r\n\nwasm32-unknown-unknown (installed)\n  aarch64-linux-android  \n";

    #[test]
    fn parsing_recovers_every_triple() {
        assert_eq!(
            parse_installed(PAYLOAD),
            vec![
                "aarch64-apple-darwin",
                "wasm32-unknown-unknown",
                "aarch64-linux-android",
            ]
        );
    }

    #[test]
    fn parsing_nothing_yields_nothing() {
        assert!(parse_installed("").is_empty());
        assert!(parse_installed("\n\n   \n").is_empty());
    }

    #[test]
    fn the_missing_set_is_the_difference_against_the_table() {
        let installed = parse_installed(PAYLOAD);
        let missing: Vec<&str> = RUST_TARGETS
            .iter()
            .map(|t| t.triple)
            .filter(|triple| !installed.contains(triple))
            .collect();

        assert!(!missing.contains(&"wasm32-unknown-unknown"));
        assert!(missing.contains(&"x86_64-linux-android"));
        #[cfg(target_vendor = "apple")]
        assert!(!missing.contains(&"aarch64-apple-darwin"));
    }

    #[test]
    fn the_install_hint_names_every_missing_triple() {
        assert_eq!(
            install_hint(&["aarch64-linux-android", "i686-linux-android"]).as_deref(),
            Some("rustup target add aarch64-linux-android i686-linux-android")
        );
        assert_eq!(install_hint(&[]), None);
    }

    #[test]
    fn the_table_lists_every_triple_once() {
        let mut seen = std::collections::HashSet::new();
        for target in RUST_TARGETS {
            assert!(seen.insert(target.triple), "duplicate {}", target.triple);
            assert!(!target.required_by.is_empty(), "{}", target.triple);
        }
    }

    #[cfg(target_vendor = "apple")]
    #[test]
    fn the_table_covers_every_apple_triple_the_cli_builds_with() {
        use crate::commands::assemble::{IosPlan, MACOS_RUST_TARGET};

        let triples: Vec<&str> = RUST_TARGETS.iter().map(|t| t.triple).collect();
        assert!(triples.contains(&MACOS_RUST_TARGET));
        for simulator in [false, true] {
            let plan = IosPlan::resolve(simulator);
            assert!(triples.contains(&plan.rust_target), "{}", plan.rust_target);
        }
    }
}
