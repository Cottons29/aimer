use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use aimer_utils::log::{JSON_OUTPUT_ENV, JSON_OUTPUT_FLAG};
use crossbeam::channel::Sender;

use crate::commands::assemble::{self, IosPlan};
use crate::commands::run::Device;
use crate::commands::run::cargo_build::{
    self, CargoBuildTarget, stream_as_app_log_split_cr, stream_stderr_as_app_log,
    stream_stdout_as_app_log,
};
use crate::commands::run::console::{RunnerEvent, Status};
use crate::commands::run::helpers::{
    ConsoleReporter, build_log, fail, run_to_completion, set_status, spawn_streamed,
};
use crate::commands::run::pipeline::{Flow, RunContext, Runner};

/// The iOS leg of the unified pipeline, shared by the physical device and the
/// simulator: `cargo build` → the shared
/// [`package_ios`](assemble::package_ios) step → install the bundle and launch
/// it by bundle id.
pub struct IosRunner {
    plan: IosPlan,
    /// The `.app` the assemble stage produced, reused by the launch stage.
    app_path: Option<String>,
}

impl IosRunner {
    /// A runner for a physical iOS device.
    #[inline]
    pub fn device() -> Self {
        Self::new(false)
    }

    /// A runner for the iOS Simulator.
    #[inline]
    pub fn simulator() -> Self {
        Self::new(true)
    }

    #[inline]
    fn new(simulator: bool) -> Self {
        Self {
            plan: IosPlan::resolve(simulator),
            app_path: None,
        }
    }

    /// The cargo build target this runner compiles with.
    fn build_target(&self) -> CargoBuildTarget {
        let rust_target = self.plan.rust_target.to_string();
        if self.plan.simulator {
            CargoBuildTarget::IosSim { rust_target }
        } else {
            CargoBuildTarget::Ios { rust_target }
        }
    }

    /// Read `CFBundleIdentifier` out of the built bundle's `Info.plist`.
    fn bundle_id(app_path: &str, tx: &Sender<RunnerEvent>) -> Option<String> {
        let plist_path = format!("{}/Info.plist", app_path);
        let output = match Command::new("plutil")
            .arg("-extract")
            .arg("CFBundleIdentifier")
            .arg("raw")
            .arg(&plist_path)
            .output()
        {
            Ok(output) => output,
            Err(e) => {
                fail(tx, format!("Failed to get bundle id: {}", e));
                return None;
            }
        };

        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

impl Runner for IosRunner {
    fn build(&mut self, ctx: &RunContext) -> Flow {
        set_status(&ctx.tx, Status::Compiling(0));
        build_log(
            &ctx.tx,
            format!("Compiling static library for {}...", self.plan.rust_target),
        );

        let Some(status) = cargo_build::spawn_cargo_build(
            &self.build_target(),
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
        match assemble::package_ios(&ctx.pkg_name, &self.plan, ctx.release, &reporter) {
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

        let app_path = self
            .app_path
            .clone()
            .unwrap_or_else(|| self.plan.app_path(&ctx.pkg_name, ctx.release));

        if !install_app(self.plan.simulator, &ctx.device, &app_path, &ctx.tx) {
            return Flow::Abort;
        }

        let Some(bundle_id) = Self::bundle_id(&app_path, &ctx.tx) else {
            return Flow::Abort;
        };

        if !launch_app(
            self.plan.simulator,
            &ctx.device,
            &bundle_id,
            &ctx.tx,
            &ctx.current_child,
        ) {
            return Flow::Abort;
        }

        Flow::Continue
    }
}

/// Install the freshly built `.app` onto the device or simulator.
fn install_app(simulator: bool, device: &Device, app_path: &str, tx: &Sender<RunnerEvent>) -> bool {
    if simulator {
        build_log(tx, "Installing app on iOS Simulator...");

        let mut install = Command::new("xcrun");
        install.args(["simctl", "install", &device.id, app_path]);

        run_to_completion(
            install,
            tx,
            "Failed to install app",
            "Failed to install on Simulator.",
        )
    } else {
        let device_name = &device.name;
        build_log(tx, format!("Installing app on {} ...", device_name));

        let mut install = Command::new("xcrun");
        install
            .args([
                "devicectl",
                "device",
                "install",
                "app",
                "--device",
                &device.id,
                app_path,
            ])
            .env("TERM", "dumb")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        run_to_completion(
            install,
            tx,
            &format!("Failed to install on {}", device_name),
            &format!("Failed to install on {}", device_name),
        )
    }
}

/// Launch the installed app, streaming its console output back as app logs.
fn launch_app(
    simulator: bool,
    device: &Device,
    bundle_id: &str,
    tx: &Sender<RunnerEvent>,
    current_child_clone: &Arc<Mutex<Option<Child>>>,
) -> bool {
    if simulator {
        build_log(tx, "Launching app on iOS Simulator...");

        let mut launch = Command::new("xcrun");
        launch.args([
            "simctl",
            "launch",
            "--console-pty",
            &device.id,
            bundle_id,
            JSON_OUTPUT_FLAG,
        ]);

        spawn_streamed(
            launch,
            tx,
            current_child_clone,
            "Failed to launch app",
            Status::Error,
            stream_stdout_as_app_log,
            stream_stderr_as_app_log,
        )
    } else {
        build_log(tx, "Launching app on iOS Device...");

        let mut launch = Command::new("xcrun");
        launch
            .args([
                "devicectl",
                "device",
                "process",
                "launch",
                "--terminate-existing",
                "--console",
                "--device",
                &device.id,
                bundle_id,
                "--",
                JSON_OUTPUT_FLAG,
            ])
            .env("TERM", "dumb")
            .env(JSON_OUTPUT_ENV, "1");

        spawn_streamed(
            launch,
            tx,
            current_child_clone,
            "Failed to launch app",
            Status::Idling,
            stream_as_app_log_split_cr,
            stream_as_app_log_split_cr,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_target_distinguishes_the_simulator_from_the_device() {
        assert!(matches!(
            IosRunner::device().build_target(),
            CargoBuildTarget::Ios { .. }
        ));
        assert!(matches!(
            IosRunner::simulator().build_target(),
            CargoBuildTarget::IosSim { .. }
        ));
    }

    #[test]
    fn the_runner_takes_its_paths_from_the_shared_plan() {
        let runner = IosRunner::device();
        assert_eq!(
            runner.plan.app_path("my_app", false),
            "builds/ios/build/Debug-iphoneos/my_app.app"
        );
        assert!(runner.app_path.is_none());
    }
}
