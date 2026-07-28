use std::io::{BufRead, BufReader, Read};
use std::net::IpAddr;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crossbeam::channel::Sender;

use crate::commands::run::cargo_message::{self, CargoMessage, ErrorReport};
use crate::commands::run::console::{RunnerEvent, Status};
use crate::commands::run::utilities::LogStyling;

pub enum CargoBuildTarget {
    Darwin,
    Ios { rust_target: String },
    IosSim { rust_target: String },
    Android { rust_target: String },
    Web,
}

impl CargoBuildTarget {
    /// Whether the build is driven by cargo itself, and can therefore be asked
    /// for the JSON messages [`stream_cargo_json`] understands.
    ///
    /// `wasm-pack` drives its own cargo invocation and does not forward
    /// `--message-format`, so the web target keeps its plain text output.
    fn speaks_cargo_json(&self) -> bool {
        !matches!(self, CargoBuildTarget::Web)
    }
}

/// The compiler invocation for `target`, in debug or release mode.
///
/// The profile flag is the only difference between an `aimer run` and an
/// `aimer run --release` build; the resulting artifact then lives under the
/// matching `target/<triple>/<profile>` directory the assemble steps read from.
pub fn cargo_command(target: &CargoBuildTarget, release: bool) -> Command {
    let mut cmd = match target {
        CargoBuildTarget::Web => {
            let mut c = Command::new("wasm-pack");
            c.arg("build")
                .arg(if release { "--release" } else { "--debug" })
                .arg("--target")
                .arg("web")
                .arg("--out-dir")
                .arg("builds/web/pkg");
            return c;
        }
        CargoBuildTarget::Android { rust_target } => {
            let mut c = Command::new("cargo");
            c.arg("ndk")
                .arg("-t")
                .arg(rust_target)
                .arg("build")
                .arg("--lib")
                .arg(cargo_message::MESSAGE_FORMAT);
            c
        }
        CargoBuildTarget::Darwin => {
            let mut c = Command::new("cargo");
            c.arg("build")
                .args(["--target", "aarch64-apple-darwin", "--lib"])
                .arg(cargo_message::MESSAGE_FORMAT);
            c
        }
        CargoBuildTarget::Ios { rust_target } | CargoBuildTarget::IosSim { rust_target } => {
            let mut c = Command::new("cargo");
            c.arg("build").arg("--lib").arg("--target").arg(rust_target).arg(cargo_message::MESSAGE_FORMAT).env("RUSTFLAGS","-C link-arg=-Wl,-U,_aimer_ios_request_frame -C link-arg=-Wl,-U,_aimer_ios_pause_frames");
            c
        }
    };

    if release {
        cmd.arg("--release");
    }
    cmd
}

pub fn spawn_cargo_build(
    target: &CargoBuildTarget,
    tx: &Sender<RunnerEvent>,
    current_child: &Arc<Mutex<Option<Child>>>,
    inspector_address: IpAddr,
    inspector_port: u16,
    release: bool,
) -> Option<ExitStatus> {
    let mut cmd = cargo_command(target, release);

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
            let _ = tx.send(RunnerEvent::BuildLog(format!(
                "Failed to run {}: {}",
                label, e
            )));
            let _ = tx.send(RunnerEvent::StatusChange(Status::Error));
            return None;
        }
    };

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    *current_child.lock().unwrap() = Some(child);

    // Cargo writes its diagnostics as JSON on stdout while keeping the
    // `Compiling` / `Finished` status lines on stderr, so the progress reader
    // below is unaffected by the message format.
    let json_reader = if target.speaks_cargo_json() {
        Some(stream_cargo_json(stdout, tx.clone()))
    } else {
        let _ = stream_stdout_as_build_log(stdout, tx.clone());
        None
    };
    let stderr_reader = stream_stderr_with_cargo_progress(stderr, tx.clone());
    let status = wait_for_child(current_child);

    // Both pipes are drained to their end before the report is written, so the
    // errors of a failed build are the very last thing in the pane instead of
    // something to scroll back for.
    let _ = stderr_reader.join();
    if let Some(report) = json_reader.and_then(|reader| reader.join().ok())
        && !report.is_empty()
    {
        for line in report.lines() {
            let _ = tx.send(RunnerEvent::BuildLog(line));
        }
    }
    status
}

