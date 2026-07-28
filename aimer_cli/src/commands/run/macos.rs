use std::process::{Command, Stdio};

use aimer_utils::log::{JSON_OUTPUT_ENV, JSON_OUTPUT_FLAG};

use crate::commands::run::cargo_build::{
    self, CargoBuildTarget, stream_stderr_as_app_log, stream_stderr_as_build_log,
    stream_stdout_as_app_log, stream_stdout_with_xcode_progress,
};
use crate::commands::run::console::Status;
use crate::commands::run::helpers::{
    build_log, build_streamed, fail, host_arch, set_status, spawn_streamed, stage_assets,
};
use crate::commands::run::pipeline::{Flow, RunContext, Runner};
use crate::commands::run::utilities::resolve_lib_path;

/// The Rust target the macOS app is compiled for.
const RUST_TARGET: &str = "aarch64-apple-darwin";

/// The macOS leg of the unified pipeline: `cargo build` → `xcodebuild` the
/// `.app` → launch the executable inside the bundle.
pub struct MacosRunner;

impl MacosRunner {
    #[inline]
    pub fn new() -> Self {
        Self
    }

    /// The `.app` bundle `xcodebuild` produces for `pkg_name`.
    fn app_path(pkg_name: &str) -> String {
        format!("builds/macos/build/Debug/{}.app", pkg_name)
    }
}

impl Default for MacosRunner {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Runner for MacosRunner {
    fn build(&mut self, ctx: &RunContext) -> Flow {
        set_status(&ctx.tx, Status::Compiling(0));
        build_log(&ctx.tx, "Compiling static library...");

        let Some(status) = cargo_build::spawn_cargo_build(
            &CargoBuildTarget::Darwin,
            &ctx.tx,
            &ctx.current_child,
            ctx.inspector_address,
            ctx.inspector_port,
        ) else {
            return Flow::Abort;
        };

        if !status.success() {
            fail(&ctx.tx, "Cargo build failed.");
            return Flow::Abort;
        }

        Flow::Continue
    }

    fn assemble(&mut self, ctx: &RunContext) -> Flow {
        let lib_name = ctx.pkg_name.replace("-", "_");
        let src_lib = resolve_lib_path(&lib_name, RUST_TARGET, CargoBuildTarget::Darwin);
        let dest_dir = "builds/macos/Libraries";
        let dest_lib = format!("{}/lib{}.a", dest_dir, lib_name);

        std::fs::create_dir_all(dest_dir).unwrap();
        if let Err(e) = std::fs::copy(&src_lib, &dest_lib) {
            build_log(
                &ctx.tx,
                format!("Failed to copy static library: src_lib = {}", src_lib),
            );
            build_log(
                &ctx.tx,
                format!("Failed to copy static library: dest_lib = {}", dest_lib),
            );
            fail(&ctx.tx, format!("Failed to copy static library: {}", e));
            return Flow::Abort;
        }
        build_log(&ctx.tx, format!("Copied static library to {}", dest_lib));

        let arch = host_arch();
        build_log(&ctx.tx, format!("Building Xcode project for {}...", arch));

        let mut xcode_build = Command::new("xcodebuild");
        xcode_build
            .arg("-project")
            .arg(format!("{}.xcodeproj", ctx.pkg_name))
            .arg("-target")
            .arg(&ctx.pkg_name)
            .arg("-configuration")
            .arg("Debug")
            .arg("SYMROOT=build")
            .arg("-arch")
            .arg(arch)
            .current_dir("builds/macos");

        if !build_streamed(
            xcode_build,
            &ctx.tx,
            &ctx.current_child,
            &format!("Failed to build Xcode project, pkg_name = {}", ctx.pkg_name),
            "Xcodebuild failed.",
            stream_stdout_with_xcode_progress,
            stream_stderr_as_build_log,
        ) {
            return Flow::Abort;
        }

        // Bundle the registered assets the same way `aimer assemble` does, so a
        // live run reads them from `Contents/Resources` too.
        stage_assets(
            &ctx.tx,
            &format!("{}/Contents/Resources", Self::app_path(&ctx.pkg_name)),
        );

        Flow::Continue
    }

    fn launch(&mut self, ctx: &RunContext) -> Flow {
        set_status(&ctx.tx, Status::Launching);
        build_log(&ctx.tx, "Launching macOS app...");

        let app_exec_path = format!(
            "{}/Contents/MacOS/{}",
            Self::app_path(&ctx.pkg_name),
            ctx.pkg_name
        );

        // Ask the app for machine readable logs: `aimer_utils::log` then writes one
        // JSON record per event, which the reader threads parse and the console
        // renders with its own colors.
        let mut app_run = Command::new(&app_exec_path);
        app_run
            .arg(JSON_OUTPUT_FLAG)
            .env(JSON_OUTPUT_ENV, "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if !spawn_streamed(
            app_run,
            &ctx.tx,
            &ctx.current_child,
            "Failed to launch macOS app",
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
    use super::*;

    #[test]
    fn app_path_points_at_the_debug_bundle() {
        assert_eq!(
            MacosRunner::app_path("my_app"),
            "builds/macos/build/Debug/my_app.app"
        );
    }
}
