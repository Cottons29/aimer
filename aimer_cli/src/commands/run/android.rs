use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use crossbeam::channel::Sender;
use crate::commands::assemble::{self, AndroidPlan};
use crate::commands::run::cargo_build::{
    self, CargoBuildTarget, stream_as_app_log_split_cr, wait_for_child,
};
use crate::commands::run::console::{RunnerEvent, Status};
use crate::commands::run::helpers::{
    ConsoleReporter, build_log, fail, run_to_completion, set_status, spawn_streamed,
};
use crate::commands::run::pipeline::{Flow, RunContext, Runner};
use crate::commands::run::utilities::{AppOutput, LogStyling};

/// Application id used when `build.gradle.kts.template` cannot be read.
const FALLBACK_APP_ID: &str = "com.example.app";

/// How long to wait for the launched app to show up in the process list, as a
/// number of [`PID_POLL_INTERVAL`] steps.
///
/// A cold start on an emulator regularly needs a couple of seconds, and the pid
/// is what keeps `logcat` scoped to the app — giving up on it early is what
/// turns the app log into a dump of the whole device.
const PID_POLL_ATTEMPTS: usize = 40;

/// Delay between two process-list lookups.
const PID_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Logcat tags the app itself writes under, used when the pid could not be
/// resolved and the log can only be narrowed down by tag.
///
/// `RustStdoutStderr` is the tag `android-activity` redirects the Rust
/// `stdout` / `stderr` to, so it carries everything the framework and the
/// application print — panics included. The rest are the tags a crash is
/// reported under: `AndroidRuntime` for an uncaught Java exception, `DEBUG` for
/// a native tombstone, `AimerActivity` for the generated activity itself.
const APP_LOG_TAGS: &[&str] = &[
    "RustStdoutStderr:V",
    "AimerActivity:V",
    "AndroidRuntime:E",
    "DEBUG:F",
];

/// Tags of the framework chatter that is logged *inside* the app's own process
/// and would otherwise survive the pid filter.
///
/// All of it is emitted by system libraries the app links against rather than by
/// the app, and none of it says anything about the program being run — property
/// lookups denied by SELinux, the dynamic loader, the graphics glue.
const SYSTEM_NOISE_TAGS: &[&str] = &[
    "libc",
    "linker",
    "nativeloader",
    "libEGL",
    "EGL_emulation",
    "eglCodecCommon",
    "GraphicsEnvironment",
    "Gralloc4",
    "ziparchive",
    "NetworkSecurityConfig",
    "ProfileInstaller",
];

/// Parse a single `adb logcat` line into the styled text shown in the app log
/// pane.
///
/// The logcat framing is stripped first, so a JSON log record written by the app
/// is recognised by
/// [`process_app_output`](LogStyling::process_app_output) and rendered from its
/// declared level — a recovered widget panic included; anything else keeps the
/// plain logcat text.
fn parse_logcat_line(l: String) -> AppOutput {
    strip_logcat_framing(l).process_app_output()
}

/// The tag of a `logcat -v time` line, i.e. the `SatelliteController` of
/// `08-03 01:50:36.063 D/SatelliteController( 1300): ...`.
///
/// `None` when the line carries no framing — a continuation line of a stack
/// trace, or output that never went through logcat at all.
fn logcat_tag(line: &str) -> Option<&str> {
    let (framing, _) = line.split_once('(')?;
    let (_, tag) = framing.rsplit_once('/')?;
    let tag = tag.trim();
    if tag.is_empty() { None } else { Some(tag) }
}

/// Whether `line` is framework chatter from one of the [`SYSTEM_NOISE_TAGS`]
/// rather than something the app said.
fn is_app_log(line: &str, app_id: &str) -> bool {

    if line.contains(app_id) && line.contains("RustStdoutStderr") {
        return true;
    }

    !logcat_tag(line).is_some_and(|tag| SYSTEM_NOISE_TAGS.contains(&tag))
}


/// The `adb logcat` arguments that narrow the log down to the app.
///
/// A pid is the exact filter and is used whenever the process could be found.
/// Without one, the log is restricted to the [`APP_LOG_TAGS`] — everything else
/// is silenced with `*:S` — because tailing the unfiltered device log buries the
/// app's own output under the chatter of every system service.
fn logcat_args(pid: Option<&str>) -> Vec<String> {
    let mut args = vec!["logcat".to_string(), "-v".to_string(), "time".to_string()];
    match pid {
        Some(pid) => {
            args.push("--pid".to_string());
            args.push(pid.to_string());
        }
        None => {
            args.extend(APP_LOG_TAGS.iter().map(|tag| tag.to_string()));
            args.push("*:S".to_string());
        }
    }
    args
}