/// Stream cargo's JSON messages as build log lines, collecting the errors on the
/// way.
///
/// Every diagnostic is replayed exactly as `cargo build` renders it, so the pane
/// keeps looking like a normal cargo build. Output that is not a cargo message —
/// a build script's own `println!` — is forwarded verbatim, so nothing is
/// swallowed.
///
/// The [`ErrorReport`] the thread returns is what [`spawn_cargo_build`] appends
/// once the build is over.
pub fn stream_cargo_json(
    stdout: impl Read + Send + 'static,
    tx: Sender<RunnerEvent>,
) -> thread::JoinHandle<ErrorReport> {
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        let mut report = ErrorReport::new();
        for l in reader.lines().map_while(Result::ok) {
            match CargoMessage::parse(&l) {
                Some(CargoMessage::Diagnostic(diagnostic)) => {
                    report.record(&diagnostic);
                    for line in diagnostic.render_lines() {
                        let _ = tx.send(RunnerEvent::BuildLog(line));
                    }
                    // Cargo leaves a blank line between diagnostics.
                    let _ = tx.send(RunnerEvent::BuildLog(String::new()));
                }
                // Artifacts and the build outcome are bookkeeping; cargo already
                // announced them on stderr.
                Some(_) => {}
                None => {
                    let _ = tx.send(RunnerEvent::BuildLog(l));
                }
            }
        }
        report
    })
}

pub fn stream_stdout_as_build_log(
    stdout: impl Read + Send + 'static,
    tx: Sender<RunnerEvent>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for l in reader.lines().map_while(Result::ok) {
            let _ = tx.send(RunnerEvent::BuildLog(l));
        }
    })
}

pub fn stream_stderr_with_cargo_progress(
    stderr: impl Read + Send + 'static,
    tx: Sender<RunnerEvent>,
) -> thread::JoinHandle<()> {
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
            } else if l.contains("Fetching")
                || l.contains("Downloading")
                || l.contains("Downloaded")
            {
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
    })
}

/// Count the resolved packages in the workspace `Cargo.lock` (each
/// `[[package]]` entry). This is used as an upper-bound denominator for the
/// compile progress percentage. Walks up from the current directory to find the
/// lock file and returns 0 when it can't be located, in which case the caller
/// falls back to an asymptotic estimate.
fn cargo_lock_package_count() -> usize {
    fn count(path: &std::path::Path) -> Option<usize> {
        let contents = std::fs::read_to_string(path).ok()?;
        let n = contents
            .lines()
            .filter(|l| l.trim() == "[[package]]")
            .count();
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

/// Stream the application's stdout as app logs, turning each line into a styled
/// console line on this reader thread.
///
/// Parsing the JSON records the app emits under
/// [`JSON_OUTPUT_FLAG`](aimer_utils::log::JSON_OUTPUT_FLAG) — and applying the
/// resulting colors — happens here rather than in the console event loop, so the
/// UI thread only stores ready-to-draw lines. Output that is not a log record is
/// forwarded unchanged.
pub fn stream_stdout_as_app_log(stdout: impl Read + Send + 'static, tx: Sender<RunnerEvent>) {
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for l in reader.lines().map_while(Result::ok) {
            let _ = tx.send(RunnerEvent::AppLog(l.process_log()));
        }
    });
}

/// Same as [`stream_stdout_as_app_log`], for the application's stderr.
pub fn stream_stderr_as_app_log(stderr: impl Read + Send + 'static, tx: Sender<RunnerEvent>) {
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for l in reader.lines().map_while(Result::ok) {
            let _ = tx.send(RunnerEvent::AppLog(l.process_log()));
        }
    });
}

/// Stream a pipe that packs several records into one line separated by carriage
/// returns (`devicectl --console`), styling every part individually.
pub fn stream_as_app_log_split_cr(pipe: impl Read + Send + 'static, tx: Sender<RunnerEvent>) {
    thread::spawn(move || {
        let reader = BufReader::new(pipe);
        for l in reader.lines().map_while(Result::ok) {
            for part in l.split('\r') {
                if !part.is_empty() {
                    let _ = tx.send(RunnerEvent::AppLog(part.to_string().process_log()));
                }
            }
        }
    });
}

