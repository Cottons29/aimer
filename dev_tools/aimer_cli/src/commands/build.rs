use std::path::Path;
use std::process::Command;

use anyhow::{Context, bail};

use crate::config::AimerManifest;
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
    println!("Building for target '{target}'{}...", if release { " (release)" } else { "" });

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
        .and_then(|m| {
            m.default_target()
                .map(|s| s.to_string())
        });
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
            c.arg("build")
                .current_dir("builds/web");
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
            let mut c = Command::new("cargo");
            c.arg("build").arg("--lib");
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