/// Stream a `logcat` pipe into the app log pane, dropping the framework
/// chatter [`is_app_log`] recognises.
///
/// Filtering here rather than in `logcat`'s own expression keeps the device-side
/// filter to the one thing it does exactly — the process — while still leaving
/// the pane with the app's output only.
fn stream_logcat(pipe: impl Read + Send + 'static, tx: Sender<RunnerEvent>) {

    thread::spawn(move || {
        let reader = BufReader::new(pipe);
        let app_id = RUNNING_APP_ID.get().cloned().unwrap_or_default();
        for line in reader.lines().map_while(Result::ok) {
            if !is_app_log(&line, &app_id) {
                continue;
            }
            let _ = tx.send(parse_logcat_line(line).into());
        }
    });
}

/// Remove the `adb logcat` prefix from `l`, leaving just what the app wrote.
fn strip_logcat_framing(l: String) -> String {
    if l.contains("I/RustStdoutStderr")
        && let Some(item) = l.split_once("): ")
    {
        return item.1.replace("       ", " ");
    }

    match l.split_once("]") {
        Some((_, log)) => log.to_string(),
        None => l,
    }
}

/// The cargo build target an [`AndroidPlan`] compiles with.
fn build_target(plan: &AndroidPlan) -> CargoBuildTarget {
    CargoBuildTarget::Android {
        rust_target: plan.rust_target.to_string(),
    }
}

/// The Android leg of the unified pipeline: `cargo ndk build` → the shared
/// [`package_android`](assemble::package_android) step → `adb install` and
/// `am start`, then tail `logcat`.
pub struct AndroidRunner {
    /// Resolved in [`build`](Runner::build) from the device ABI, then reused by
    /// the later stages.
    plan: Option<AndroidPlan>,
    /// The APK the assemble stage produced, reused by the launch stage.
    apk_path: Option<String>,
}

impl AndroidRunner {
    #[inline]
    pub fn new() -> Self {
        Self {
            plan: None,
            apk_path: None,
        }
    }

    /// The plan resolved by the build stage, or `None` (after reporting it) when
    /// the pipeline somehow got here without one.
    fn plan(&self, ctx: &RunContext) -> Option<&AndroidPlan> {
        if self.plan.is_none() {
            fail(&ctx.tx, "Android build plan is missing.");
        }
        self.plan.as_ref()
    }

