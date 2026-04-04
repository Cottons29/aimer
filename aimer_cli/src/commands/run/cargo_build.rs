use crate::commands::run::console::{RunnerEvent, Status};
use crossbeam::channel::Sender;
use std::io::{BufRead, BufReader, Read};
use std::net::IpAddr;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub enum CargoBuildTarget {
    Macos,
    Ios { rust_target: String },
    IosSim { rust_target: String },
    Android { rust_target: String },
    Web,
}

pub fn spawn_cargo_build(
    target: &CargoBuildTarget,
    tx: &Sender<RunnerEvent>,
    current_child: &Arc<Mutex<Option<Child>>>,
    inspector_address: IpAddr,
    inspector_port: u16,
) -> Option<ExitStatus> {
    let mut cmd = match target {
        CargoBuildTarget::Web => {
            let mut c = Command::new("wasm-pack");
            c.arg("build")
                .arg("--debug")
                .arg("--target")
                .arg("web")
                .arg("--out-dir")
                .arg("builds/web/pkg");
            c
        }
        CargoBuildTarget::Android { rust_target } => {
            let mut c = Command::new("cargo");
            c.arg("ndk")
                .arg("-t")
                .arg(rust_target)
                .arg("build")
                .arg("--lib");
            c
        }
        CargoBuildTarget::Macos => {
            let mut c = Command::new("cargo");
            c.arg("build").arg("--lib");
            c
        }
        CargoBuildTarget::Ios { rust_target } | CargoBuildTarget::IosSim { rust_target } => {
            let mut c = Command::new("cargo");
            c.arg("build").arg("--lib").arg("--target").arg(rust_target);
            c
        }
    };

    cmd.env("DEFAULT_INSPECTOR_PORT", inspector_port.to_string());
    cmd.env("DEFAULT_INSPECTOR_ADDRESS", inspector_address.to_string());

    let mut child = match cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
        Ok(child) => child,
        Err(e) => {
            let label = match target {
                CargoBuildTarget::Web => "wasm-pack build",
                CargoBuildTarget::Android { .. } => "cargo ndk build",
                _ => "cargo build",
            };
            let _ = tx.send(RunnerEvent::BuildLog(format!("Failed to run {}: {}", label, e)));
            let _ = tx.send(RunnerEvent::StatusChange(Status::Error));
            return None;
        }
    };

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    *current_child.lock().unwrap() = Some(child);

    stream_stdout_as_build_log(stdout, tx.clone());
    stream_stderr_with_cargo_progress(stderr, tx.clone());
    wait_for_child(current_child)
}

pub fn stream_stdout_as_build_log(stdout: impl Read + Send + 'static, tx: Sender<RunnerEvent>) {
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if let Ok(l) = line {
                let _ = tx.send(RunnerEvent::BuildLog(l));
            }
        }
    });
}

pub fn stream_stderr_with_cargo_progress(stderr: impl Read + Send + 'static, tx: Sender<RunnerEvent>) {
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        let mut fetch_count = 0;
        let mut compile_count = 0;
        for line in reader.lines() {
            if let Ok(l) = line {
                if l.contains("Fetching") || l.contains("Updating") || l.contains("Downloading") {
                    fetch_count = (fetch_count + 1).min(99);
                    let _ = tx.send(RunnerEvent::StatusChange(Status::Fetching(fetch_count)));
                } else if l.contains("Compiling") {
                    compile_count = (compile_count + 1).min(99);
                    let _ = tx.send(RunnerEvent::StatusChange(Status::Compiling(compile_count)));
                } else if l.contains("Finished") {
                    let _ = tx.send(RunnerEvent::StatusChange(Status::Compiling(100)));
                }
                let _ = tx.send(RunnerEvent::BuildLog(l));
            }
        }
    });
}

pub fn stream_stderr_as_build_log(stderr: impl Read + Send + 'static, tx: Sender<RunnerEvent>) {
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(l) = line {
                let _ = tx.send(RunnerEvent::BuildLog(l));
            }
        }
    });
}

pub fn stream_stdout_as_app_log(stdout: impl Read + Send + 'static, tx: Sender<RunnerEvent>) {
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if let Ok(l) = line {
                let _ = tx.send(RunnerEvent::AppLog(l));
            }
        }
    });
}

pub fn stream_stderr_as_app_log(stderr: impl Read + Send + 'static, tx: Sender<RunnerEvent>) {
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(l) = line {
                let _ = tx.send(RunnerEvent::AppLog(l));
            }
        }
    });
}

pub fn stream_as_app_log_split_cr(pipe: impl Read + Send + 'static, tx: Sender<RunnerEvent>) {
    thread::spawn(move || {
        let reader = BufReader::new(pipe);
        for line in reader.lines() {
            if let Ok(l) = line {
                for part in l.split('\r') {
                    if !part.is_empty() {
                        let _ = tx.send(RunnerEvent::AppLog(part.to_string()));
                    }
                }
            }
        }
    });
}

pub fn stream_stdout_with_xcode_progress(stdout: impl Read + Send + 'static, tx: Sender<RunnerEvent>) {
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        let mut build_count = 0;
        for line in reader.lines() {
            if let Ok(l) = line {
                if l.contains("Compile") || l.contains("Process") || l.contains("Link") {
                    build_count = (build_count + 2).min(99);
                    let _ = tx.send(RunnerEvent::StatusChange(Status::Building(build_count)));
                } else if l.contains("** BUILD SUCCEEDED **") {
                    let _ = tx.send(RunnerEvent::StatusChange(Status::Building(100)));
                }
                let _ = tx.send(RunnerEvent::BuildLog(l));
            }
        }
    });
}

pub fn stream_stdout_with_gradle_progress(stdout: impl Read + Send + 'static, tx: Sender<RunnerEvent>) {
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        let mut build_count = 0;
        for line in reader.lines() {
            if let Ok(l) = line {
                if l.contains("Task :") {
                    build_count = (build_count + 2).min(99);
                    let _ = tx.send(RunnerEvent::StatusChange(Status::Building(build_count)));
                } else if l.contains("BUILD SUCCESSFUL") {
                    let _ = tx.send(RunnerEvent::StatusChange(Status::Building(100)));
                }
                let _ = tx.send(RunnerEvent::BuildLog(l));
            }
        }
    });
}

pub fn wait_for_child(current_child: &Arc<Mutex<Option<Child>>>) -> Option<ExitStatus> {
    loop {
        let mut guard = current_child.lock().unwrap();
        if let Some(child) = guard.as_mut() {
            if let Ok(Some(status)) = child.try_wait() {
                return Some(status);
            }
        } else {
            return None;
        }
        drop(guard);
        thread::sleep(Duration::from_millis(100));
    }
}
