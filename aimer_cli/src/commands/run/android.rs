use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use crate::commands::assemble::{self, AndroidPlan};
use crate::commands::run::cargo_build::{
    self, CargoBuildTarget, stream_as_app_log_split_cr, wait_for_child,
};
use crate::commands::run::console::{RunnerEvent, Status};
use crate::commands::run::helpers::{
    ConsoleReporter, build_log, fail, run_to_completion, set_status, spawn_streamed,
};
use crate::commands::run::pipeline::{Flow, RunContext, Runner};
use crate::commands::run::utilities::{LogStyling, StyledLog};

/// Application id used when `build.gradle.kts.template` cannot be read.
const FALLBACK_APP_ID: &str = "com.example.app";

/// Parse a single `adb logcat` line into the styled text shown in the app log
/// pane.
///
/// The logcat framing is stripped first, so a JSON log record written by the app
/// is recognised by [`process_log`](LogStyling::process_log) and rendered from
/// its declared level; anything else keeps the plain logcat text.
fn parse_logcat_line(l: String) -> StyledLog {
    strip_logcat_framing(l).process_log()
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
            .args(["-s", &ctx.device.id, "shell", "getprop", "ro.product.cpu.abi"])
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
        let template = project_dir.join("app/build.gradle.kts.template");
        let Ok(content) = std::fs::read_to_string(template) else {
            return FALLBACK_APP_ID.to_string();
        };
        application_id_of(&content).unwrap_or_else(|| FALLBACK_APP_ID.to_string())
    }

    /// Wait for the launched app to appear in the process list and return its
    /// pid, so `logcat` can be filtered down to it.
    fn app_pid(ctx: &RunContext, app_id: &str) -> Option<String> {
        for _ in 0..10 {
            if let Ok(output) = Command::new("adb")
                .args(["-s", &ctx.device.id, "shell", "pidof", "-s", app_id])
                .output()
            {
                let out = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !out.is_empty() {
                    return Some(out);
                }
            }
            thread::sleep(Duration::from_millis(200));
        }
        None
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

        let app_id = Self::app_id(&assemble::android_project_dir());

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

        let mut logcat_cmd = Command::new("adb");
        logcat_cmd.args(["-s", &ctx.device.id, "logcat", "-v", "time"]);

        if let Some(pid) = Self::app_pid(ctx, &app_id) {
            logcat_cmd.args(["--pid", &pid]);
        }

        if !spawn_streamed(
            logcat_cmd,
            &ctx.tx,
            &ctx.current_child,
            "Failed to run logcat",
            Status::Error,
            |stdout, tx| {
                thread::spawn(move || {
                    let reader = BufReader::new(stdout);
                    for line in reader.lines().map_while(Result::ok) {
                        let _ = tx.send(RunnerEvent::AppLog(parse_logcat_line(line)));
                    }
                });
            },
            |stderr, tx| {
                thread::spawn(move || {
                    let reader = BufReader::new(stderr);
                    for line in reader.lines().map_while(Result::ok) {
                        let _ = tx.send(RunnerEvent::AppLog(parse_logcat_line(line)));
                    }
                });
            },
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
        let gradle = "android {\n    defaultConfig {\n        applicationId = \"com.acme.demo\"\n    }\n}";
        assert_eq!(
            application_id_of(gradle).as_deref(),
            Some("com.acme.demo")
        );
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
}
