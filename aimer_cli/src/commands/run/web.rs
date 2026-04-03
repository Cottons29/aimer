use crate::commands::run::Device;
use crate::commands::run::cargo_build::{
    self, CargoBuildTarget, stream_stderr_as_app_log, stream_stderr_as_build_log, stream_stdout_as_app_log, stream_stdout_as_build_log,
    wait_for_child,
};
use crate::commands::run::console::{RunnerEvent, Status};
use crossbeam::channel::Sender;
use std::net::IpAddr;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

pub fn spawn_web_runner(
    _: Device,
    _: String,
    tx: Sender<RunnerEvent>,
    current_child_clone: Arc<Mutex<Option<Child>>>,
    inspector_address: IpAddr,
    inspector_port: u16,
) {
    let _ = tx.send(RunnerEvent::StatusChange(Status::Compiling(0)));
    let _ = tx.send(RunnerEvent::BuildLog("Running wasm-pack build...".to_string()));

    let status = match cargo_build::spawn_cargo_build(&CargoBuildTarget::Web, &tx, &current_child_clone, inspector_address, inspector_port)
    {
        Some(s) => s,
        None => return,
    };

    if !status.success() {
        let _ = tx.send(RunnerEvent::BuildLog("wasm-pack build failed.".to_string()));
        let _ = tx.send(RunnerEvent::StatusChange(Status::Error));
        return;
    }

    let _ = tx.send(RunnerEvent::StatusChange(Status::Building(0)));
    let _ = tx.send(RunnerEvent::BuildLog("Running npm install...".to_string()));

    let mut npm_install = match Command::new("npm")
        .arg("install")
        .current_dir("builds/web")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(install) => install,
        Err(e) => {
            let _ = tx.send(RunnerEvent::BuildLog(format!("Failed to run npm install: {}", e)));
            let _ = tx.send(RunnerEvent::StatusChange(Status::Error));
            return;
        }
    };

    let stdout = npm_install.stdout.take().unwrap();
    let stderr = npm_install.stderr.take().unwrap();

    *current_child_clone.lock().unwrap() = Some(npm_install);

    stream_stdout_as_build_log(stdout, tx.clone());
    stream_stderr_as_build_log(stderr, tx.clone());

    let status = match wait_for_child(&current_child_clone) {
        Some(s) => s,
        None => return,
    };

    if !status.success() {
        let _ = tx.send(RunnerEvent::BuildLog("npm install failed.".to_string()));
        let _ = tx.send(RunnerEvent::StatusChange(Status::Error));
        return;
    }

    let _ = tx.send(RunnerEvent::StatusChange(Status::Launching));
    let _ = tx.send(RunnerEvent::BuildLog("Starting vite server...".to_string()));

    let mut npm_run = match Command::new("npm")
        .arg("run")
        .arg("dev")
        .current_dir("builds/web")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(run) => run,
        Err(e) => {
            let _ = tx.send(RunnerEvent::BuildLog(format!("Failed to run npm run dev: {}", e)));
            let _ = tx.send(RunnerEvent::StatusChange(Status::Error));
            return;
        }
    };

    let stdout = npm_run.stdout.take().unwrap();
    let stderr = npm_run.stderr.take().unwrap();

    *current_child_clone.lock().unwrap() = Some(npm_run);

    stream_stdout_as_app_log(stdout, tx.clone());
    stream_stderr_as_app_log(stderr, tx.clone());

    let _ = tx.send(RunnerEvent::StatusChange(Status::Running));

    wait_for_child(&current_child_clone);

    let _ = tx.send(RunnerEvent::StatusChange(Status::Idling));
}
