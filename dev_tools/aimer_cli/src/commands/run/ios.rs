use crate::commands::run::Device;
use crate::commands::run::cargo_build::{
    self, CargoBuildTarget, stream_as_app_log_split_cr, stream_stderr_as_app_log,
    stream_stderr_as_build_log, stream_stdout_as_app_log, stream_stdout_with_xcode_progress,
    wait_for_child,
};
use crate::commands::run::console::{RunnerEvent, Status};
use crate::commands::run::helpers::{
    build_log, build_streamed, fail, host_arch, run_to_completion, set_status, spawn_streamed,
};
use crate::commands::run::utilities::resolve_lib_path;
use crossbeam::channel::Sender;
use std::fs;
use std::net::IpAddr;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

/// The two flavours of the otherwise-identical iOS build/launch flow.
#[derive(Clone, Copy)]
pub(crate) enum IosVariant {
    Device,
    Simulator,
}

pub fn spawn_ios_runner(
    device: Device,
    pkg_name: String,
    tx: Sender<RunnerEvent>,
    current_child_clone: Arc<Mutex<Option<Child>>>,
    inspector_address: IpAddr,
    inspector_port: u16,
) {
    run_ios(
        IosVariant::Device,
        device,
        pkg_name,
        tx,
        current_child_clone,
        inspector_address,
        inspector_port,
    );
}

/// Shared iOS build → package → launch pipeline used by both the physical
/// device and the simulator runners. Everything that differs between the two is
/// selected from `variant`.
pub(crate) fn run_ios(
    variant: IosVariant,
    device: Device,
    pkg_name: String,
    tx: Sender<RunnerEvent>,
    current_child_clone: Arc<Mutex<Option<Child>>>,
    inspector_address: IpAddr,
    inspector_port: u16,
) {
    let xcode_arch = host_arch();
    let (rust_target, sdk, build_target, debug_subdir) = match variant {
        IosVariant::Device => {
            let rust_target = "aarch64-apple-ios";
            (
                rust_target,
                "iphoneos",
                CargoBuildTarget::Ios { rust_target: rust_target.to_string() },
                "Debug-iphoneos",
            )
        }
        IosVariant::Simulator => {
            let rust_target =
                if xcode_arch == "x86_64" { "x86_64-apple-ios" } else { "aarch64-apple-ios-sim" };
            (
                rust_target,
                "iphonesimulator",
                CargoBuildTarget::IosSim { rust_target: rust_target.to_string() },
                "Debug-iphonesimulator",
            )
        }
    };

    let app_path = format!("builds/ios/build/{}/{}.app", debug_subdir, pkg_name);

    {
        let app = Path::new(&app_path);
        if app.exists() {
            fs::remove_dir_all(app).unwrap();
        }
    }

    set_status(&tx, Status::Compiling(0));
    build_log(&tx, format!("Compiling static library for {}...", rust_target));

    let status = match cargo_build::spawn_cargo_build(
        &build_target,
        &tx,
        &current_child_clone,
        inspector_address,
        inspector_port,
    ) {
        Some(s) => s,
        None => return,
    };

    if !status.success() {
        fail(&tx, "Cargo build failed.");
        return;
    }

    let lib_name = pkg_name.replace("-", "_");
    // let src_lib = format!("target/{}/debug/lib{}.a", rust_target, lib_name);
    let src_lib = resolve_lib_path(
        &lib_name,
        rust_target,
        CargoBuildTarget::Ios { rust_target: rust_target.to_string() },
    );
    let dest_dir = "builds/ios/Libraries";
    let dest_lib = format!("{}/lib{}.a", dest_dir, lib_name);

    fs::create_dir_all(dest_dir).unwrap();
    if let Err(e) = fs::copy(&src_lib, &dest_lib) {
        fail(&tx, format!("Failed to copy static library: {}", e));
        return;
    } else {
        build_log(&tx, format!("Copied static library to {}", dest_lib));
    }

    build_log(&tx, "Building Xcode project for iOS...");

    let mut xcode_build = Command::new("xcodebuild");
    xcode_build
        .arg("-project")
        .arg(format!("{}.xcodeproj", pkg_name))
        .arg("-target")
        .arg(&pkg_name)
        .arg("-configuration")
        .arg("Debug")
        .arg("-sdk")
        .arg(sdk)
        .arg("SYMROOT=build")
        .arg("-arch")
        .arg(xcode_arch)
        .current_dir("builds/ios");

    if !build_streamed(
        xcode_build,
        &tx,
        &current_child_clone,
        "Failed to build Xcode project",
        "Xcodebuild failed.",
        stream_stdout_with_xcode_progress,
        stream_stderr_as_build_log,
    ) {
        return;
    }

    set_status(&tx, Status::Launching);

    if crate::commands::assets::copy_assets_into(&app_path).is_err() {
        fail(&tx, format!("Failed to copy assets into {}", app_path));
        return;
    };

    if !install_app(variant, &device, &app_path, &tx) {
        return;
    }

    let plist_path = format!("{}/Info.plist", app_path);
    let bundle_id_output = match Command::new("plutil")
        .arg("-extract")
        .arg("CFBundleIdentifier")
        .arg("raw")
        .arg(&plist_path)
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            fail(&tx, format!("Failed to get bundle id: {}", e));
            return;
        }
    };

    let bundle_id = String::from_utf8_lossy(&bundle_id_output.stdout).trim().to_string();

    if !launch_app(variant, &device, &bundle_id, &tx, &current_child_clone) {
        return;
    }

    set_status(&tx, Status::Running);

    wait_for_child(&current_child_clone);

    set_status(&tx, Status::Idling);
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
                .args(["devicectl", "device", "install", "app", "--device", &device.id, app_path])
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
                ])
                .env("TERM", "dumb");

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
            launch.args(["simctl", "launch", "--console-pty", &device.id, bundle_id]);

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
