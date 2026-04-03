use crate::commands::run::Device;
use crate::commands::run::cargo_build::{
    self, CargoBuildTarget, stream_stderr_as_app_log, stream_stderr_as_build_log, stream_stdout_as_app_log,
    stream_stdout_with_xcode_progress, wait_for_child,
};
use crate::commands::run::console::{RunnerEvent, Status};
use crossbeam::channel::Sender;
use std::net::IpAddr;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

pub fn spawn_macos_runner(
    device: Device,
    pkg_name: String,
    tx: Sender<RunnerEvent>,
    current_child_clone: Arc<Mutex<Option<std::process::Child>>>,
    inspector_address: IpAddr,
    inspector_port: u16,
) {
    let _ = tx.send(RunnerEvent::StatusChange(Status::Compiling(0)));
    let _ = tx.send(RunnerEvent::BuildLog("Compiling static library...".to_string()));

    let status =
        match cargo_build::spawn_cargo_build(&CargoBuildTarget::Macos, &tx, &current_child_clone, inspector_address, inspector_port) {
            Some(s) => s,
            None => return,
        };

    if !status.success() {
        let _ = tx.send(RunnerEvent::BuildLog("Cargo build failed.".to_string()));
        let _ = tx.send(RunnerEvent::StatusChange(Status::Error));
        return;
    }

    let lib_name = pkg_name.replace("-", "_");
    let src_lib = format!("target/debug/lib{}.a", lib_name);
    let dest_dir = "builds/staticlib/macos";
    let dest_lib = format!("{}/lib{}.a", dest_dir, lib_name);

    std::fs::create_dir_all(dest_dir).unwrap();
    if let Err(e) = std::fs::copy(&src_lib, &dest_lib) {
        let _ = tx.send(RunnerEvent::BuildLog(format!("Failed to copy static library: {}", e)));
        let _ = tx.send(RunnerEvent::StatusChange(Status::Error));
        return;
    } else {
        let _ = tx.send(RunnerEvent::BuildLog(format!("Copied static library to {}", dest_lib)));
    }

    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x86_64",
        _ => "arm64",
    };

    let _ = tx.send(RunnerEvent::BuildLog(format!("Building Xcode project for {}...", arch)));

    let mut xcode_build = match Command::new("xcodebuild")
        .arg("-project")
        .arg(format!("{}.xcodeproj", pkg_name))
        .arg("-target")
        .arg(&pkg_name)
        .arg("-configuration")
        .arg("Debug")
        .arg("SYMROOT=build")
        .arg("-arch")
        .arg(arch)
        .current_dir("builds/macos")
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
    let _ = tx.send(RunnerEvent::BuildLog("Launching macOS app...".to_string()));

    let app_exec_path = format!("builds/macos/build/Debug/{}.app/Contents/MacOS/{}", pkg_name, pkg_name);

    let mut app_run = match Command::new(&app_exec_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(run) => run,
        Err(e) => {
            let _ = tx.send(RunnerEvent::BuildLog(format!("Failed to launch macOS app: {}", e)));
            let _ = tx.send(RunnerEvent::StatusChange(Status::Error));
            return;
        }
    };

    let stdout = app_run.stdout.take().unwrap();
    let stderr = app_run.stderr.take().unwrap();

    *current_child_clone.lock().unwrap() = Some(app_run);

    stream_stdout_as_app_log(stdout, tx.clone());
    stream_stderr_as_app_log(stderr, tx.clone());

    let _ = tx.send(RunnerEvent::StatusChange(Status::Running));

    wait_for_child(&current_child_clone);

    let _ = tx.send(RunnerEvent::StatusChange(Status::Idling));
}
