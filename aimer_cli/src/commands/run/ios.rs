use std::fs;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use aimer_utils::log::{JSON_OUTPUT_ENV, JSON_OUTPUT_FLAG};
use crossbeam::channel::Sender;

use crate::commands::run::Device;
use crate::commands::run::cargo_build::{
    self, CargoBuildTarget, stream_as_app_log_split_cr, stream_stderr_as_app_log,
    stream_stderr_as_build_log, stream_stdout_as_app_log, stream_stdout_with_xcode_progress,
};
use crate::commands::run::console::{RunnerEvent, Status};
use crate::commands::run::helpers::{
    build_log, build_streamed, fail, host_arch, run_to_completion, set_status, spawn_streamed,
    stage_assets,
};
use crate::commands::run::pipeline::{Flow, RunContext, Runner};
use crate::commands::run::utilities::resolve_lib_path;

/// The two flavours of the otherwise-identical iOS pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IosVariant {
    Device,
    Simulator,
}

/// Everything that differs between an iOS device build and a simulator build,
/// resolved once from the variant and the host architecture.
struct IosPlan {
    /// Rust target triple the static library is compiled for.
    rust_target: &'static str,
    /// `xcodebuild -sdk` value.
    sdk: &'static str,
    /// `xcodebuild -arch` value.
    arch: &'static str,
    /// Subdirectory of `builds/ios/build` the `.app` lands in.
    build_subdir: &'static str,
}

impl IosPlan {
    /// Resolve the plan for `variant` on this host.
    fn resolve(variant: IosVariant) -> Self {
        let arch = host_arch();
        match variant {
            IosVariant::Device => Self {
                rust_target: "aarch64-apple-ios",
                sdk: "iphoneos",
                arch,
                build_subdir: "Debug-iphoneos",
            },
            IosVariant::Simulator => Self {
                rust_target: if arch == "x86_64" {
                    "x86_64-apple-ios"
                } else {
                    "aarch64-apple-ios-sim"
                },
                sdk: "iphonesimulator",
                arch,
                build_subdir: "Debug-iphonesimulator",
            },
        }
    }

    /// The cargo build target this plan compiles with.
    fn build_target(&self, variant: IosVariant) -> CargoBuildTarget {
        let rust_target = self.rust_target.to_string();
        match variant {
            IosVariant::Device => CargoBuildTarget::Ios { rust_target },
            IosVariant::Simulator => CargoBuildTarget::IosSim { rust_target },
        }
    }

    /// The `.app` bundle `xcodebuild` produces for `pkg_name`.
    fn app_path(&self, pkg_name: &str) -> String {
        format!("builds/ios/build/{}/{}.app", self.build_subdir, pkg_name)
    }
}

/// The iOS leg of the unified pipeline, shared by the physical device and the
/// simulator: `cargo build` → `xcodebuild` the `.app` → install it and launch
/// it by bundle id.
pub struct IosRunner {
    variant: IosVariant,
    plan: IosPlan,
}

impl IosRunner {
    /// A runner for a physical iOS device.
    #[inline]
    pub fn device() -> Self {
        Self::new(IosVariant::Device)
    }

    /// A runner for the iOS Simulator.
    #[inline]
    pub fn simulator() -> Self {
        Self::new(IosVariant::Simulator)
    }

