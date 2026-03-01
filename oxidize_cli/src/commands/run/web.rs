use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use crate::commands::run::console::{RunnerEvent, Status};
use crate::commands::run::Device;

pub fn spawn_web_runner(
    device: Device,
    pkg_name: String,
    tx: std::sync::mpsc::Sender<RunnerEvent>,
    current_child_clone:Arc<Mutex<Option<Child>>>,
)  {
    let _ = tx.send(RunnerEvent::StatusChange(Status::Compiling(0)));
    let _ = tx.send(RunnerEvent::BuildLog("Running wasm-pack build...".to_string()));

    let mut wasm_build = Command::new("wasm-pack")
        .arg("build")
        .arg("--target")
        .arg("web")
        .arg("--out-dir")
        .arg("builds/web/pkg")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start wasm-pack");

    let stdout = wasm_build.stdout.take().unwrap();
    let stderr = wasm_build.stderr.take().unwrap();

    *current_child_clone.lock().unwrap() = Some(wasm_build);

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
        let mut compile_count = 0;
        for line in reader.lines() {
            if let Ok(l) = line {
                if l.contains("Compiling") {
                    compile_count = (compile_count + 5).min(99);
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
            return; // Child was removed (killed)
        }
        drop(guard);
        thread::sleep(Duration::from_millis(100));
    };

    if !status.success() {
        let _ = tx.send(RunnerEvent::BuildLog("wasm-pack build failed.".to_string()));
        let _ = tx.send(RunnerEvent::StatusChange(Status::Idling));
        return;
    }

    let _ = tx.send(RunnerEvent::StatusChange(Status::Building(0)));
    let _ = tx.send(RunnerEvent::BuildLog("Running npm install...".to_string()));

    let mut npm_install = Command::new("npm")
        .arg("install")
        .current_dir("builds/web")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start npm install");

    let stdout = npm_install.stdout.take().unwrap();
    let stderr = npm_install.stderr.take().unwrap();

    *current_child_clone.lock().unwrap() = Some(npm_install);

    let tx_clone3 = tx.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if let Ok(l) = line {
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
            return; // Child was removed (killed)
        }
        drop(guard);
        thread::sleep(Duration::from_millis(100));
    };

    if !status.success() {
        let _ = tx.send(RunnerEvent::BuildLog("npm install failed.".to_string()));
        let _ = tx.send(RunnerEvent::StatusChange(Status::Idling));
        return;
    }

    let _ = tx.send(RunnerEvent::StatusChange(Status::Launching));
    let _ = tx.send(RunnerEvent::BuildLog("Starting vite server...".to_string()));

    let mut npm_run = Command::new("npm")
        .arg("run")
        .arg("dev")
        .current_dir("builds/web")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start vite server");

    let stdout = npm_run.stdout.take().unwrap();
    let stderr = npm_run.stderr.take().unwrap();

    *current_child_clone.lock().unwrap() = Some(npm_run);

    let tx_clone5 = tx.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if let Ok(l) = line {
                let _ = tx_clone5.send(RunnerEvent::AppLog(l));
            }
        }
    });

    let tx_clone6 = tx.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(l) = line {
                let _ = tx_clone6.send(RunnerEvent::AppLog(l));
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