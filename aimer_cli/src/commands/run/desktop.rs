use std::process::{Command, Stdio};

use aimer_utils::log::{JSON_OUTPUT_ENV, JSON_OUTPUT_FLAG};

use crate::commands::assemble;
use crate::commands::run::cargo_build::{
    self, CargoBuildTarget, stream_stderr_as_app_log, stream_stdout_as_app_log,
};
use crate::commands::run::console::Status;
use crate::commands::run::helpers::{ConsoleReporter, build_log, fail, set_status, spawn_streamed};
use crate::commands::run::pipeline::{Flow, RunContext, Runner};
use crate::targets::Targets;

/// The Windows/Linux leg of the unified pipeline: `cargo build --bin` → the
/// shared [`package_desktop`](assemble::package_desktop) step → launch the
/// executable inside the portable bundle.
///
/// One runner serves both platforms: they differ only in the file extension of
/// the executable and in which scaffold files the packaging step copies, which
/// the shared packager already decides from the target.
pub struct DesktopRunner {
    /// Which desktop platform is being run; always the host's own.
    target: Targets,
    /// The bundle the assemble stage produced, reused by the launch stage.
    bundle_path: Option<String>,
}

impl DesktopRunner {
    /// A runner for `target`, which must be the desktop platform of the host.
    #[inline]
    pub fn new(target: Targets) -> Self {
        Self {
            target,
            bundle_path: None,
        }
    }
}

impl Runner for DesktopRunner {
    fn build(&mut self, ctx: &RunContext) -> Flow {
        if let Err(e) = assemble::ensure_host_desktop(self.target) {
            fail(&ctx.tx, format!("{e:#}"));
            return Flow::Abort;
        }

        set_status(&ctx.tx, Status::Compiling(0));
        build_log(&ctx.tx, "Compiling application binary...");

        let Some(status) = cargo_build::spawn_cargo_build(
            &CargoBuildTarget::Desktop {
                bin_name: ctx.pkg_name.clone(),
            },
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
        match assemble::package_desktop(self.target, &ctx.pkg_name, ctx.release, &reporter) {
            Ok(bundle_path) => {
                self.bundle_path = Some(bundle_path);
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
        build_log(&ctx.tx, format!("Launching {} app...", self.target));

        let bundle_path = self.bundle_path.clone().unwrap_or_else(|| {
            assemble::desktop_bundle_path(self.target, &ctx.pkg_name, ctx.release)
        });
        let app_exec_path = format!(
            "{bundle_path}/{}",
            assemble::desktop_exe_name(&ctx.pkg_name)
        );

        // Ask the app for machine readable logs: `aimer_utils::log` then writes
        // one JSON record per event, which the reader threads parse and the
        // console renders with its own colors.
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
            "Failed to launch desktop app",
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
        for target in [Targets::Windows, Targets::Linux] {
            let runner = DesktopRunner::new(target);
            assert_eq!(runner.target, target);
            assert!(runner.bundle_path.is_none());
        }
    }

    #[test]
    fn the_launch_stage_falls_back_to_the_shared_bundle_path() {
        assert_eq!(
            assemble::desktop_bundle_path(Targets::Linux, "my_app", true),
            "builds/linux/bundle/release/my_app"
        );
        assert_eq!(
            assemble::desktop_exe_name("my_app"),
            format!("my_app{}", std::env::consts::EXE_SUFFIX)
        );
    }
}
