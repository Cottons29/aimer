use std::env::current_dir;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use crate::commands::run::cargo_build::{
    self, CargoBuildTarget, stream_as_app_log_split_cr, stream_stderr_as_build_log,
    stream_stdout_with_gradle_progress, wait_for_child,
};
use crate::commands::run::console::{RunnerEvent, Status};
use crate::commands::run::helpers::{
    build_log, build_streamed, fail, run_to_completion, set_status, spawn_streamed, stage_assets,
};
use crate::commands::run::pipeline::{Flow, RunContext, Runner};
use crate::commands::run::utilities::{LogStyling, StyledLog, resolve_lib_path};

/// The APK Gradle's `assembleDebug` produces.
const APK_PATH: &str = "builds/android/app/build/outputs/apk/debug/app-debug.apk";

/// Application id used when `build.gradle.kts.template` cannot be read.
const FALLBACK_APP_ID: &str = "com.example.app";

fn resolve_compatible_java_home() -> Option<String> {
    if cfg!(target_os = "macos") {
        for version in ["17", "21", "23", "11"] {
            let Ok(output) = std::process::Command::new("/usr/libexec/java_home")
                .arg("-v")
                .arg(version)
                .output()
            else {
                continue;
            };
            if !output.status.success() {
                continue;
            }
            if let Ok(path) = String::from_utf8(output.stdout) {
                return Some(path.trim().to_string());
            }
        }
    }
    None
}

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

/// The Rust target and JNI directory matching the connected device's ABI.
#[derive(Clone, Debug, PartialEq, Eq)]
struct AndroidPlan {
    rust_target: &'static str,
    jni_dir: &'static str,
}

impl AndroidPlan {
    /// Map the `ro.product.cpu.abi` property of the device to a build plan,
    /// defaulting to 64-bit ARM for an unknown ABI.
    fn for_abi(abi: &str) -> Self {
        let (rust_target, jni_dir) = match abi {
            "x86_64" => ("x86_64-linux-android", "x86_64"),
            "armeabi-v7a" => ("armv7-linux-androideabi", "armeabi-v7a"),
            "x86" => ("i686-linux-android", "x86"),
            _ => ("aarch64-linux-android", "arm64-v8a"),
        };
        Self {
            rust_target,
            jni_dir,
        }
    }

    /// The cargo build target this plan compiles with.
    fn build_target(&self) -> CargoBuildTarget {
        CargoBuildTarget::Android {
            rust_target: self.rust_target.to_string(),
        }
    }

    /// Where Gradle expects the shared library for this ABI.
    fn jni_libs_dir(&self) -> String {
        format!("builds/android/app/src/main/jniLibs/{}", self.jni_dir)
    }
}

/// The Android leg of the unified pipeline: `cargo ndk build` → Gradle
/// `assembleDebug` → `adb install` and `am start`, then tail `logcat`.
pub struct AndroidRunner {
    /// Resolved in [`build`](Runner::build) from the device ABI, then reused by
    /// the later stages.
    plan: Option<AndroidPlan>,
}

impl AndroidRunner {
    #[inline]
    pub fn new() -> Self {
        Self { plan: None }
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

    /// Absolute path of the generated Android project.
    fn project_dir() -> PathBuf {
        current_dir().unwrap_or_default().join("builds/android")
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
            &plan.build_target(),
            &ctx.tx,
            &ctx.current_child,
            ctx.inspector_address,
            ctx.inspector_port,
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
        let Some(plan) = self.plan(ctx) else {
            return Flow::Abort;
        };

        let project_dir = Self::project_dir();
        build_log(
            &ctx.tx,
            format!("[Aimer] current_dir: {}", project_dir.display()),
        );

        let lib_name = ctx.pkg_name.replace("-", "_");
        let src_lib = resolve_lib_path(&lib_name, plan.rust_target, plan.build_target());
        let dest_dir = plan.jni_libs_dir();
        let dest_lib = format!("{}/lib{}.so", dest_dir, lib_name);

        std::fs::create_dir_all(&dest_dir).unwrap_or_default();
        if std::fs::copy(&src_lib, &dest_lib).is_ok() {
            build_log(&ctx.tx, format!("Copied library to {}", dest_lib));
        }

        // Stage registered assets into the APK's `assets/` source set (incrementally)
        // before Gradle packs it, so they are readable at runtime via AssetManager.
        stage_assets(&ctx.tx, "builds/android/app/src/main/assets");

        build_log(&ctx.tx, "Building Android project via Gradle...");

        let gradlew = if cfg!(windows) {
            "gradlew.bat"
        } else {
            "gradlew"
        };

        let mut cmd = Command::new(project_dir.join(gradlew));
        cmd.arg("assembleDebug").current_dir(&project_dir);

        if let Some(java_home) = resolve_compatible_java_home() {
            build_log(&ctx.tx, format!("Using JAVA_HOME: {}", java_home));
            cmd.env("JAVA_HOME", java_home);
        }

        if !build_streamed(
            cmd,
            &ctx.tx,
            &ctx.current_child,
            "Failed to run gradle",
            "Gradle build failed.",
            stream_stdout_with_gradle_progress,
            stream_stderr_as_build_log,
        ) {
            return Flow::Abort;
        }

        Flow::Continue
    }

    fn launch(&mut self, ctx: &RunContext) -> Flow {
        set_status(&ctx.tx, Status::Launching);

        let device_name = &ctx.device.name;
        build_log(&ctx.tx, format!("Installing app on {} ...", device_name));

        let mut install = Command::new("adb");
        install
            .args(["-s", &ctx.device.id, "install", "-r", APK_PATH])
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

        let app_id = Self::app_id(&Self::project_dir());

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
    fn plan_maps_every_known_abi() {
        for (abi, rust_target, jni_dir) in [
            ("x86_64", "x86_64-linux-android", "x86_64"),
            ("armeabi-v7a", "armv7-linux-androideabi", "armeabi-v7a"),
            ("x86", "i686-linux-android", "x86"),
            ("arm64-v8a", "aarch64-linux-android", "arm64-v8a"),
        ] {
            let plan = AndroidPlan::for_abi(abi);
            assert_eq!(plan.rust_target, rust_target, "{abi}");
            assert_eq!(plan.jni_dir, jni_dir, "{abi}");
        }
    }

    #[test]
    fn plan_falls_back_to_arm64_for_an_unknown_abi() {
        assert_eq!(
            AndroidPlan::for_abi("mips"),
            AndroidPlan::for_abi("arm64-v8a")
        );
    }

    #[test]
    fn jni_libs_dir_follows_the_abi() {
        assert_eq!(
            AndroidPlan::for_abi("x86_64").jni_libs_dir(),
            "builds/android/app/src/main/jniLibs/x86_64"
        );
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
