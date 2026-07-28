use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::commands::assemble::copy_assets_into;
use crate::commands::run::cargo_build::{stream_stderr_as_app_log, stream_stdout_as_app_log};
use crate::commands::run::console::Status;
use crate::commands::run::helpers::{build_log, fail, set_status, spawn_streamed};
use crate::commands::run::pipeline::{Flow, RunContext, Runner};

pub fn find_llvm_ar() -> Option<PathBuf> {
    // 1. Explicit override via environment variable
    if let Ok(path) = env::var("LLVM_AR") {
        let p = PathBuf::from(path);
        if p.is_file() {
            return Some(p);
        }
    }

    // 2. Look for versioned or plain names on PATH
    let candidates = [
        "llvm-ar",
        "llvm-ar-18",
        "llvm-ar-17",
        "llvm-ar-16",
        "llvm-ar-15",
        "llvm-ar-14",
    ];

    if let Ok(path_var) = env::var("PATH") {
        for dir in env::split_paths(&path_var) {
            for name in &candidates {
                let full = dir.join(name);
                if full.is_file() {
                    return Some(full);
                }
            }
        }
    }

    // 3. Fall back to `which`/`where` as a last resort
    let which_cmd = if cfg!(windows) { "where" } else { "which" };
    if let Ok(output) = Command::new(which_cmd).arg("llvm-ar").output() {
        if output.status.success() {
            let found = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .map(str::trim)
                .map(PathBuf::from);
            if let Some(p) = found {
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }

    // 4. Common install locations
    let common_paths = [
        "/usr/bin/llvm-ar",
        "/usr/local/bin/llvm-ar",
        "/opt/homebrew/opt/llvm/bin/llvm-ar",
        "/opt/llvm/bin/llvm-ar",
    ];
    common_paths
        .iter()
        .map(Path::new)
        .find(|p| p.is_file())
        .map(|p| p.to_path_buf())
}

pub fn configure_trunk(command: &mut Command, llvm_ar: &Path) {
    command
        .env("AR_wasm32_unknown_unknown", llvm_ar)
        .env("NO_COLOR", "true");
}

/// The web leg of the unified pipeline.
///
/// There is no separate build stage: `trunk serve` compiles the wasm bundle
/// itself and then keeps serving it, so the pipeline only stages the assets and
/// launches the dev server.
pub struct WebRunner;

impl WebRunner {
    #[inline]
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebRunner {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Runner for WebRunner {
    fn assemble(&mut self, ctx: &RunContext) -> Flow {
        set_status(&ctx.tx, Status::Building(0));

        let artifact = "builds/web";
        if copy_assets_into(artifact).is_err() {
            fail(&ctx.tx, format!("Failed to copy assets into {artifact}"));
            return Flow::Abort;
        }

        Flow::Continue
    }

    fn launch(&mut self, ctx: &RunContext) -> Flow {
        #[cfg(target_os = "macos")]
        let Some(llvm_ar) = find_llvm_ar() else {
            fail(&ctx.tx, "Failed to find llvm-ar".to_string());
            return Flow::Abort;
        };

        set_status(&ctx.tx, Status::Launching);
        build_log(&ctx.tx, "Starting trunk server...");

        let mut trunk = Command::new("trunk");
        #[cfg(target_os = "macos")]
        configure_trunk(&mut trunk, &llvm_ar);
        trunk.arg("serve").current_dir("builds/web");

        if !spawn_streamed(
            trunk,
            &ctx.tx,
            &ctx.current_child,
            "Failed to run trunk serve",
            Status::Error,
            stream_stdout_as_app_log,
            stream_stderr_as_app_log,
        ) {
            return Flow::Abort;
        }

        Flow::Continue
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::run::web::find_llvm_ar;

    #[test]
    fn test_find_llvm_ar() {
        match find_llvm_ar() {
            Some(path) => println!("Found llvm-ar at: {}", path.display()),
            None => println!("llvm-ar not found"),
        }
    }
}
