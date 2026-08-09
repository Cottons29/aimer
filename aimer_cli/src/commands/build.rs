use std::path::Path;
use std::process::Command;

use anyhow::{Context, bail};

use crate::commands::assemble::ensure_host_desktop;
use crate::commands::run::web::{configure_trunk, find_llvm_ar};
use crate::config::{AimerManifest, resolve_package_name};
use crate::errors::AimerError;
use crate::targets::Targets;

/// Non-interactive build entry point used by `aimer build`.
///
/// Resolves the target (CLI flag → manifest default), then runs the
/// appropriate compiler invocation with inherited stdio so it is friendly to
/// CI logs.
pub fn execute(target: Option<String>, release: bool) -> anyhow::Result<()> {
    let target = resolve_target(target)?;

    let mut cmd = build_command(target, release)?;
    println!(
        "Building for target '{target}'{}...",
        if release { " (release)" } else { "" }
    );

    let status = cmd
        .status()
        .with_context(|| format!("failed to start build for target '{target}'"))?;

    if !status.success() {
        bail!("build failed for target '{target}'");
    }

    println!("Build finished successfully for '{target}'.");
    Ok(())
}

/// Resolve the build target from the explicit flag, falling back to the
/// `aimer.toml` default, and finally erroring with guidance.
fn resolve_target(target: Option<String>) -> anyhow::Result<Targets> {
    if let Some(t) = target {
        return Targets::try_from(t.as_str()).map_err(|_| AimerError::UnknownTarget(t).into());
    }

    let manifest_default = AimerManifest::load_from(Path::new("."))
        .ok()
        .flatten()
        .and_then(|m| m.default_target().map(|s| s.to_string()));
    if let Some(default) = manifest_default {
        return Targets::try_from(default.as_str())
            .map_err(|_| AimerError::UnknownTarget(default).into());
    }

    bail!(
        "no target specified; pass --target <macos|windows|linux|android|ios|web> \
         or set [build].default_target in aimer.toml"
    )
}

/// Map a target to its compiler invocation.
fn build_command(target: Targets, release: bool) -> anyhow::Result<Command> {
    let mut cmd = match target {
        Targets::Web => {
            let mut c = Command::new("trunk");

            #[cfg(target_os = "macos")]
            {
                let Some(llvm_ar) = find_llvm_ar() else {
                    bail!("Failed to find llvm-ar".to_string());
                };

                configure_trunk(&mut c, &llvm_ar);
            }
            c.arg("build").current_dir("builds/web");
            if release {
                c.arg("--release");
            }
            c
        }
        Targets::Android | Targets::AndroidSimulator => {
            let mut c = Command::new("cargo");
            c.arg("ndk")
                .arg("-t")
                .arg("arm64-v8a")
                .arg("build")
                .arg("--lib");
            if release {
                c.arg("--release");
            }
            c
        }
        Targets::Macos => {
            let mut c = Command::new("cargo");
            c.arg("build")
                .args(["--target", "aarch64-apple-darwin", "--lib"]);
            if release {
                c.arg("--release");
            }
            c
        }
        Targets::Ios => {
            let mut c = Command::new("cargo");
            c.arg("build").args(["--target", "aarch64-apple-ios", "--lib"]).env("RUSTFLAGS","-C link-arg=-Wl,-U,_aimer_ios_request_frame -C link-arg=-Wl,-U,_aimer_ios_pause_frames");
            if release {
                c.arg("--release");
            }
            c
        }
        Targets::IosSimulator => {
            let mut c = Command::new("cargo");
            c.arg("build").args(["--target", "aarch64-apple-ios-sim", "--lib"]).env("RUSTFLAGS","-C link-arg=-Wl,-U,_aimer_ios_request_frame -C link-arg=-Wl,-U,_aimer_ios_pause_frames");
            if release {
                c.arg("--release");
            }
            c
        }
        Targets::Windows | Targets::Linux => {
            // Host build: the desktop bundle ships a real executable, and
            // cargo's default triple is the one it will be launched on.
            ensure_host_desktop(target)?;
            let mut c = Command::new("cargo");
            c.arg("build")
                .args(["--bin", &resolve_package_name(Path::new("."))]);
            if release {
                c.arg("--release");
            }
            c
        }
        Targets::Terminated => bail!("'terminated' is not a buildable target"),
    };

    // Keep stdio attached to the terminal for CI-friendly output.
    cmd.stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());

    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The arguments `build_command` passes for `target`, as lossy strings.
    fn args_of(target: Targets, release: bool) -> anyhow::Result<Vec<String>> {
        Ok(build_command(target, release)?
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect())
    }

    #[test]
    fn a_desktop_build_asks_cargo_for_the_application_binary() {
        let Some(host) = Targets::host_desktop() else {
            return;
        };
        let args = args_of(host, false).expect("host desktop build");
        assert!(args.contains(&"--bin".to_string()), "{args:?}");
        assert!(!args.contains(&"--lib".to_string()), "{args:?}");
        assert!(!args.contains(&"--release".to_string()), "{args:?}");

        let release = args_of(host, true).expect("host desktop release build");
        assert!(release.contains(&"--release".to_string()), "{release:?}");
    }

    #[test]
    fn a_desktop_build_for_another_os_is_refused() {
        for target in [Targets::Windows, Targets::Linux] {
            if Targets::host_desktop() == Some(target) {
                continue;
            }
            let err = args_of(target, false).expect_err("cross-OS desktop build");
            let message = err.to_string();
            assert!(message.contains(&target.to_string()), "{message}");
            assert!(message.contains(std::env::consts::OS), "{message}");
        }
    }

    #[test]
    fn terminated_is_not_buildable() {
        assert!(build_command(Targets::Terminated, false).is_err());
    }
}
