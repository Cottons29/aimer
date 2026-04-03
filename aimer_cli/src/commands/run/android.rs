use crate::commands::run::Device;
use crate::commands::run::cargo_build::{
    self, CargoBuildTarget, stream_as_app_log_split_cr, stream_stderr_as_build_log, stream_stdout_with_gradle_progress, wait_for_child,
};
use crate::commands::run::console::{RunnerEvent, Status};
use crossbeam::channel::Sender;
use std::env::current_dir;
use std::io::{BufRead, BufReader};
use std::net::IpAddr;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

fn resolve_compatible_java_home() -> Option<String> {
    if cfg!(target_os = "macos") {
        for version in ["17", "21", "23", "11"] {
            if let Ok(output) = std::process::Command::new("/usr/libexec/java_home")
                .arg("-v")
                .arg(version)
                .output()
            {
                if output.status.success() {
                    if let Ok(path) = String::from_utf8(output.stdout) {
                        return Some(path.trim().to_string());
                    }
                }
            }
        }
    }
    None
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
            let _ = tx.send(RunnerEvent::BuildLog(format!("Failed to get ABI: {}", e)));
            let _ = tx.send(RunnerEvent::StatusChange(Status::Idling));
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

    let _ = tx.send(RunnerEvent::StatusChange(Status::Compiling(0)));
    let _ = tx.send(RunnerEvent::BuildLog(format!("Compiling shared library for {}...", rust_target)));

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
        let _ = tx.send(RunnerEvent::BuildLog("Cargo build failed.".to_string()));
        let _ = tx.send(RunnerEvent::StatusChange(Status::Error));
        return;
    }

    let current_dir = current_dir().unwrap().join("builds/android");
    tx.send(RunnerEvent::BuildLog(format!("[Aimer] current_dir: {}", current_dir.display())))
        .unwrap();

    let lib_name = pkg_name.replace("-", "_");
    let src_lib = format!("target/{}/debug/lib{}.so", rust_target, lib_name);
    let dest_dir = format!("builds/android/app/src/main/jniLibs/{}", jni_dir_name);
    let dest_lib = format!("{}/lib{}.so", dest_dir, lib_name);

    std::fs::create_dir_all(dest_dir).unwrap_or_default();
    if let Ok(_) = std::fs::copy(&src_lib, &dest_lib) {
        let _ = tx.send(RunnerEvent::BuildLog(format!("Copied library to {}", dest_lib)));
    }

    let _ = tx.send(RunnerEvent::BuildLog("Building Android project via Gradle...".to_string()));

    let gradlew = if cfg!(windows) { "gradlew.bat" } else { "gradlew" };
    let gradlew_path = current_dir.join(gradlew);

    let mut cmd = Command::new(&gradlew_path);
    cmd.arg("assembleDebug")
        .current_dir(&current_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(java_home) = resolve_compatible_java_home() {
        let _ = tx.send(RunnerEvent::BuildLog(format!("Using JAVA_HOME: {}", java_home)));
        cmd.env("JAVA_HOME", java_home);
    }

    let mut gradle_build = match cmd.spawn() {
        Ok(status) => status,
        Err(e) => {
            let _ = tx.send(RunnerEvent::BuildLog(format!("Failed to run gradle: {}", e)));
            let _ = tx.send(RunnerEvent::StatusChange(Status::Error));
            return;
        }
    };

    let stdout = gradle_build.stdout.take().unwrap();
    let stderr = gradle_build.stderr.take().unwrap();

    *current_child_clone.lock().unwrap() = Some(gradle_build);

    stream_stdout_with_gradle_progress(stdout, tx.clone());
    stream_stderr_as_build_log(stderr, tx.clone());

    let status = match wait_for_child(&current_child_clone) {
        Some(s) => s,
        None => return,
    };

    if !status.success() {
        let _ = tx.send(RunnerEvent::BuildLog("Gradle build failed.".to_string()));
        let _ = tx.send(RunnerEvent::StatusChange(Status::Error));
        return;
    }

    let _ = tx.send(RunnerEvent::StatusChange(Status::Launching));
    let device_name = &device.name;
    let _ = tx.send(RunnerEvent::BuildLog(format!("Installing app on {} ...", device_name)));
    let apk_path = "builds/android/app/build/outputs/apk/debug/app-debug.apk";

    let install_status = match Command::new("adb")
        .args(["-s", &device.id, "install", "-r", apk_path])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .status()
    {
        Ok(status) => status,
        Err(e) => {
            let _ = tx.send(RunnerEvent::BuildLog(format!("Failed to install: {}", e)));
            let _ = tx.send(RunnerEvent::StatusChange(Status::Error));
            return;
        }
    };

    if !install_status.success() {
        let _ = tx.send(RunnerEvent::BuildLog(format!("Failed to install on {}", device_name)));
        let _ = tx.send(RunnerEvent::StatusChange(Status::Error));
        return;
    }

    let _ = tx.send(RunnerEvent::BuildLog("Launching app on Android device...".to_string()));

    let build_gradle_path = current_dir.join("app/build.gradle.kts.template");
    let mut app_id = "com.example.app".to_string();
    if let Ok(content) = std::fs::read_to_string(build_gradle_path) {
        for line in content.lines() {
            if line.contains("applicationId") {
                if let Some(id) = line.split('"').nth(1) {
                    app_id = id.to_string();
                    break;
                }
            }
        }
    }

    let mut app_run = match Command::new("adb")
        .args(["-s", &device.id, "shell", "am", "start", "-n", &format!("{}/android.app.NativeActivity", app_id)])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(status) => status,
        Err(e) => {
            let _ = tx.send(RunnerEvent::BuildLog(format!("Failed to run app: {}", e)));
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

    // Wait for the launch command to finish
    wait_for_child(&current_child_clone);

    // Wait for the app to start and get its PID
    let mut pid = String::new();
    for _ in 0..10 {
        if let Ok(output) = Command::new("adb")
            .args(["-s", &device.id, "shell", "pidof", "-s", &app_id])
            .output()
        {
            let out = String::from_utf8_lossy(&output.stdout).trim().to_string();
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

    let mut logcat = match logcat_cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(status) => status,
        Err(e) => {
            let _ = tx.send(RunnerEvent::BuildLog(format!("Failed to run logcat: {}", e)));
            let _ = tx.send(RunnerEvent::StatusChange(Status::Error));
            return;
        }
    };

    let logcat_stdout = logcat.stdout.take().unwrap();
    let logcat_stderr = logcat.stderr.take().unwrap();

    *current_child_clone.lock().unwrap() = Some(logcat);

    let parse_log = move |l: String| -> String {
        if l.contains("I/RustStdoutStderr") {
            if let Some(item) = l.split_once("): ") {
                return format!("{}", item.1.replace("       ", " "));
            }
        }

        match l.split_once("]") {
            Some((_, log)) => log.to_string(),
            None => l,
        }
    };

    let tx_logcat1 = tx.clone();
    thread::spawn(move || {
        let reader = BufReader::new(logcat_stdout);
        for line in reader.lines() {
            if let Ok(l) = line {
                let log = parse_log(l);
                let _ = tx_logcat1.send(RunnerEvent::AppLog(log));
            }
        }
    });

    let tx_logcat2 = tx.clone();
    thread::spawn(move || {
        let reader = BufReader::new(logcat_stderr);
        for line in reader.lines() {
            if let Ok(l) = line {
                let log = parse_log(l);
                let _ = tx_logcat2.send(RunnerEvent::AppLog(log));
            }
        }
    });

    wait_for_child(&current_child_clone);

    let _ = tx.send(RunnerEvent::StatusChange(Status::Idling));
}
