use crate::commands::run::Device;
use crate::commands::run::console::RunnerEvent;
use crate::commands::run::ios::{IosVariant, run_ios};
use crossbeam::channel::Sender;
use std::net::IpAddr;
use std::process::Child;
use std::sync::{Arc, Mutex};

pub fn spawn_ios_simulator_runner(
    device: Device,
    pkg_name: String,
    tx: Sender<RunnerEvent>,
    current_child_clone: Arc<Mutex<Option<Child>>>,
    inspector_address: IpAddr,
    inspector_port: u16,
) {
    run_ios(
        IosVariant::Simulator,
        device,
        pkg_name,
        tx,
        current_child_clone,
        inspector_address,
        inspector_port,
    );
}
