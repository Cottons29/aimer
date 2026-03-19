use crate::commands::run::Device;
use crate::commands::run::console::{RunnerEvent, Status};
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub fn spawn_ios_runner(
    device: Device,
    pkg_name: String,
    tx: std::sync::mpsc::Sender<RunnerEvent>,
    current_child_clone: Arc<Mutex<Option<Child>>>,
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

    let mut cargo_build = match Command::new("cargo")
        .arg("build")
        .arg("--lib")
        .arg("--target")
        .arg(rust_target)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn(){
        Ok(build) => build,
        Err(e) => {
            let _ = tx.send(RunnerEvent::BuildLog(format!("Failed to build static library: {}", e)));
            let _ = tx.send(RunnerEvent::StatusChange(Status::Idling));
            return;
        }
    };

    let stdout = cargo_build.stdout.take().unwrap();
    let stderr = cargo_build.stderr.take().unwrap();

    *current_child_clone.lock().unwrap() = Some(cargo_build);

    let tx_clone1 = tx.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if let Ok(l) = line {
                let _ = tx_clone1.send(RunnerEvent::BuildLog(l));
            }
        }
    });

    let tx_clone2 = tx.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        let mut fetch_count = 0;
        let mut compile_count = 0;
        for line in reader.lines() {
            if let Ok(l) = line {
                if l.contains("Fetching") || l.contains("Updating") || l.contains("Downloading") {
                    fetch_count = (fetch_count + 1).min(99);
                    let _ = tx_clone2.send(RunnerEvent::StatusChange(Status::Fetching(fetch_count)));
                } else if l.contains("Compiling") {
                    compile_count = (compile_count + 1).min(99);
                    let _ = tx_clone2.send(RunnerEvent::StatusChange(Status::Compiling(compile_count)));
                } else if l.contains("Finished") {
                    let _ = tx_clone2.send(RunnerEvent::StatusChange(Status::Compiling(100)));
                }
                let _ = tx_clone2.send(RunnerEvent::BuildLog(l));
            }
        }
    });

    let status = loop {
        let mut guard = current_child_clone.lock().unwrap();
        if let Some(child) = guard.as_mut() {
            if let Ok(Some(status)) = child.try_wait() {
                break status;
            }
        } else {
            return;
        }
        drop(guard);
        thread::sleep(Duration::from_millis(100));
    };

    if !status.success() {
        let _ = tx.send(RunnerEvent::BuildLog("Cargo build failed.".to_string()));
        let _ = tx.send(RunnerEvent::StatusChange(Status::Idling));
        return;
    }

    let lib_name = pkg_name.replace("-", "_");
    let src_lib = format!("target/{}/debug/lib{}.a", rust_target, lib_name);
    let dest_dir = "builds/staticlib/ios";
    let dest_lib = format!("{}/lib{}.a", dest_dir, lib_name);

    std::fs::create_dir_all(dest_dir).unwrap();
    if let Err(e) = std::fs::copy(&src_lib, &dest_lib) {
        let _ = tx.send(RunnerEvent::BuildLog(format!("Failed to copy static library: {}", e)));
        let _ = tx.send(RunnerEvent::StatusChange(Status::Idling));
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
        .spawn(){
        Ok(build) => build,
        Err(e) => {
            let _ = tx.send(RunnerEvent::BuildLog(format!("Failed to build Xcode project: {}", e)));
            let _ = tx.send(RunnerEvent::StatusChange(Status::Idling));
            return;
        }
    };

    let stdout = xcode_build.stdout.take().unwrap();
    let stderr = xcode_build.stderr.take().unwrap();

    *current_child_clone.lock().unwrap() = Some(xcode_build);

    let tx_clone3 = tx.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        let mut build_count = 0;
        for line in reader.lines() {
            if let Ok(l) = line {
                if l.contains("Compile") || l.contains("Process") || l.contains("Link") {
                    build_count = (build_count + 2).min(99);
                    let _ = tx_clone3.send(RunnerEvent::StatusChange(Status::Building(build_count)));
                } else if l.contains("** BUILD SUCCEEDED **") {
                    let _ = tx_clone3.send(RunnerEvent::StatusChange(Status::Building(100)));
                }
                let _ = tx_clone3.send(RunnerEvent::BuildLog(l));
            }
        }
    });

    let tx_clone4 = tx.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(l) = line {
                let _ = tx_clone4.send(RunnerEvent::BuildLog(l));
            }
        }
    });

    let status = loop {
        let mut guard = current_child_clone.lock().unwrap();
        if let Some(child) = guard.as_mut() {
            if let Ok(Some(status)) = child.try_wait() {
                break status;
            }
        } else {
            return;
        }
        drop(guard);
        thread::sleep(Duration::from_millis(100));
    };

    if !status.success() {
        let _ = tx.send(RunnerEvent::BuildLog("Xcodebuild failed.".to_string()));
        let _ = tx.send(RunnerEvent::StatusChange(Status::Idling));
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
            let _ = tx.send(RunnerEvent::StatusChange(Status::Idling));
            return;
        }
    };

    if !install_status.success() {
        let _ = tx.send(RunnerEvent::BuildLog(format!("Failed to install on {}", device_name)));
        let _ = tx.send(RunnerEvent::StatusChange(Status::Idling));
        return;
    }

    let _ = tx.send(RunnerEvent::BuildLog("Launching app on iOS Device...".to_string()));

    let plist_path = format!("{}/Info.plist", app_path);
    let bundle_id_output = match Command::new("plutil")
        .arg("-extract")
        .arg("CFBundleIdentifier")
        .arg("raw")
        .arg(&plist_path)
        .output(){
        Ok(output) => output,
        Err(e) => {
            let _ = tx.send(RunnerEvent::BuildLog(format!("Failed to get bundle id: {}", e)));
            let _ = tx.send(RunnerEvent::StatusChange(Status::Idling));
            return;
        }
    };

    let bundle_id = String::from_utf8_lossy(&bundle_id_output.stdout)
        .trim()
        .to_string();

    let mut app_run = match Command::new("xcrun")
        .args([
            "devicectl",
            "device",
            "process",
            "launch",
            "--terminate-existing",
            "--console",
            "--device",
            &device.id,
            &bundle_id,
        ])
        .env("TERM", "dumb")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn(){
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

    let tx_clone5 = tx.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if let Ok(l) = line {
                for part in l.split('\r') {
                    if !part.is_empty() {
                        let _ = tx_clone5.send(RunnerEvent::AppLog(part.to_string()));
                    }
                }
            }
        }
    });

    let tx_clone6 = tx.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(l) = line {
                for part in l.split('\r') {
                    if !part.is_empty() {
                        let _ = tx_clone6.send(RunnerEvent::AppLog(part.to_string()));
                    }
                }
            }
        }
    });

    let _ = tx.send(RunnerEvent::StatusChange(Status::Running));

    loop {
        let mut guard = current_child_clone.lock().unwrap();
        if let Some(child) = guard.as_mut() {
            if let Ok(Some(_)) = child.try_wait() {
                break;
            }
        } else {
            return;
        }
        drop(guard);
        thread::sleep(Duration::from_millis(100));
    }

    let _ = tx.send(RunnerEvent::StatusChange(Status::Idling));
}