pub fn stream_stdout_with_xcode_progress(
    stdout: impl Read + Send + 'static,
    tx: Sender<RunnerEvent>,
) {
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

pub fn stream_stdout_with_gradle_progress(
    stdout: impl Read + Send + 'static,
    tx: Sender<RunnerEvent>,
) {
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

        let child = guard.as_mut()?;

        if let Ok(Some(status)) = child.try_wait() {
            return Some(status);
        }

        drop(guard);
        thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::commands::run::utilities::StyledLog;
    use crate::console::state::strip_ansi;

    /// The arguments `cargo_command` passes for `target` in `release` mode.
    fn cargo_args(target: &CargoBuildTarget, release: bool) -> Vec<String> {
        cargo_command(target, release)
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect()
    }

    #[test]
    fn a_debug_build_carries_no_profile_flag() {
        for target in [
            CargoBuildTarget::Darwin,
            CargoBuildTarget::Ios {
                rust_target: "aarch64-apple-ios".to_string(),
            },
            CargoBuildTarget::IosSim {
                rust_target: "aarch64-apple-ios-sim".to_string(),
            },
            CargoBuildTarget::Android {
                rust_target: "aarch64-linux-android".to_string(),
            },
        ] {
            let args = cargo_args(&target, false);
            assert!(!args.contains(&"--release".to_string()), "{args:?}");
        }
    }

    #[test]
    fn a_release_build_asks_cargo_for_the_release_profile() {
        for target in [
            CargoBuildTarget::Darwin,
            CargoBuildTarget::Ios {
                rust_target: "aarch64-apple-ios".to_string(),
            },
            CargoBuildTarget::Android {
                rust_target: "aarch64-linux-android".to_string(),
            },
        ] {
            let args = cargo_args(&target, true);
            assert!(args.contains(&"--release".to_string()), "{args:?}");
        }
    }

    #[test]
    fn the_web_build_swaps_the_wasm_pack_profile_flag() {
        assert_eq!(
            cargo_args(&CargoBuildTarget::Web, false),
            vec!["build", "--debug", "--target", "web", "--out-dir", "builds/web/pkg"]
        );
        assert_eq!(
            cargo_args(&CargoBuildTarget::Web, true),
            vec!["build", "--release", "--target", "web", "--out-dir", "builds/web/pkg"]
        );
    }

    #[test]
    fn every_cargo_driven_build_still_asks_for_json_messages() {
        for target in [
            CargoBuildTarget::Darwin,
            CargoBuildTarget::Ios {
                rust_target: "aarch64-apple-ios".to_string(),
            },
            CargoBuildTarget::Android {
                rust_target: "aarch64-linux-android".to_string(),
            },
        ] {
            let args = cargo_args(&target, true);
            assert!(
                args.contains(&cargo_message::MESSAGE_FORMAT.to_string()),
                "{args:?}"
            );
        }
    }

    /// Collect every app log the reader thread produces for `input`, as the
    /// console draws them with source locations shown.
    fn app_logs(input: &str) -> Vec<String> {
        styled_app_logs(input)
            .iter()
            .map(|line| line.render(true))
            .collect()
    }

    /// Collect every app log the reader thread produces for `input`.
    fn styled_app_logs(input: &str) -> Vec<StyledLog> {
        let (tx, rx) = crossbeam::channel::unbounded();
        stream_stdout_as_app_log(Cursor::new(input.to_string()), tx);
        let mut lines = Vec::new();
        while let Ok(event) = rx.recv() {
            match event {
                RunnerEvent::AppLog(line) => lines.push(line),
                _ => panic!("expected an app log"),
            }
        }
        lines
    }

    #[test]
    fn reader_thread_renders_json_records() {
        let logs = app_logs(
            "{\"__aimer\":1,\"level\":\"error\",\"message\":\"boom\",\"file\":\"src/main.rs\",\"line\":3}\n",
        );

        assert_eq!(logs.len(), 1);
        assert!(logs[0].contains("[ERROR] boom"));
        assert!(logs[0].contains("(src/main.rs:3)"));
        assert!(!logs[0].contains("__aimer"));
    }

    #[test]
    fn reader_thread_keeps_the_source_location_toggleable() {
        let logs = styled_app_logs(
            "{\"__aimer\":1,\"level\":\"error\",\"message\":\"boom\",\"file\":\"src/main.rs\",\"line\":3}\nplain\n",
        );

        assert!(logs[0].has_location());
        assert!(!logs[0].render(false).contains("src/main.rs"));
        assert!(!logs[1].has_location());
    }

    #[test]
    fn reader_thread_passes_plain_output_through() {
        let logs = app_logs("plain println output\n");

        assert_eq!(logs, vec!["plain println output".to_string()]);
    }

    #[test]
    fn reader_thread_keeps_unparsable_json_verbatim() {
        let broken = r#"{"level":"info","message":oops}"#;
        let logs = app_logs(&format!("{broken}\n"));

        assert_eq!(logs, vec![broken.to_string()]);
    }

    #[test]
    fn reader_thread_preserves_line_order() {
        let logs = app_logs(
            "first\n{\"level\":\"info\",\"message\":\"second\"}\nthird\n",
        );

        assert_eq!(logs.len(), 3);
        assert_eq!(logs[0], "first");
        assert!(logs[1].contains("[INFO] second"));
        assert_eq!(logs[2], "third");
    }

    // ── Cargo JSON build output ──────────────────────────────────────

    /// Collect the build log lines the JSON reader thread produces for `input`,
    /// plus the report it hands back, as plain text.
    fn cargo_json_build(input: &str) -> (Vec<String>, Vec<String>) {
        let (tx, rx) = crossbeam::channel::unbounded();
        let reader = stream_cargo_json(Cursor::new(input.to_string()), tx);
        let report = reader.join().expect("reader thread");
        let mut lines = Vec::new();
        while let Ok(event) = rx.try_recv() {
            match event {
                RunnerEvent::BuildLog(line) => lines.push(strip_ansi(&line)),
                _ => panic!("expected a build log"),
            }
        }
        (
            lines,
            report.lines().iter().map(|l| strip_ansi(l)).collect(),
        )
    }

    /// A `compiler-message` line as cargo writes it, rendered the way rustc
    /// would print it.
    fn compiler_message(level: &str, message: &str, code: &str) -> String {
        format!(
            r#"{{"reason":"compiler-message","target":{{"name":"app","kind":["lib"]}},"message":{{"level":"{level}","message":"{message}","code":{{"code":"{code}"}},"spans":[{{"file_name":"src/lib.rs","line_start":4,"column_start":9,"is_primary":true}}],"rendered":"{level}[{code}]: {message}\n --> src/lib.rs:4:9\n"}}}}"#
        )
    }

    #[test]
    fn cargo_json_reader_replays_diagnostics_like_cargo() {
        let (lines, _) = cargo_json_build(&format!(
            "{}\n",
            compiler_message("error", "mismatched types", "E0308")
        ));

        assert_eq!(
            lines,
            vec![
                "error[E0308]: mismatched types".to_string(),
                " --> src/lib.rs:4:9".to_string(),
                // Cargo separates diagnostics with a blank line.
                String::new(),
            ]
        );
    }

    #[test]
    fn cargo_json_reader_hides_the_json_bookkeeping() {
        let (lines, report) = cargo_json_build(concat!(
            r#"{"reason":"compiler-artifact","target":{"name":"app","kind":["lib"]}}"#,
            "\n",
            r#"{"reason":"build-script-executed","package_id":"app"}"#,
            "\n",
            r#"{"reason":"build-finished","success":true}"#,
            "\n",
        ));

        assert!(lines.is_empty(), "{lines:?}");
        assert!(report.is_empty());
    }

    #[test]
    fn cargo_json_reader_forwards_output_that_is_not_a_cargo_message() {
        let (lines, report) = cargo_json_build("build script says hello\n");

        assert_eq!(lines, vec!["build script says hello".to_string()]);
        assert!(report.is_empty());
    }

    #[test]
    fn cargo_json_reader_collects_every_error_into_the_report() {
        let (lines, report) = cargo_json_build(&format!(
            "{}\n{}\n{}\n{}\n",
            compiler_message("warning", "unused variable", "W0001"),
            compiler_message("error", "mismatched types", "E0308"),
            compiler_message("error", "cannot find function", "E0425"),
            r#"{"reason":"build-finished","success":false}"#
        ));

        // Everything is still streamed as it arrives, in cargo's own shape.
        assert!(lines.iter().any(|l| l.contains("warning[W0001]")));
        assert!(lines.iter().any(|l| l.contains("error[E0308]")));

        // And the errors — only the errors — come back grouped.
        assert!(report.iter().any(|l| l.contains("Compile Error")));
        assert!(report.iter().any(|l| l.contains("mismatched types")));
        assert!(report.iter().any(|l| l.contains("cannot find function")));
        assert!(!report.iter().any(|l| l.contains("unused variable")));
        assert!(report.last().unwrap().contains("2 errors"));
    }

    #[test]
    fn cargo_json_reader_reports_nothing_for_a_clean_build() {
        let (_, report) = cargo_json_build(&format!(
            "{}\n{}\n",
            compiler_message("warning", "unused variable", "W0001"),
            r#"{"reason":"build-finished","success":true}"#
        ));

        assert!(report.is_empty());
    }

    #[test]
    fn cargo_json_reader_preserves_line_order() {
        let (lines, _) = cargo_json_build(&format!(
            "first\n{}\nlast\n",
            compiler_message("error", "boom", "E0001")
        ));

        assert_eq!(lines.first().unwrap(), "first");
        assert!(lines[1].contains("error[E0001]: boom"));
        assert_eq!(lines.last().unwrap(), "last");
        assert_eq!(lines.len(), 5, "{lines:?}");
    }

    #[test]
    fn cargo_json_is_requested_for_cargo_driven_targets_only() {
        assert!(CargoBuildTarget::Darwin.speaks_cargo_json());
        assert!(
            CargoBuildTarget::Ios {
                rust_target: "aarch64-apple-ios".to_string()
            }
            .speaks_cargo_json()
        );
        assert!(
            CargoBuildTarget::IosSim {
                rust_target: "aarch64-apple-ios-sim".to_string()
            }
            .speaks_cargo_json()
        );
        assert!(
            CargoBuildTarget::Android {
                rust_target: "arm64-v8a".to_string()
            }
            .speaks_cargo_json()
        );
        // wasm-pack drives cargo itself and drops the flag.
        assert!(!CargoBuildTarget::Web.speaks_cargo_json());
    }

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