    #[inline]
    fn new(variant: IosVariant) -> Self {
        Self {
            variant,
            plan: IosPlan::resolve(variant),
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

        Some(
            String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_string(),
        )
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
            &self.plan.build_target(self.variant),
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
        let app_path = self.plan.app_path(&ctx.pkg_name);

        // Start from a clean bundle so a stale executable or asset can never be
        // launched when xcodebuild decides it has nothing to do.
        let app = Path::new(&app_path);
        if app.exists() && let Err(e) = fs::remove_dir_all(app) {
            fail(&ctx.tx, format!("Failed to clean {}: {}", app_path, e));
            return Flow::Abort;
        }

        let lib_name = ctx.pkg_name.replace("-", "_");
        let src_lib = resolve_lib_path(
            &lib_name,
            self.plan.rust_target,
            self.plan.build_target(self.variant),
        );
        let dest_dir = "builds/ios/Libraries";
        let dest_lib = format!("{}/lib{}.a", dest_dir, lib_name);

        fs::create_dir_all(dest_dir).unwrap();
        if let Err(e) = fs::copy(&src_lib, &dest_lib) {
            fail(&ctx.tx, format!("Failed to copy static library: {}", e));
            return Flow::Abort;
        }
        build_log(&ctx.tx, format!("Copied static library to {}", dest_lib));

        build_log(&ctx.tx, "Building Xcode project for iOS...");

        let mut xcode_build = Command::new("xcodebuild");
        xcode_build
            .arg("-project")
            .arg(format!("{}.xcodeproj", ctx.pkg_name))
            .arg("-target")
            .arg(&ctx.pkg_name)
            .arg("-configuration")
            .arg("Debug")
            .arg("-sdk")
            .arg(self.plan.sdk)
            .arg("SYMROOT=build")
            .arg("-arch")
            .arg(self.plan.arch)
            .current_dir("builds/ios");

        if !build_streamed(
            xcode_build,
            &ctx.tx,
            &ctx.current_child,
            "Failed to build Xcode project",
            "Xcodebuild failed.",
            stream_stdout_with_xcode_progress,
            stream_stderr_as_build_log,
        ) {
            return Flow::Abort;
        }

        stage_assets(&ctx.tx, &app_path);

        Flow::Continue
    }

    fn launch(&mut self, ctx: &RunContext) -> Flow {
        set_status(&ctx.tx, Status::Launching);

        let app_path = self.plan.app_path(&ctx.pkg_name);

        if !install_app(self.variant, &ctx.device, &app_path, &ctx.tx) {
            return Flow::Abort;
        }

        let Some(bundle_id) = Self::bundle_id(&app_path, &ctx.tx) else {
            return Flow::Abort;
        };

        if !launch_app(
            self.variant,
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
fn install_app(
    variant: IosVariant,
    device: &Device,
    app_path: &str,
    tx: &Sender<RunnerEvent>,
) -> bool {
    match variant {
        IosVariant::Device => {
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
        IosVariant::Simulator => {
            build_log(tx, "Installing app on iOS Simulator...");

            let mut install = Command::new("xcrun");
            install.args(["simctl", "install", &device.id, app_path]);

            run_to_completion(
                install,
                tx,
                "Failed to install app",
                "Failed to install on Simulator.",
            )
        }
    }
}

/// Launch the installed app, streaming its console output back as app logs.
fn launch_app(
    variant: IosVariant,
    device: &Device,
    bundle_id: &str,
    tx: &Sender<RunnerEvent>,
    current_child_clone: &Arc<Mutex<Option<Child>>>,
) -> bool {
    match variant {
        IosVariant::Device => {
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
                    // Arguments after the bundle id are forwarded to the app, so
                    // it logs JSON records the console can parse and colorize.
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
        IosVariant::Simulator => {
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_plan_targets_the_iphoneos_sdk() {
        let plan = IosPlan::resolve(IosVariant::Device);
        assert_eq!(plan.rust_target, "aarch64-apple-ios");
        assert_eq!(plan.sdk, "iphoneos");
        assert_eq!(plan.build_subdir, "Debug-iphoneos");
    }

    #[test]
    fn simulator_plan_follows_the_host_architecture() {
        let plan = IosPlan::resolve(IosVariant::Simulator);
        assert_eq!(plan.sdk, "iphonesimulator");
        assert_eq!(plan.build_subdir, "Debug-iphonesimulator");
        let expected = if plan.arch == "x86_64" {
            "x86_64-apple-ios"
        } else {
            "aarch64-apple-ios-sim"
        };
        assert_eq!(plan.rust_target, expected);
    }

    #[test]
    fn build_target_distinguishes_the_simulator_from_the_device() {
        let device = IosRunner::device();
        assert!(matches!(
            device.plan.build_target(device.variant),
            CargoBuildTarget::Ios { .. }
        ));

        let simulator = IosRunner::simulator();
        assert!(matches!(
            simulator.plan.build_target(simulator.variant),
            CargoBuildTarget::IosSim { .. }
        ));
    }

    #[test]
    fn app_path_lives_under_the_variant_build_subdir() {
        assert_eq!(
            IosPlan::resolve(IosVariant::Device).app_path("my_app"),
            "builds/ios/build/Debug-iphoneos/my_app.app"
        );
        assert_eq!(
            IosPlan::resolve(IosVariant::Simulator).app_path("my_app"),
            "builds/ios/build/Debug-iphonesimulator/my_app.app"
        );
    }
}
