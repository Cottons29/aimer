use std::io::{BufRead, BufReader};
use std::net::IpAddr;
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use crossbeam::channel::Sender;

use crate::commands::run::Device;
use crate::commands::run::android::AndroidRunner;
use crate::commands::run::cargo_build::{CargoBuildTarget, cargo_command, wait_for_child};
use crate::commands::run::console::{RunnerEvent, Status};
use crate::commands::run::desktop::DesktopRunner;
use crate::commands::run::helpers::set_status;
use crate::commands::run::ios::IosRunner;
use crate::commands::run::macos::MacosRunner;
use crate::commands::run::web::WebRunner;
use crate::targets::Targets;

/// Everything a per-target runner needs to build and launch the app.
pub struct RunContext {
    pub device: Device,
    pub pkg_name: String,
    pub tx: Sender<RunnerEvent>,
    pub current_child: Arc<Mutex<Option<Child>>>,
    pub inspector_address: IpAddr,
    pub inspector_port: u16,
    /// Compile and package with the release profile instead of debug, as asked
    /// for by `aimer run --release`. Every stage derives its profile from this
    /// flag: the cargo profile, the Xcode configuration, the Gradle task and
    /// therefore the artifact paths.
    pub release: bool,
}

/// One step of the unified run pipeline, in the order [`drive`] executes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Compile the Rust library or binary for the target.
    Build,
    /// Turn the compiled artifact into something launchable: stage the native
    /// library and the registered assets, then run the platform packager
    /// (`xcodebuild`, Gradle, ...).
    Assemble,
    /// Install the bundle if the platform needs it and start the app, leaving
    /// the process registered as the current child so it can be killed and its
    /// output streamed.
    Launch,
}

impl Stage {
    /// The stages in pipeline order.
    pub const ALL: [Stage; 3] = [Stage::Build, Stage::Assemble, Stage::Launch];
}

/// Whether the pipeline should carry on after a stage.
///
/// A stage that returns [`Flow::Abort`] has already reported the failure to the
/// console — usually through [`fail`](crate::commands::run::helpers::fail) — so
/// [`drive`] just stops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum Flow {
    /// The stage succeeded; run the next one.
    Continue,
    /// The stage failed or was cancelled; stop the pipeline.
    Abort,
}

/// A per-target run pipeline, split into the three stages every platform shares:
/// [`build`](Runner::build) → [`assemble`](Runner::assemble) →
/// [`launch`](Runner::launch).
///
/// Implementors only describe *what* their platform does in each stage;
/// sequencing, the `Running`/`Idling` status transitions and waiting for the app
/// to exit all live in [`drive`], so no runner repeats them. State discovered in
/// one stage — an Android ABI, an iOS bundle id — is kept in the runner itself,
/// which is why the stages take `&mut self`.
///
/// `Send` is required because runners are dispatched onto a background thread.
pub trait Runner: Send {
    /// Compile the Rust code for the target.
    ///
    /// Defaults to doing nothing, for targets whose launch step compiles as a
    /// side effect (`trunk serve` on web).
    fn build(&mut self, _ctx: &RunContext) -> Flow {
        Flow::Continue
    }

    /// Package the compiled artifact into a launchable bundle.
    ///
    /// Defaults to doing nothing, for targets that launch the compiler output
    /// directly.
    fn assemble(&mut self, _ctx: &RunContext) -> Flow {
        Flow::Continue
    }

    /// Start the app, registering the spawned process as the current child.
    ///
    /// The stage must not wait for the app to exit — [`drive`] does that once
    /// the process is registered.
    fn launch(&mut self, ctx: &RunContext) -> Flow;

    /// Dispatch `stage` to the matching method. Provided so [`drive`] can walk
    /// [`Stage::ALL`]; not meant to be overridden.
    fn stage(&mut self, ctx: &RunContext, stage: Stage) -> Flow {
        match stage {
            Stage::Build => self.build(ctx),
            Stage::Assemble => self.assemble(ctx),
            Stage::Launch => self.launch(ctx),
        }
    }
}

