use crate::commands::run::Device;
use crate::commands::run::cargo_build::{
    self, CargoBuildTarget, stream_as_app_log_split_cr, stream_stderr_as_build_log, stream_stdout_with_xcode_progress, wait_for_child,
};
use crate::commands::run::console::{RunnerEvent, Status};
use crossbeam::channel::Sender;
use std::net::IpAddr;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

pub fn spawn_ios_runner(
    device: Device,
    pkg_name: String,
    tx: Sender<RunnerEvent>,
    current_child_clone: Arc<Mutex<Option<Child>>>,
    inspector_address: IpAddr,
    inspector_port: u16,
) {
    let host_arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x86_64",
        _ => "arm64",
    };
    let rust_target = "aarch64-apple-ios";
    let xcode_arch = host_arch;
    let sdk = "iphoneos";

    let _ = tx.send(RunnerEvent::StatusChange(Status::Compiling(0)));
    let _ = tx.send(RunnerEvent::BuildLog(format!("Compiling static library for {}...", rust_target)));

    let status = match cargo_build::spawn_cargo_build(
        &CargoBuildTarget::Ios { rust_target: rust_target.to_string() },
        &tx,
        &current_child_clone,
        inspector_address,
        inspector_port,
    ) {
        Some(s) => s,
        None => return,
    };

    if !status.success() {
        let _ = tx.send(RunnerEvent::BuildLog("Cargo build failed.".to_string()));
        let _ = tx.send(RunnerEvent::StatusChange(Status::Error));
        return;
    }

    let lib_name = pkg_name.replace("-", "_");
    let src_lib = format!("target/{}/debug/lib{}.a", rust_target, lib_name);
    let dest_dir = "builds/staticlib/ios";
    let dest_lib = format!("{}/lib{}.a", dest_dir, lib_name);

    std::fs::create_dir_all(dest_dir).unwrap();
    if let Err(e) = std::fs::copy(&src_lib, &dest_lib) {
        let _ = tx.send(RunnerEvent::BuildLog(format!("Failed to copy static library: {}", e)));
        let _ = tx.send(RunnerEvent::StatusChange(Status::Error));
        return;
    } else {
        let _ = tx.send(RunnerEvent::BuildLog(format!("Copied static library to {}", dest_lib)));
    }

    let _ = tx.send(RunnerEvent::BuildLog("Building Xcode project for iOS...".to_string()));

    let mut xcode_build = match Command::new("xcodebuild")
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
        .current_dir("builds/ios")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(build) => build,
        Err(e) => {
            let _ = tx.send(RunnerEvent::BuildLog(format!("Failed to build Xcode project: {}", e)));
            let _ = tx.send(RunnerEvent::StatusChange(Status::Error));
            return;
        }
    };

    let stdout = xcode_build.stdout.take().unwrap();
    let stderr = xcode_build.stderr.take().unwrap();

    *current_child_clone.lock().unwrap() = Some(xcode_build);

    stream_stdout_with_xcode_progress(stdout, tx.clone());
    stream_stderr_as_build_log(stderr, tx.clone());

    let status = match wait_for_child(&current_child_clone) {
        Some(s) => s,
        None => return,
    };

    if !status.success() {
        let _ = tx.send(RunnerEvent::BuildLog("Xcodebuild failed.".to_string()));
        let _ = tx.send(RunnerEvent::StatusChange(Status::Error));
        return;
    }

    let _ = tx.send(RunnerEvent::StatusChange(Status::Launching));
    let device_name = &device.name;
    let _ = tx.send(RunnerEvent::BuildLog(format!("Installing app on {} ...", device_name)));
    let app_path = format!("builds/ios/build/Debug-iphoneos/{}.app", pkg_name);

    let install_status = match Command::new("xcrun")
        .args(["devicectl", "device", "install", "app", "--device", &device.id, &app_path])
        .env("TERM", "dumb")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .status()
    {
        Ok(status) => status,
        Err(e) => {
            let _ = tx.send(RunnerEvent::BuildLog(format!("Failed to install on {}: {}", device_name, e)));
            let _ = tx.send(RunnerEvent::StatusChange(Status::Error));
            return;
        }
    };

    if !install_status.success() {
        let _ = tx.send(RunnerEvent::BuildLog(format!("Failed to install on {}", device_name)));
        let _ = tx.send(RunnerEvent::StatusChange(Status::Error));
        return;
    }

    let _ = tx.send(RunnerEvent::BuildLog("Launching app on iOS Device...".to_string()));

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
            let _ = tx.send(RunnerEvent::BuildLog(format!("Failed to get bundle id: {}", e)));
            let _ = tx.send(RunnerEvent::StatusChange(Status::Error));
            return;
        }
    };

    let bundle_id = String::from_utf8_lossy(&bundle_id_output.stdout)
        .trim()
        .to_string();

    let mut app_run = match Command::new("xcrun")
        .args(["devicectl", "device", "process", "launch", "--terminate-existing", "--console", "--device", &device.id, &bundle_id])
        .env("TERM", "dumb")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(run) => run,
        Err(e) => {
            let _ = tx.send(RunnerEvent::BuildLog(format!("Failed to launch app: {}", e)));
            let _ = tx.send(RunnerEvent::StatusChange(Status::Idling));
            return;
        }
    };

    let stdout = app_run.stdout.take().unwrap();
    let stderr = app_run.stderr.take().unwrap();

    *current_child_clone.lock().unwrap() = Some(app_run);

    stream_as_app_log_split_cr(stdout, tx.clone());
    stream_as_app_log_split_cr(stderr, tx.clone());

    let _ = tx.send(RunnerEvent::StatusChange(Status::Running));

    wait_for_child(&current_child_clone);

    let _ = tx.send(RunnerEvent::StatusChange(Status::Idling));
}
