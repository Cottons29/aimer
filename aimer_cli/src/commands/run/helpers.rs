use std::process::{Child, ChildStderr, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};

use anyhow::bail;
use crossbeam::channel::Sender;

use crate::commands::assemble::{Reporter, Step, StepKind};
use crate::commands::run::cargo_build::{
    stream_stderr_as_build_log, stream_stdout_as_build_log, stream_stdout_with_gradle_progress,
    stream_stdout_with_xcode_progress, wait_for_child,
};
use crate::commands::run::console::{RunnerEvent, Status};
use crate::commands::run::pipeline::RunContext;

/// Emit a build-log line. Thin wrapper over the `tx.send(BuildLog(..))` pattern
/// that every runner repeats constantly.
pub fn build_log(tx: &Sender<RunnerEvent>, msg: impl Into<String>) {
    let _ = tx.send(RunnerEvent::BuildLog(msg.into()));
}

/// Update the runner status shown in the console status bar.
pub fn set_status(tx: &Sender<RunnerEvent>, status: Status) {
    let _ = tx.send(RunnerEvent::StatusChange(status));
}

/// Report a failure: log `msg` and switch the status to [`Status::Error`].
/// This is the canonical "something went wrong, bail out" helper.
pub fn fail(tx: &Sender<RunnerEvent>, msg: impl Into<String>) {
    build_log(tx, msg);
    set_status(tx, Status::Error);
}

/// [`Reporter`] that runs the shared assemble steps inside a live `aimer run`.
///
/// Every step is spawned with piped stdio, registered as the current child so it
/// can be killed when the run is cancelled, and streamed into the Build Logs
/// pane with the progress parsing matching its [`StepKind`]. That is the only
/// difference from `aimer assemble`'s
/// [`StdioReporter`](crate::commands::assemble::StdioReporter) — the packaging
/// itself is the same code.
pub struct ConsoleReporter<'a> {
    tx: &'a Sender<RunnerEvent>,
    current_child: &'a Arc<Mutex<Option<Child>>>,
}

impl<'a> ConsoleReporter<'a> {
    /// A reporter streaming into `tx` and registering each step in
    /// `current_child`.
    #[inline]
    pub fn new(tx: &'a Sender<RunnerEvent>, current_child: &'a Arc<Mutex<Option<Child>>>) -> Self {
        Self { tx, current_child }
    }

    /// A reporter for the console a running pipeline streams to.
    #[inline]
    pub fn of(ctx: &'a RunContext) -> Self {
        Self::new(&ctx.tx, &ctx.current_child)
    }
}

impl Reporter for ConsoleReporter<'_> {
    fn note(&self, message: String) {
        build_log(self.tx, message);
    }

    fn run(&self, mut cmd: Command, step: Step) -> anyhow::Result<()> {
        let mut child = match cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
            Ok(child) => child,
            Err(e) => bail!("failed to start {}: {e}", step.action),
        };

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        *self.current_child.lock().unwrap() = Some(child);

        match step.kind {
            StepKind::Xcode => {
                stream_stdout_with_xcode_progress(stdout, self.tx.clone());
            }
            StepKind::Gradle => {
                stream_stdout_with_gradle_progress(stdout, self.tx.clone());
            }
            StepKind::Cargo | StepKind::Other => {
                stream_stdout_as_build_log(stdout, self.tx.clone());
            }
        }
        stream_stderr_as_build_log(stderr, self.tx.clone());

        match wait_for_child(self.current_child) {
            Some(status) if status.success() => Ok(()),
            Some(_) => bail!("{} failed", step.action),
            None => bail!("{} was cancelled", step.action),
        }
    }
}

/// Host CPU mapped to the Apple/Xcode architecture name (`arm64` / `x86_64`).
pub fn host_arch() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x86_64",
        _ => "arm64",
    }
}

/// Spawn `cmd` with piped stdout/stderr, register it as the current child so it
/// can be killed on cancel, and start streaming both pipes with the supplied
/// streamers.
///
/// Returns `false` (after reporting `spawn_fail_msg` with `fail_status`) when
/// the process could not be spawned, so callers can simply
/// `if !spawn_streamed(..) { return; }`.
pub fn spawn_streamed(
    mut cmd: Command,
    tx: &Sender<RunnerEvent>,
    current_child: &Arc<Mutex<Option<Child>>>,
    spawn_fail_msg: &str,
    fail_status: Status,
    stream_out: impl FnOnce(ChildStdout, Sender<RunnerEvent>),
    stream_err: impl FnOnce(ChildStderr, Sender<RunnerEvent>),
) -> bool {
    let mut child = match cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
        Ok(child) => child,
        Err(e) => {
            build_log(tx, format!("{spawn_fail_msg}: {e}"));
            set_status(tx, fail_status);
            return false;
        }
    };

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    *current_child.lock().unwrap() = Some(child);

    stream_out(stdout, tx.clone());
    stream_err(stderr, tx.clone());
    true
}

/// Run `cmd` to completion (inheriting whatever stdio the caller configured),
/// reporting `spawn_fail_msg` if it cannot be launched and `fail_msg` if it
/// exits with a non-zero status. Returns `true` only on success.
pub fn run_to_completion(
    mut cmd: Command,
    tx: &Sender<RunnerEvent>,
    spawn_fail_msg: &str,
    fail_msg: &str,
) -> bool {
    match cmd.status() {
        Ok(status) if status.success() => true,
        Ok(_) => {
            fail(tx, fail_msg);
            false
        }
        Err(e) => {
            fail(tx, format!("{spawn_fail_msg}: {e}"));
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use crossbeam::channel::{Receiver, unbounded};

    use super::*;

    /// A fresh console channel and child slot for a [`ConsoleReporter`].
    fn console() -> (
        Sender<RunnerEvent>,
        Receiver<RunnerEvent>,
        Arc<Mutex<Option<Child>>>,
    ) {
        let (tx, rx) = unbounded();
        (tx, rx, Arc::new(Mutex::new(None)))
    }

    /// Every build log line the reporter emitted.
    fn build_logs(rx: &Receiver<RunnerEvent>) -> Vec<String> {
        rx.try_iter()
            .filter_map(|event| match event {
                RunnerEvent::BuildLog(line) => Some(line),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_note_becomes_a_build_log_line() {
        let (tx, rx, child) = console();

        ConsoleReporter::new(&tx, &child).note("staging".to_string());

        assert_eq!(build_logs(&rx), vec!["staging".to_string()]);
    }

    #[test]
    fn a_successful_step_streams_its_output_into_the_build_pane() {
        let (tx, rx, child) = console();

        let mut cmd = Command::new("echo");
        cmd.arg("building");

        ConsoleReporter::new(&tx, &child)
            .run(cmd, Step::new(StepKind::Other, "echo"))
            .unwrap();

        assert!(build_logs(&rx).contains(&"building".to_string()));
    }

    #[test]
    fn a_failing_step_names_the_action() {
        let (tx, _rx, child) = console();

        let err = ConsoleReporter::new(&tx, &child)
            .run(
                Command::new("false"),
                Step::new(StepKind::Cargo, "the step"),
            )
            .unwrap_err();

        assert_eq!(err.to_string(), "the step failed");
    }

    #[test]
    fn a_step_that_cannot_start_is_reported_as_such() {
        let (tx, _rx, child) = console();

        let err = ConsoleReporter::new(&tx, &child)
            .run(
                Command::new("aimer-no-such-binary"),
                Step::new(StepKind::Gradle, "the step"),
            )
            .unwrap_err();

        assert!(err.to_string().starts_with("failed to start the step"));
    }
}