    /// The ABI the connected device reports.
    fn device_abi(&self, ctx: &RunContext) -> Option<String> {
        let output = match Command::new("adb")
            .args([
                "-s",
                &ctx.device.id,
                "shell",
                "getprop",
                "ro.product.cpu.abi",
            ])
            .output()
        {
            Ok(output) => output,
            Err(e) => {
                build_log(&ctx.tx, format!("Failed to get ABI: {}", e));
                set_status(&ctx.tx, Status::Idling);
                return None;
            }
        };

        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// The application id declared in the Android project template.
    fn app_id(project_dir: &Path) -> String {
        let template = project_dir.join("app/build.gradle.kts");
        let Ok(content) = std::fs::read_to_string(template) else {
            return FALLBACK_APP_ID.to_string();
        };
        application_id_of(&content).unwrap_or_else(|| FALLBACK_APP_ID.to_string())
    }

    /// Wait for the launched app to appear in the process list and return its
    /// pid, so `logcat` can be filtered down to it.
    fn app_pid(ctx: &RunContext, app_id: &str) -> Option<String> {
        for _ in 0..PID_POLL_ATTEMPTS {
            if let Ok(output) = Command::new("adb")
                .args(["-s", &ctx.device.id, "shell", "pidof", "-s", app_id])
                .output()
            {
                let out = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !out.is_empty() {
                    return Some(out);
                }
            }
            thread::sleep(PID_POLL_INTERVAL);
        }
        None
    }

    /// Drop everything the device logged so far.
    ///
    /// `logcat` replays its ring buffer before it starts following, so without
    /// this the pane opens on however many minutes of unrelated device history
    /// the buffer happens to hold. Called just before the app is launched, so
    /// nothing of the run itself is lost.
    fn clear_device_log(ctx: &RunContext) {
        let _ = Command::new("adb")
            .args(["-s", &ctx.device.id, "logcat", "-c"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

impl Default for AndroidRunner {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

/// Extract the `applicationId` from a Gradle build script.
fn application_id_of(gradle: &str) -> Option<String> {
    gradle
        .lines()
        .filter(|line| line.contains("applicationId"))
        .find_map(|line| line.split('"').nth(1))
        .map(str::to_string)
}

pub static RUNNING_APP_ID: OnceLock<String> = OnceLock::new();

impl Runner for AndroidRunner {
    fn build(&mut self, ctx: &RunContext) -> Flow {
        let Some(abi) = self.device_abi(ctx) else {
            return Flow::Abort;
        };
        let plan = AndroidPlan::for_abi(&abi);

        set_status(&ctx.tx, Status::Compiling(0));
        build_log(
            &ctx.tx,
            format!("Compiling shared library for {}...", plan.rust_target),
        );

        let Some(status) = cargo_build::spawn_cargo_build(
            &build_target(&plan),
            &ctx.tx,
            &ctx.current_child,
            ctx.inspector_address,
            ctx.inspector_port,
            ctx.release,
        ) else {
            return Flow::Abort;
        };

        if !status.success() {
            fail(&ctx.tx, "Cargo build failed.");
            return Flow::Abort;
        }

        self.plan = Some(plan);
        Flow::Continue
    }

    fn assemble(&mut self, ctx: &RunContext) -> Flow {
        let Some(plan) = self.plan(ctx).cloned() else {
            return Flow::Abort;
        };

        let reporter = ConsoleReporter::of(ctx);
        match assemble::package_android(&ctx.pkg_name, &plan, ctx.release, &reporter) {
            Ok(apk_path) => {
                self.apk_path = Some(apk_path);
                Flow::Continue
            }
            Err(e) => {
                fail(&ctx.tx, format!("{e:#}"));
                Flow::Abort
            }
        }
    }

    fn launch(&mut self, ctx: &RunContext) -> Flow {
        set_status(&ctx.tx, Status::Launching);

        let device_name = &ctx.device.name;
        build_log(&ctx.tx, format!("Installing app on {} ...", device_name));

        let apk_path = self
            .apk_path
            .clone()
            .unwrap_or_else(|| assemble::apk_path(ctx.release));

        let mut install = Command::new("adb");
        install
            .args(["-s", &ctx.device.id, "install", "-r", &apk_path])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if !run_to_completion(
            install,
            &ctx.tx,
            "Failed to install",
            &format!("Failed to install on {}", device_name),
        ) {
            return Flow::Abort;
        }

        build_log(&ctx.tx, "Launching app on Android device...");

        // The log is tailed from here on, so the device history stops now.
        Self::clear_device_log(ctx);

        let app_id = Self::app_id(&assemble::android_project_dir());
        let app_id_clone = app_id.clone();
        let _ = RUNNING_APP_ID.get_or_init(|| app_id_clone);

        let mut app_run = Command::new("adb");
        app_run.args([
            "-s",
            &ctx.device.id,
            "shell",
            "am",
            "start",
            "-n",
            &format!("{}/com.aimer.AimerActivity", app_id),
        ]);

        if !spawn_streamed(
            app_run,
            &ctx.tx,
            &ctx.current_child,
            "Failed to run app",
            Status::Idling,
            stream_as_app_log_split_cr,
            stream_as_app_log_split_cr,
        ) {
            return Flow::Abort;
        }

        // `am start` returns as soon as the activity is up; the app itself keeps
        // running on the device, so the pipeline follows it through logcat.
        wait_for_child(&ctx.current_child);

        let pid = Self::app_pid(ctx, &app_id);
        if pid.is_none() {
            build_log(
                &ctx.tx,
                format!("Could not find the app process; showing its log tags only. {app_id}"),
            );
        }

        let mut logcat_cmd = Command::new("adb");
        logcat_cmd
            .args(["-s", &ctx.device.id])
            .args(logcat_args(pid.as_deref()));

        if !spawn_streamed(
            logcat_cmd,
            &ctx.tx,
            &ctx.current_child,
            "Failed to run logcat",
            Status::Error,
            stream_logcat,
            stream_logcat,
        ) {
            return Flow::Abort;
        }

        Flow::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_target_follows_the_plan() {
        let CargoBuildTarget::Android { rust_target } =
            build_target(&AndroidPlan::for_abi("x86_64"))
        else {
            panic!("expected an Android build target");
        };
        assert_eq!(rust_target, "x86_64-linux-android");
    }

    #[test]
    fn application_id_is_read_from_the_gradle_template() {
        let gradle =
            "android {\n    defaultConfig {\n        applicationId = \"com.acme.demo\"\n    }\n}";
        assert_eq!(application_id_of(gradle).as_deref(), Some("com.acme.demo"));
    }

    #[test]
    fn application_id_is_absent_when_the_template_does_not_declare_one() {
        assert!(application_id_of("android { }").is_none());
    }

    #[test]
    fn app_id_falls_back_when_the_template_is_missing() {
        assert_eq!(
            AndroidRunner::app_id(Path::new("/nonexistent/project")),
            FALLBACK_APP_ID
        );
    }

    #[test]
    fn logcat_framing_is_stripped_from_rust_output() {
        let line = "01-01 00:00:00.000 I/RustStdoutStderr(1234): hello".to_string();
        assert_eq!(strip_logcat_framing(line), "hello");
    }

    #[test]
    fn the_tag_of_a_logcat_line_is_read_from_its_framing() {
        assert_eq!(
            logcat_tag("08-03 01:50:36.063 D/SatelliteController( 1300): config: null"),
            Some("SatelliteController")
        );
        assert_eq!(
            logcat_tag("01-01 00:00:00.000 I/RustStdoutStderr(1234): hello"),
            Some("RustStdoutStderr")
        );
        // A stack trace continuation, and plain output, carry no framing.
        assert_eq!(logcat_tag("\tat java.lang.Thread.run(Thread.java:1571)"), None);
        assert_eq!(logcat_tag("plain println output"), None);
    }

    #[test]
    fn logcat_is_scoped_to_the_app_process_when_its_pid_is_known() {
        let args = logcat_args(Some("4321"));
        assert_eq!(
            args,
            vec!["logcat", "-v", "time", "--pid", "4321"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn logcat_falls_back_to_the_app_tags_without_a_pid() {
        // The whole point: an unfiltered `logcat` drowns the app in the log of
        // every system service, so the tags are the fallback, never "everything".
        let args = logcat_args(None);
        assert!(!args.contains(&"--pid".to_string()), "{args:?}");
        assert!(args.contains(&"RustStdoutStderr:V".to_string()), "{args:?}");
        assert_eq!(args.last().unwrap(), "*:S", "{args:?}");
    }

    fn set_global_app_id() -> &'static str {
        RUNNING_APP_ID.get_or_init(|| FALLBACK_APP_ID.to_string())
    }

    #[test]
    fn framework_chatter_inside_the_app_process_is_dropped() {
        assert!(!is_app_log(
            "08-03 01:50:59.852 W/libc    ( 5818): Access denied finding property \"x\"",set_global_app_id()
        ));
        assert!(!is_app_log(
            "08-03 01:50:59.852 D/nativeloader( 5818): classloader namespace",set_global_app_id()
        ));
    }

    #[test]
    fn the_apps_own_output_is_never_dropped() {
        for line in [
            "01-01 00:00:00.000 I/RustStdoutStderr(1234): hello",
            "01-01 00:00:00.000 E/AndroidRuntime(1234): FATAL EXCEPTION: main",
            "\tat com.aimer.AimerActivity.onCreate(AimerActivity.kt:31)",
            "plain println output",
        ] {
            assert!(is_app_log(line,set_global_app_id()), "{line:?}");
        }
    }

    #[test]
    fn the_app_log_pane_only_sees_what_survived_the_filter() {
        let (tx, rx) = crossbeam::channel::unbounded();
        stream_logcat(
            std::io::Cursor::new(
                "08-03 01:50:59.852 W/libc    ( 5818): Access denied finding property \"x\"\n\
                 01-01 00:00:00.000 I/RustStdoutStderr(1234): hello\n"
                    .to_string(),
            ),
            tx,
        );

        let logs: Vec<String> = rx
            .iter()
            .map(|event| match event {
                RunnerEvent::AppLog(line) => line.render(true),
                _ => panic!("expected an app log"),
            })
            .collect();
        assert_eq!(logs, vec!["hello".to_string()]);
    }
}
