use crate::commands::run::console::{RunnerEvent, Status};
use crossbeam::channel::Sender;
use std::io::{BufRead, BufReader, Read};
use std::net::IpAddr;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub enum CargoBuildTarget {
    Darwin,
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
        CargoBuildTarget::Darwin => {
            let mut c = Command::new("cargo");
            c.arg("build").args(["--target", "aarch64-apple-darwin", "--lib"]);
            c
        }
        CargoBuildTarget::Ios { rust_target } | CargoBuildTarget::IosSim { rust_target } => {
            let mut c = Command::new("cargo");
            c.arg("build").arg("--lib").arg("--target").arg(rust_target).env("RUSTFLAGS","-C link-arg=-Wl,-U,_aimer_ios_request_frame -C link-arg=-Wl,-U,_aimer_ios_pause_frames");
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
        for l in reader.lines().map_while(Result::ok) {
            let _ = tx.send(RunnerEvent::BuildLog(l));
        }
    });
}

pub fn stream_stderr_with_cargo_progress(stderr: impl Read + Send + 'static, tx: Sender<RunnerEvent>) {
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        // Resolved package count gives the compile progress a real denominator,
        // so the percentage tracks the build instead of sticking at 99%.
        let total_units = cargo_lock_package_count();
        let mut fetch_count = 0;
        let mut compile_count: usize = 0;
        for l in reader.lines().map_while(Result::ok) {
            if l.contains("Locking") || l.contains("Updating") {
                let _ = tx.send(RunnerEvent::StatusChange(Status::Locking));
            } else if l.contains("Fetching") || l.contains("Downloading") || l.contains("Downloaded") {
                fetch_count = (fetch_count + 1).min(99);
                let _ = tx.send(RunnerEvent::StatusChange(Status::Fetching(fetch_count)));
            } else if l.contains("Compiling") {
                compile_count += 1;
                let pct = compile_progress(compile_count, total_units);
                let _ = tx.send(RunnerEvent::StatusChange(Status::Compiling(pct)));
            } else if l.contains("Finished") {
                let _ = tx.send(RunnerEvent::StatusChange(Status::Compiling(100)));
            }
            let _ = tx.send(RunnerEvent::BuildLog(l));
        }
    });
}

/// Count the resolved packages in the workspace `Cargo.lock` (each `[[package]]`
/// entry). This is used as an upper-bound denominator for the compile progress
/// percentage. Walks up from the current directory to find the lock file and
/// returns 0 when it can't be located, in which case the caller falls back to an
/// asymptotic estimate.
fn cargo_lock_package_count() -> usize {
    fn count(path: &std::path::Path) -> Option<usize> {
        let contents = std::fs::read_to_string(path).ok()?;
        let n = contents.lines().filter(|l| l.trim() == "[[package]]").count();
        if n > 0 { Some(n) } else { None }
    }
    let mut dir = std::env::current_dir().ok();
    while let Some(d) = dir {
        if let Some(n) = count(&d.join("Cargo.lock")) {
            return n;
        }
        dir = d.parent().map(|p| p.to_path_buf());
    }
    0
}

/// Map a number of compiled crates to a 0–99 percentage.
///
/// Using `total` (the resolved package count) as the denominator keeps the bar
/// proportional on full builds. When `total` is unknown (0) or underestimates
/// the work, the denominator grows with `compiled` so the value climbs smoothly
/// toward 99 instead of sticking there for the rest of the build.
fn compile_progress(compiled: usize, total: usize) -> u8 {
    let denom = total.max(compiled + 1);
    ((compiled * 100) / denom).min(99) as u8
}

pub fn stream_stderr_as_build_log(stderr: impl Read + Send + 'static, tx: Sender<RunnerEvent>) {
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for l in reader.lines().map_while(Result::ok) {
            let _ = tx.send(RunnerEvent::BuildLog(l));
        }
    });
}

pub fn stream_stdout_as_app_log(stdout: impl Read + Send + 'static, tx: Sender<RunnerEvent>) {
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for l in reader.lines().map_while(Result::ok) {
            let _ = tx.send(RunnerEvent::AppLog(l));
        }
    });
}

pub fn stream_stderr_as_app_log(stderr: impl Read + Send + 'static, tx: Sender<RunnerEvent>) {
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for l in reader.lines().map_while(Result::ok) {
            let _ = tx.send(RunnerEvent::AppLog(l));
        }
    });
}

pub fn stream_as_app_log_split_cr(pipe: impl Read + Send + 'static, tx: Sender<RunnerEvent>) {
    thread::spawn(move || {
        let reader = BufReader::new(pipe);
        for l in reader.lines().map_while(Result::ok) {
            for part in l.split('\r') {
                if !part.is_empty() {
                    let _ = tx.send(RunnerEvent::AppLog(part.to_string()));
                }
            }
        }
    });
}

pub fn stream_stdout_with_xcode_progress(stdout: impl Read + Send + 'static, tx: Sender<RunnerEvent>) {
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        let mut build_count = 0;
        for l in reader.lines().map_while(Result::ok) {
            if l.contains("Compile") || l.contains("Process") || l.contains("Link") {
                build_count = (build_count + 2).min(99);
                let _ = tx.send(RunnerEvent::StatusChange(Status::Building(build_count)));
            } else if l.contains("** BUILD SUCCEEDED **") {
                let _ = tx.send(RunnerEvent::StatusChange(Status::Building(100)));
            }
            let _ = tx.send(RunnerEvent::BuildLog(l));
        }
    });
}

pub fn stream_stdout_with_gradle_progress(stdout: impl Read + Send + 'static, tx: Sender<RunnerEvent>) {
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        let mut build_count = 0;
        for l in reader.lines().map_while(Result::ok) {
            if l.contains("Task :") {
                build_count = (build_count + 2).min(99);
                let _ = tx.send(RunnerEvent::StatusChange(Status::Building(build_count)));
            } else if l.contains("BUILD SUCCESSFUL") {
                let _ = tx.send(RunnerEvent::StatusChange(Status::Building(100)));
            }
            let _ = tx.send(RunnerEvent::BuildLog(l));
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

#[cfg(test)]
mod tests {
    use super::compile_progress;

    #[test]
    fn compile_progress_is_proportional_with_known_total() {
        assert_eq!(compile_progress(0, 100), 0);
        assert_eq!(compile_progress(1, 100), 1);
        assert_eq!(compile_progress(50, 100), 50);
        assert_eq!(compile_progress(99, 100), 99);
    }

    #[test]
    fn compile_progress_never_reaches_100_before_finished() {
        // Even when the compiled count matches or exceeds the estimate, the bar
        // stays below 100 until the explicit `Finished` line sets it to 100.
        assert_eq!(compile_progress(100, 100), 99);
        assert!(compile_progress(200, 100) < 100);
        assert_eq!(compile_progress(1000, 100), 99);
    }

    #[test]
    fn compile_progress_climbs_smoothly_without_a_total() {
        // total == 0 (unknown): denominator grows with `compiled`, so the value
        // climbs toward 99 instead of sticking immediately.
        assert_eq!(compile_progress(0, 0), 0);
        assert_eq!(compile_progress(1, 0), 50);
        assert_eq!(compile_progress(3, 0), 75);
        assert!(compile_progress(50, 0) < compile_progress(200, 0));
        assert!(compile_progress(200, 0) <= 99);
    }

    #[test]
    fn compile_progress_grows_when_total_underestimates() {
        // Underestimated total must not pin the bar; it keeps climbing.
        let early = compile_progress(10, 5);
        let later = compile_progress(40, 5);
        assert!(later > early);
        assert!(later <= 99);
    }
}
