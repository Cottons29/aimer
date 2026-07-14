use std::net::IpAddr;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use crossbeam::channel::Sender;

use crate::commands::run::Device;
use crate::commands::run::cargo_build::{
    self, CargoBuildTarget, stream_stderr_as_app_log, stream_stderr_as_build_log,
    stream_stdout_as_app_log, stream_stdout_with_xcode_progress, wait_for_child,
};
use crate::commands::run::console::{RunnerEvent, Status};
use crate::commands::run::helpers::{
    build_log, build_streamed, fail, host_arch, set_status, spawn_streamed,
};
use crate::commands::run::utilities::resolve_lib_path;

pub fn spawn_macos_runner(
    _device: Device,
    pkg_name: String,
    tx: Sender<RunnerEvent>,
    current_child_clone: Arc<Mutex<Option<std::process::Child>>>,
    inspector_address: IpAddr,
    inspector_port: u16,
) {
    set_status(&tx, Status::Compiling(0));
    build_log(&tx, "Compiling static library...");

    let status = match cargo_build::spawn_cargo_build(
        &CargoBuildTarget::Darwin,
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
    let src_lib = resolve_lib_path(&lib_name, "aarch64-apple-darwin", CargoBuildTarget::Darwin);
    let dest_dir = "builds/macos/Libraries";
    let dest_lib = format!("{}/lib{}.a", dest_dir, lib_name);

    std::fs::create_dir_all(dest_dir).unwrap();
    if let Err(e) = std::fs::copy(&src_lib, &dest_lib) {
        build_log(&tx, format!("Failed to copy static library: src_lib = {}", src_lib));
        build_log(&tx, format!("Failed to copy static library: dest_lib = {}", dest_lib));
        fail(&tx, format!("Failed to copy static library: {}", e));
        return;
    } else {
        build_log(&tx, format!("Copied static library to {}", dest_lib));
    }

    let arch = host_arch();

    build_log(&tx, format!("Building Xcode project for {}...", arch));

    let mut xcode_build = Command::new("xcodebuild");
    xcode_build
        .arg("-project")
        .arg(format!("{}.xcodeproj", pkg_name))
        .arg("-target")
        .arg(&pkg_name)
        .arg("-configuration")
        .arg("Debug")
        .arg("SYMROOT=build")
        .arg("-arch")
        .arg(arch)
        .current_dir("builds/macos");

    if !build_streamed(
        xcode_build,
        &tx,
        &current_child_clone,
        &format!("Failed to build Xcode project, pkg_name = {}", pkg_name),
        "Xcodebuild failed.",
        stream_stdout_with_xcode_progress,
        stream_stderr_as_build_log,
    ) {
        return;
    }

    set_status(&tx, Status::Launching);
    build_log(&tx, "Launching macOS app...");

    let app_exec_path =
        format!("builds/macos/build/Debug/{}.app/Contents/MacOS/{}", pkg_name, pkg_name);

    let mut app_run = Command::new(&app_exec_path);
    app_run
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if !spawn_streamed(
        app_run,
        &tx,
        &current_child_clone,
        "Failed to launch macOS app",
        Status::Error,
        stream_stdout_as_app_log,
        stream_stderr_as_app_log,
    ) {
        return;
    }

    set_status(&tx, Status::Running);

    wait_for_child(&current_child_clone);

    set_status(&tx, Status::Idling);
}
