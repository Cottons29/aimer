use std::io::{BufRead, BufReader};
use std::net::IpAddr;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use crossbeam::channel::Sender;

use crate::commands::run::Device;
use crate::commands::run::android::spawn_android_runner;
use crate::commands::run::console::{RunnerEvent, Status};
use crate::commands::run::ios::spawn_ios_runner;
use crate::commands::run::ios_sim::spawn_ios_simulator_runner;
use crate::commands::run::macos::spawn_macos_runner;
use crate::commands::run::web::spawn_web_runner;
use crate::targets::Targets;

/// Everything a per-target runner needs to build and launch the app.
pub struct RunContext {
    pub device: Device,
    pub pkg_name: String,
    pub tx: Sender<RunnerEvent>,
    pub current_child: Arc<Mutex<Option<Child>>>,
    pub inspector_address: IpAddr,
    pub inspector_port: u16,
}

/// A per-target build/launch pipeline. Implementors carry out the full
/// compile → package → launch flow for one platform.
///
/// `Send` is required because runners are dispatched onto a background thread.
pub trait Runner: Send {
    fn run(&self, ctx: RunContext);
}

macro_rules! define_runner {
    ($name:ident, $spawn:path) => {
        pub struct $name;
        impl Runner for $name {
            fn run(&self, ctx: RunContext) {
                $spawn(
                    ctx.device,
                    ctx.pkg_name,
                    ctx.tx,
                    ctx.current_child,
                    ctx.inspector_address,
                    ctx.inspector_port,
                );
            }
        }
    };
}

define_runner!(MacosRunner, spawn_macos_runner);
define_runner!(WebRunner, spawn_web_runner);
define_runner!(IosRunner, spawn_ios_runner);
define_runner!(IosSimulatorRunner, spawn_ios_simulator_runner);
define_runner!(AndroidRunner, spawn_android_runner);

/// Resolve the [`Runner`] for a target, or `None` if the target is not
/// runnable on the fly.
pub fn runner_for(target: Targets) -> Option<Box<dyn Runner>> {
    match target {
        Targets::Macos => Some(Box::new(MacosRunner)),
        Targets::Web => Some(Box::new(WebRunner)),
        Targets::Ios => Some(Box::new(IosRunner)),
        Targets::IosSimulator => Some(Box::new(IosSimulatorRunner)),
        Targets::Android | Targets::AndroidSimulator => Some(Box::new(AndroidRunner)),
        _ => None,
    }
}

/// Shared wasm-pack web build used by both the initial run and hot reloads.
///
/// Spawns the build on a background thread and streams its stdout/stderr back
/// as [`RunnerEvent`]s, de-duplicating what used to be two copies of this
/// logic inside `console.rs`.
pub fn spawn_wasm_pack(tx: Sender<RunnerEvent>) {
    thread::spawn(move || {
        let _ = tx.send(RunnerEvent::StatusChange(Status::Compiling(0)));
        let _ = tx.send(RunnerEvent::BuildLog("Running wasm-pack build...".to_string()));

        let mut wasm_build = match Command::new("wasm-pack")
            .arg("build")
            .arg("--debug")
            .arg("--target")
            .arg("web")
            .arg("--out-dir")
            .arg("builds/web/pkg")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                let _ = tx.send(RunnerEvent::BuildLog(format!("Failed to start wasm-pack: {e}")));
                let _ = tx.send(RunnerEvent::StatusChange(Status::Error));
                return;
            }
        };

        if let Some(stdout) = wasm_build
            .stdout
            .take()
        {
            let tx_out = tx.clone();
            thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader
                    .lines()
                    .map_while(Result::ok)
                {
                    let _ = tx_out.send(RunnerEvent::BuildLog(line));
                }
            });
        }

        if let Some(stderr) = wasm_build
            .stderr
            .take()
        {
            let tx_err = tx.clone();
            thread::spawn(move || {
                let reader = BufReader::new(stderr);
                let mut compile_count = 0;
                for line in reader
                    .lines()
                    .map_while(Result::ok)
                {
                    if line.contains("Compiling") {
                        compile_count = (compile_count + 5).min(99);
                        let _ = tx_err
                            .send(RunnerEvent::StatusChange(Status::Compiling(compile_count)));
                    } else if line.contains("Finished") {
                        let _ = tx_err.send(RunnerEvent::StatusChange(Status::Compiling(100)));
                    }
                    let _ = tx_err.send(RunnerEvent::BuildLog(line));
                }
            });
        }

        match wasm_build.wait() {
            Ok(status) if status.success() => {
                let _ = tx.send(RunnerEvent::BuildLog(
                    "wasm-pack build successful. Vite will auto-reload.".to_string(),
                ));
            }
            _ => {
                let _ = tx.send(RunnerEvent::BuildLog("wasm-pack build failed.".to_string()));
            }
        }
        let _ = tx.send(RunnerEvent::StatusChange(Status::Running));
    });
}