/// Drive `runner` through the unified pipeline: build → assemble → launch.
///
/// Stops at the first stage that returns [`Flow::Abort`], leaving the status the
/// failing stage reported. Otherwise the app is running, so the status becomes
/// [`Status::Running`], and once the launched process exits the pipeline settles
/// back to [`Status::Idling`].
pub fn drive(mut runner: Box<dyn Runner>, ctx: RunContext) {
    for stage in Stage::ALL {
        if runner.stage(&ctx, stage) == Flow::Abort {
            return;
        }
    }

    set_status(&ctx.tx, Status::Running);
    wait_for_child(&ctx.current_child);
    set_status(&ctx.tx, Status::Idling);
}

/// Resolve the [`Runner`] for a target, or `None` if the target is not
/// runnable on the fly.
pub fn runner_for(target: Targets) -> Option<Box<dyn Runner>> {
    match target {
        Targets::Macos => Some(Box::new(MacosRunner::new())),
        Targets::Web => Some(Box::new(WebRunner::new())),
        Targets::Ios => Some(Box::new(IosRunner::device())),
        Targets::IosSimulator => Some(Box::new(IosRunner::simulator())),
        Targets::Android | Targets::AndroidSimulator => Some(Box::new(AndroidRunner::new())),
        Targets::Windows => Some(Box::new(DesktopRunner::new(Targets::Windows))),
        Targets::Linux => Some(Box::new(DesktopRunner::new(Targets::Linux))),
        _ => None,
    }
}

