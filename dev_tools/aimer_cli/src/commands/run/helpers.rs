use std::process::{Child, ChildStderr, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};

use crossbeam::channel::Sender;

use crate::commands::run::cargo_build::wait_for_child;
use crate::commands::run::console::{RunnerEvent, Status};

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

/// Stage the registered `[assets]` into `dest_root` for a live `run`,
/// reporting the outcome through the TUI console.
///
/// Copying is incremental — only new or changed files are written — so hot
/// reloads don't re-copy unchanged assets. Failures and missing files are
/// logged as build-log warnings rather than aborting the run, mirroring how
/// `assemble` treats them.
pub fn stage_assets(tx: &Sender<RunnerEvent>, dest_root: &str) {
    match crate::commands::assets::copy_assets_into(dest_root) {
        Ok(report) => {
            for rel in &report.copied {
                build_log(tx, format!("Staged asset {rel} -> {dest_root}/{rel}"));
            }
            for rel in &report.missing {
                build_log(tx, format!("Warning: registered asset '{rel}' not found; skipping"));
            }
        }
        Err(e) => build_log(tx, format!("Warning: failed to stage assets into {dest_root}: {e}")),
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
    let mut child = match cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
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

/// Run a streamed build step end to end: spawn it (see [`spawn_streamed`]),
/// wait for completion, and verify success. Reports `spawn_fail_msg` if it
/// cannot be launched and `build_fail_msg` if it exits with a non-zero status.
/// Returns `true` only when the step finished successfully.
pub fn build_streamed(
    cmd: Command,
    tx: &Sender<RunnerEvent>,
    current_child: &Arc<Mutex<Option<Child>>>,
    spawn_fail_msg: &str,
    build_fail_msg: &str,
    stream_out: impl FnOnce(ChildStdout, Sender<RunnerEvent>),
    stream_err: impl FnOnce(ChildStderr, Sender<RunnerEvent>),
) -> bool {
    if !spawn_streamed(
        cmd,
        tx,
        current_child,
        spawn_fail_msg,
        Status::Error,
        stream_out,
        stream_err,
    ) {
        return false;
    }

    match wait_for_child(current_child) {
        Some(status) if status.success() => true,
        Some(_) => {
            fail(tx, build_fail_msg);
            false
        }
        None => false,
    }
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
