use std::env::current_dir;
use std::io::{BufRead, BufReader};
use std::net::IpAddr;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crossbeam::channel::Sender;

use crate::commands::run::Device;
use crate::commands::run::cargo_build::{
    self, CargoBuildTarget, stream_as_app_log_split_cr, stream_stderr_as_build_log,
    stream_stdout_with_gradle_progress, wait_for_child,
};
use crate::commands::run::console::{RunnerEvent, Status};
use crate::commands::run::helpers::{
    build_log, build_streamed, fail, run_to_completion, set_status, spawn_streamed, stage_assets,
};
use crate::commands::run::utilities::resolve_lib_path;

fn resolve_compatible_java_home() -> Option<String> {
    if cfg!(target_os = "macos") {
        for version in ["17", "21", "23", "11"] {
            let Ok(output) = std::process::Command::new("/usr/libexec/java_home")
                .arg("-v")
                .arg(version)
                .output()
            else {
                continue;
            };
            if !output.status.success() {
                continue;
            }
            if let Ok(path) = String::from_utf8(output.stdout) {
                return Some(path.trim().to_string());
            }
        }
    }
    None
}

/// Parse a single `adb logcat` line into the text shown in the app log pane.
fn parse_logcat_line(l: String) -> String {
    if l.contains("I/RustStdoutStderr")
        && let Some(item) = l.split_once("): ")
    {
        return item.1.replace("       ", " ");
    }

    match l.split_once("]") {
        Some((_, log)) => log.to_string(),
        None => l,
    }
}

pub fn spawn_android_runner(
    device: Device,
    pkg_name: String,
    tx: Sender<RunnerEvent>,
    current_child_clone: Arc<Mutex<Option<Child>>>,
    inspector_address: IpAddr,
    inspector_port: u16,
) {
    let abi_output = match Command::new("adb")
        .args(["-s", &device.id, "shell", "getprop", "ro.product.cpu.abi"])
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            build_log(&tx, format!("Failed to get ABI: {}", e));
            set_status(&tx, Status::Idling);
            return;
        }
    };

    let abi = String::from_utf8_lossy(&abi_output.stdout)
        .trim()
        .to_string();

    let (rust_target, jni_dir_name) = match abi.as_str() {
        "x86_64" => ("x86_64-linux-android", "x86_64"),
        "armeabi-v7a" => ("armv7-linux-androideabi", "armeabi-v7a"),
        "x86" => ("i686-linux-android", "x86"),
        _ => ("aarch64-linux-android", "arm64-v8a"),
    };

    set_status(&tx, Status::Compiling(0));
    build_log(&tx, format!("Compiling shared library for {}...", rust_target));

    let status = match cargo_build::spawn_cargo_build(
        &CargoBuildTarget::Android { rust_target: rust_target.to_string() },
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

    let current_dir = current_dir()
        .unwrap()
        .join("builds/android");
    build_log(&tx, format!("[Aimer] current_dir: {}", current_dir.display()));

    let lib_name = pkg_name.replace("-", "_");
    let src_lib = resolve_lib_path(
        &lib_name,
        rust_target,
        CargoBuildTarget::Android { rust_target: rust_target.to_string() },
    );
    let dest_dir = format!("builds/android/app/src/main/jniLibs/{}", jni_dir_name);
    let dest_lib = format!("{}/lib{}.so", dest_dir, lib_name);

    std::fs::create_dir_all(dest_dir).unwrap_or_default();
    if std::fs::copy(&src_lib, &dest_lib).is_ok() {
        build_log(&tx, format!("Copied library to {}", dest_lib));
    }

    // Stage registered assets into the APK's `assets/` source set (incrementally)
    // before Gradle packs it, so they are readable at runtime via AssetManager.
    stage_assets(&tx, "builds/android/app/src/main/assets");

    build_log(&tx, "Building Android project via Gradle...");

    let gradlew = if cfg!(windows) { "gradlew.bat" } else { "gradlew" };
    let gradlew_path = current_dir.join(gradlew);

    let mut cmd = Command::new(&gradlew_path);
    cmd.arg("assembleDebug")
        .current_dir(&current_dir);

    if let Some(java_home) = resolve_compatible_java_home() {
        build_log(&tx, format!("Using JAVA_HOME: {}", java_home));
        cmd.env("JAVA_HOME", java_home);
    }

    if !build_streamed(
        cmd,
        &tx,
        &current_child_clone,
        "Failed to run gradle",
        "Gradle build failed.",
        stream_stdout_with_gradle_progress,
        stream_stderr_as_build_log,
    ) {
        return;
    }

    set_status(&tx, Status::Launching);
    let device_name = &device.name;
    build_log(&tx, format!("Installing app on {} ...", device_name));
    let apk_path = "builds/android/app/build/outputs/apk/debug/app-debug.apk";

    let mut install = Command::new("adb");
    install
        .args(["-s", &device.id, "install", "-r", apk_path])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if !run_to_completion(
        install,
        &tx,
        "Failed to install",
        &format!("Failed to install on {}", device_name),
    ) {
        return;
    }

    build_log(&tx, "Launching app on Android device...");

    let build_gradle_path = current_dir.join("app/build.gradle.kts.template");
    let mut app_id = "com.example.app".to_string();
    if let Ok(content) = std::fs::read_to_string(build_gradle_path) {
        for line in content.lines() {
            if !line.contains("applicationId") {
                continue;
            }
            if let Some(id) = line.split('"').nth(1) {
                app_id = id.to_string();
                break;
            }
        }
    }

    let mut app_run = Command::new("adb");
    app_run.args([
        "-s",
        &device.id,
        "shell",
        "am",
        "start",
        "-n",
        &format!("{}/com.aimer.AimerActivity", app_id),
    ]);

    if !spawn_streamed(
        app_run,
        &tx,
        &current_child_clone,
        "Failed to run app",
        Status::Idling,
        stream_as_app_log_split_cr,
        stream_as_app_log_split_cr,
    ) {
        return;
    }

    set_status(&tx, Status::Running);

    // Wait for the launch command to finish
    wait_for_child(&current_child_clone);

    // Wait for the app to start and get its PID
    let mut pid = String::new();
    for _ in 0..10 {
        if let Ok(output) = Command::new("adb")
            .args(["-s", &device.id, "shell", "pidof", "-s", &app_id])
            .output()
        {
            let out = String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_string();
            if !out.is_empty() {
                pid = out;
                break;
            }
        }
        thread::sleep(Duration::from_millis(200));
    }

    // Launch logcat to tail logs
    let mut logcat_cmd = Command::new("adb");
    logcat_cmd.args(["-s", &device.id, "logcat", "-v", "time"]);

    if !pid.is_empty() {
        logcat_cmd.args(["--pid", &pid]);
    }

    if !spawn_streamed(
        logcat_cmd,
        &tx,
        &current_child_clone,
        "Failed to run logcat",
        Status::Error,
        |stdout, tx| {
            thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader
                    .lines()
                    .map_while(Result::ok)
                {
                    let _ = tx.send(RunnerEvent::AppLog(parse_logcat_line(line)));
                }
            });
        },
        |stderr, tx| {
            thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader
                    .lines()
                    .map_while(Result::ok)
                {
                    let _ = tx.send(RunnerEvent::AppLog(parse_logcat_line(line)));
                }
            });
        },
    ) {
        return;
    }

    wait_for_child(&current_child_clone);

    set_status(&tx, Status::Idling);
}