/// Shared wasm-pack web build used by both the initial run and hot reloads.
///
/// Spawns the build on a background thread and streams its stdout/stderr back
/// as [`RunnerEvent`]s, de-duplicating what used to be two copies of this
/// logic inside `console.rs`. `release` selects the cargo profile wasm-pack
/// compiles with, mirroring `aimer run --release`.
pub fn spawn_wasm_pack(tx: Sender<RunnerEvent>, release: bool) {
    thread::spawn(move || {
        let _ = tx.send(RunnerEvent::StatusChange(Status::Compiling(0)));
        let _ = tx.send(RunnerEvent::BuildLog(
            "Running wasm-pack build...".to_string(),
        ));

        let mut wasm_build = match cargo_command(&CargoBuildTarget::Web, release)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                let _ = tx.send(RunnerEvent::BuildLog(format!(
                    "Failed to start wasm-pack: {e}"
                )));
                let _ = tx.send(RunnerEvent::StatusChange(Status::Error));
                return;
            }
        };

        if let Some(stdout) = wasm_build.stdout.take() {
            let tx_out = tx.clone();
            thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines().map_while(Result::ok) {
                    let _ = tx_out.send(RunnerEvent::BuildLog(line));
                }
            });
        }

        if let Some(stderr) = wasm_build.stderr.take() {
            let tx_err = tx.clone();
            thread::spawn(move || {
                let reader = BufReader::new(stderr);
                let mut compile_count = 0;
                for line in reader.lines().map_while(Result::ok) {
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

#[cfg(test)]
mod tests {
    use crossbeam::channel::{Receiver, unbounded};

    use super::*;

    /// A runner that only records which stages ran, optionally aborting at one
    /// of them.
    struct SpyRunner {
        seen: Arc<Mutex<Vec<Stage>>>,
        abort_at: Option<Stage>,
    }

    impl SpyRunner {
        fn new(abort_at: Option<Stage>) -> (Self, Arc<Mutex<Vec<Stage>>>) {
            let seen = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    seen: Arc::clone(&seen),
                    abort_at,
                },
                seen,
            )
        }

        fn record(&self, stage: Stage) -> Flow {
            self.seen.lock().unwrap().push(stage);
            if self.abort_at == Some(stage) {
                Flow::Abort
            } else {
                Flow::Continue
            }
        }
    }

    impl Runner for SpyRunner {
        fn build(&mut self, _ctx: &RunContext) -> Flow {
            self.record(Stage::Build)
        }

        fn assemble(&mut self, _ctx: &RunContext) -> Flow {
            self.record(Stage::Assemble)
        }

        fn launch(&mut self, _ctx: &RunContext) -> Flow {
            self.record(Stage::Launch)
        }
    }

    /// A runner that implements nothing but the mandatory launch stage, to pin
    /// the defaults down.
    struct LaunchOnlyRunner;

    impl Runner for LaunchOnlyRunner {
        fn launch(&mut self, _ctx: &RunContext) -> Flow {
            Flow::Continue
        }
    }

    /// A context with no child process, so [`drive`] returns as soon as the
    /// stages are done.
    fn context() -> (RunContext, Receiver<RunnerEvent>) {
        let (tx, rx) = unbounded();
        let ctx = RunContext {
            device: Device {
                name: "test".to_string(),
                target: Targets::Macos,
                id: "test".to_string(),
            },
            pkg_name: "my_app".to_string(),
            tx,
            current_child: Arc::new(Mutex::new(None)),
            inspector_address: "127.0.0.1".parse().unwrap(),
            inspector_port: 0,
            release: false,
        };
        (ctx, rx)
    }

    /// Every status the pipeline reported, in order.
    fn statuses(rx: &Receiver<RunnerEvent>) -> Vec<Status> {
        rx.try_iter()
            .filter_map(|event| match event {
                RunnerEvent::StatusChange(status) => Some(status),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn drive_runs_the_stages_in_pipeline_order() {
        let (runner, seen) = SpyRunner::new(None);
        let (ctx, _rx) = context();

        drive(Box::new(runner), ctx);

        assert_eq!(*seen.lock().unwrap(), Stage::ALL.to_vec());
    }

    #[test]
    fn drive_reports_running_then_idling_after_a_successful_launch() {
        let (runner, _seen) = SpyRunner::new(None);
        let (ctx, rx) = context();

        drive(Box::new(runner), ctx);

        assert_eq!(statuses(&rx), vec![Status::Running, Status::Idling]);
    }

    #[test]
    fn drive_stops_at_a_failing_build() {
        let (runner, seen) = SpyRunner::new(Some(Stage::Build));
        let (ctx, rx) = context();

        drive(Box::new(runner), ctx);

        assert_eq!(*seen.lock().unwrap(), vec![Stage::Build]);
        assert!(statuses(&rx).is_empty());
    }

    #[test]
    fn drive_stops_at_a_failing_assemble() {
        let (runner, seen) = SpyRunner::new(Some(Stage::Assemble));
        let (ctx, rx) = context();

        drive(Box::new(runner), ctx);

        assert_eq!(*seen.lock().unwrap(), vec![Stage::Build, Stage::Assemble]);
        assert!(statuses(&rx).is_empty());
    }

    #[test]
    fn drive_does_not_report_running_when_the_launch_fails() {
        let (runner, seen) = SpyRunner::new(Some(Stage::Launch));
        let (ctx, rx) = context();

        drive(Box::new(runner), ctx);

        assert_eq!(*seen.lock().unwrap(), Stage::ALL.to_vec());
        assert!(statuses(&rx).is_empty());
    }

    #[test]
    fn build_and_assemble_default_to_doing_nothing() {
        let (ctx, rx) = context();

        drive(Box::new(LaunchOnlyRunner), ctx);

        assert_eq!(statuses(&rx), vec![Status::Running, Status::Idling]);
    }

    #[test]
    fn runner_for_covers_every_runnable_target() {
        for target in [
            Targets::Macos,
            Targets::Web,
            Targets::Ios,
            Targets::IosSimulator,
            Targets::Android,
            Targets::AndroidSimulator,
            Targets::Windows,
            Targets::Linux,
        ] {
            assert!(runner_for(target).is_some(), "{target}");
        }
    }

    #[test]
    fn runner_for_rejects_targets_that_cannot_be_run() {
        assert!(runner_for(Targets::Terminated).is_none());
    }
}
