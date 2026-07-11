use crate::commands::assemble::copy_assets_into;
use crate::commands::run::cargo_build::{stream_stderr_as_app_log, stream_stdout_as_app_log, wait_for_child};
use crate::commands::run::console::{RunnerEvent, Status};
use crate::commands::run::helpers::{build_log, set_status, spawn_streamed};
use crate::commands::run::Device;
use crossbeam::channel::Sender;
use std::net::IpAddr;
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};

pub fn spawn_web_runner(
    _: Device,
    _: String,
    tx: Sender<RunnerEvent>,
    current_child_clone: Arc<Mutex<Option<Child>>>,
    _: IpAddr,
    _: u16,
) {
    // Write [[copy]] entries into Trunk.toml so that trunk itself copies
    // registered assets into dist/ during its build. Trunk cleans dist/
    // before building, so manual pre-staging would be wiped.
    set_status(&tx, Status::Building(0));
    // if let Err(e) = crate::commands::assets::sync_trunk_copy_entries() {
    //     fail(&tx, format!("Warning: failed to sync trunk asset entries: {e}"));
    //     return;
    // }

    let artifact = "builds/web";
    let Ok(_) = copy_assets_into(artifact) else  {
        println!("Failed to copy assets into {artifact}");
        return;
    };

    set_status(&tx, Status::Launching);
    build_log(&tx, "Starting trunk server...");



    let mut trunk = Command::new("trunk");
    trunk.arg("serve").current_dir("builds/web");

    if !spawn_streamed(
        trunk,
        &tx,
        &current_child_clone,
        "Failed to run trunk serve",
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
