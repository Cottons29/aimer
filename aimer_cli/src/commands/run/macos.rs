use std::process::{Command, Stdio};

use aimer_utils::log::{JSON_OUTPUT_ENV, JSON_OUTPUT_FLAG};

use crate::commands::assemble;
use crate::commands::run::cargo_build::{
    self, CargoBuildTarget, stream_stderr_as_app_log, stream_stdout_as_app_log,
};
use crate::commands::run::console::Status;
use crate::commands::run::helpers::{ConsoleReporter, build_log, fail, set_status, spawn_streamed};
use crate::commands::run::pipeline::{Flow, RunContext, Runner};

/// The macOS leg of the unified pipeline: `cargo build` → the shared
/// [`package_macos`](assemble::package_macos) step → launch the executable
/// inside the bundle.
pub struct MacosRunner {
    /// The `.app` the assemble stage produced, reused by the launch stage.
    app_path: Option<String>,
}

impl MacosRunner {
    #[inline]
    pub fn new() -> Self {
        Self { app_path: None }
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
            ctx.release,
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
        let reporter = ConsoleReporter::of(ctx);
        match assemble::package_macos(&ctx.pkg_name, ctx.release, &reporter) {
            Ok(app_path) => {
                self.app_path = Some(app_path);
                Flow::Continue
            }
            Err(e) => {
                fail(&ctx.tx, format!("{e:#}"));
                Flow::Abort
            }
        }
    }

    fn launch(&mut self, ctx: &RunContext) -> Flow {
        set_status(&ctx.tx, Status::Launching);
        build_log(&ctx.tx, "Launching macOS app...");

        let app_path = self
            .app_path
            .clone()
            .unwrap_or_else(|| assemble::macos_app_path(&ctx.pkg_name, ctx.release));
        let app_exec_path = format!("{}/Contents/MacOS/{}", app_path, ctx.pkg_name);

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
    fn a_fresh_runner_has_no_bundle_yet() {
        assert!(MacosRunner::new().app_path.is_none());
    }

    #[test]
    fn the_launch_stage_falls_back_to_the_shared_bundle_path() {
        assert_eq!(
            assemble::macos_app_path("my_app", true),
            "builds/macos/build/Release/my_app.app"
        );
    }
}
