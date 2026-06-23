use crate::commands::run::Device;
use crate::commands::run::cargo_build::{
    self, CargoBuildTarget, stream_stderr_as_app_log, stream_stderr_as_build_log, stream_stdout_as_app_log, stream_stdout_as_build_log,
    wait_for_child,
};
use crate::commands::run::console::{RunnerEvent, Status};
use crate::commands::run::helpers::{build_log, build_streamed, fail, set_status, spawn_streamed, stage_assets};
use crossbeam::channel::Sender;
use std::net::IpAddr;
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};

pub fn spawn_web_runner(
    _: Device,
    _: String,
    tx: Sender<RunnerEvent>,
    current_child_clone: Arc<Mutex<Option<Child>>>,
    inspector_address: IpAddr,
    inspector_port: u16,
) {
    set_status(&tx, Status::Compiling(0));
    build_log(&tx, "Running wasm-pack build...");

    let status = match cargo_build::spawn_cargo_build(&CargoBuildTarget::Web, &tx, &current_child_clone, inspector_address, inspector_port)
    {
        Some(s) => s,
        None => return,
    };

    if !status.success() {
        fail(&tx, "wasm-pack build failed.");
        return;
    }

    set_status(&tx, Status::Building(0));
    build_log(&tx, "Running npm install...");

    let mut npm_install = Command::new("npm");
    npm_install.arg("install").current_dir("builds/web");

    if !build_streamed(
        npm_install,
        &tx,
        &current_child_clone,
        "Failed to run npm install",
        "npm install failed.",
        stream_stdout_as_build_log,
        stream_stderr_as_build_log,
    ) {
        return;
    }

    // Stage registered assets into Vite's `public/` dir (incrementally) so they
    // are served at the site root and fetched at runtime via web-sys. Without
    // this, `aimer run` (web) would serve the SPA fallback for `/assets/...`.
    stage_assets(&tx, "builds/web/public");

    set_status(&tx, Status::Launching);
    build_log(&tx, "Starting vite server...");

    let mut npm_run = Command::new("npm");
    npm_run.arg("run").arg("dev").current_dir("builds/web");

    if !spawn_streamed(
        npm_run,
        &tx,
        &current_child_clone,
        "Failed to run npm run dev",
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
