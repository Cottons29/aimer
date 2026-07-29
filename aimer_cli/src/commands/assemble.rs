pub(crate) mod link_flags;

use std::env::current_dir;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, bail};

use crate::commands::run::helpers::host_arch;
use crate::commands::run::utilities::get_project_root;
use crate::config::resolve_package_name;
use crate::errors::AimerError;
use crate::targets::Targets;

/// The Rust target the macOS app is compiled for.
pub(crate) const MACOS_RUST_TARGET: &str = "aarch64-apple-darwin";

/// ABI assumed when no device is involved, i.e. for `aimer assemble android`.
pub(crate) const ANDROID_DEFAULT_ABI: &str = "arm64-v8a";

/// The generated Xcode project of the iOS app, relative to the project root.
pub(crate) const IOS_PROJECT_DIR: &str = "builds/ios";

/// Non-interactive bundling entry point used by `aimer assemble <platform>`.
///
/// Unlike `aimer build` (which only compiles the Rust library) this command
/// produces the *distributable platform bundle* — a `.app` on macOS/iOS, an
/// `.apk` on Android, or the static web `dist/` directory — in either debug or
/// release mode.
///
/// The packaging steps themselves ([`package_macos`], [`package_ios`],
/// [`package_android`]) are shared with the interactive `aimer run` pipeline;
/// the only difference is the [`Reporter`] they are handed. Here that is
/// [`StdioReporter`], which runs each step synchronously with inherited stdio so
/// it is friendly to CI logs.
pub fn execute(platform: String, release: bool) -> anyhow::Result<()> {
    let target = Targets::try_from(platform.as_str())
        .map_err(|_| AimerError::UnknownTarget(platform.clone()))?;

    println!(
        "Assembling '{target}' bundle in {} mode...",
        profile_name(release)
    );

    let pkg_name = resolve_package_name(Path::new("."));
    let reporter = StdioReporter;

    let artifact = match target {
        Targets::Macos => {
            build_macos(release, &reporter)?;
            package_macos(&pkg_name, release, &reporter)?
        }
        Targets::Ios | Targets::IosSimulator => {
            let plan = IosPlan::resolve(target == Targets::IosSimulator);
            build_ios(&plan, release, &reporter)?;
            package_ios(&pkg_name, &plan, release, &reporter)?
        }
        Targets::Android | Targets::AndroidSimulator => {
            let plan = AndroidPlan::for_abi(ANDROID_DEFAULT_ABI);
            build_android(&plan, release, &reporter)?;
            package_android(&pkg_name, &plan, release, &reporter)?
        }
        Targets::Web => assemble_web(release, &reporter)?,
        Targets::Windows | Targets::Linux => assemble_desktop(target, release, &reporter)?,
        Targets::Terminated => bail!("'terminated' is not an assemblable platform"),
    };

    println!("Bundle assembled successfully: {artifact}");
    Ok(())
}

/// The tool an assemble step runs, so a [`Reporter`] can pick the right
/// progress parsing for its output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StepKind {
    /// `cargo` or `cargo ndk`.
    Cargo,
    /// `xcodebuild`.
    Xcode,
    /// The Gradle wrapper.
    Gradle,
    /// Anything else, e.g. `trunk`.
    Other,
}

/// One command an assemble stage runs, together with the human readable action
/// used in its progress and error messages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Step {
    pub kind: StepKind,
    pub action: String,
}

impl Step {
    /// A step running `kind` to carry out `action`.
    #[inline]
    pub(crate) fn new(kind: StepKind, action: impl Into<String>) -> Self {
        Self {
            kind,
            action: action.into(),
        }
    }
}

/// Where an assemble step sends its output.
///
/// The same packaging code serves `aimer assemble`, which prints synchronously
/// to stdout ([`StdioReporter`]), and `aimer run`, which streams every line into
/// the TUI console
/// ([`ConsoleReporter`](crate::commands::run::helpers::ConsoleReporter)).
pub(crate) trait Reporter {
    /// Report one progress line.
    fn note(&self, message: String);

    /// Run `cmd` to completion, reporting its output, and fail when it cannot be
    /// started or exits with a non-zero status.
    fn run(&self, cmd: Command, step: Step) -> anyhow::Result<()>;

    /// Stage every file registered under `[assets]` in `Aimer.toml` into
    /// `dest_root`.
    ///
    /// Copying is incremental — only new or changed files are written — so a
    /// rebuild does not re-copy unchanged assets. A registered file that does
    /// not exist is reported as a warning and skipped; only a broken asset
    /// registry is an error.
    fn stage_assets(&self, dest_root: &str) -> anyhow::Result<()> {
        self.note(format!("Copying assets into {dest_root}"));
        let report = crate::commands::assets::copy_assets_into(dest_root)?;
        for rel in &report.copied {
            self.note(format!(
                "Copied asset {rel} -> {}",
                Path::new(dest_root).join(rel).display()
            ));
        }
        for rel in &report.missing {
            self.note(format!(
                "warning: registered asset '{rel}' not found; skipping"
            ));
        }
        Ok(())
    }
}

/// [`Reporter`] for the non-interactive `aimer assemble`: every step inherits
/// this process' stdio and progress goes to stdout.
pub(crate) struct StdioReporter;

impl Reporter for StdioReporter {
    fn note(&self, message: String) {
        println!("{message}");
    }

    fn run(&self, mut cmd: Command, step: Step) -> anyhow::Result<()> {
        cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
        let status = cmd
            .status()
            .with_context(|| format!("failed to start {}", step.action))?;
        if !status.success() {
            bail!("{} failed", step.action);
        }
        Ok(())
    }
}

/// The Cargo profile directory name for the requested build mode.
pub(crate) fn profile_name(release: bool) -> &'static str {
    if release { "release" } else { "debug" }
}

/// The Xcode `-configuration` value for the requested build mode.
pub(crate) fn xcode_configuration(release: bool) -> &'static str {
    if release { "Release" } else { "Debug" }
}

/// The Gradle assemble task for the requested build mode.
pub(crate) fn gradle_task(release: bool) -> &'static str {
    if release {
        "assembleRelease"
    } else {
        "assembleDebug"
    }
}

/// Absolute path of the compiled Rust artifact for `rust_target`/`profile`.
pub(crate) fn artifact_path(
    rust_target: &str,
    lib_name: &str,
    release: bool,
    extension: &str,
) -> String {
    let root = get_project_root(true).unwrap_or_else(|_| current_dir().unwrap());
    format!(
        "{}/target/{}/{}/lib{}{}",
        root.display(),
        rust_target,
        profile_name(release),
        lib_name,
        extension
    )
}

/// The library name cargo derives from a package name.
pub(crate) fn lib_name_of(pkg_name: &str) -> String {
    pkg_name.replace('-', "_")
}

/// Copy a freshly compiled native library into `dest_dir`, creating the
/// directory tree first.
fn copy_lib(
    src: &str,
    dest_dir: &str,
    lib_file: &str,
    reporter: &dyn Reporter,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(dest_dir).with_context(|| format!("creating {dest_dir}"))?;
    let dest = format!("{dest_dir}/{lib_file}");
    std::fs::copy(src, &dest)
        .with_context(|| format!("copying native library '{src}' -> '{dest}'"))?;
    reporter.note(format!("Copied library to {dest}"));
    Ok(())
}

/// An `xcodebuild` invocation for the project in `project_dir`.
fn xcode_command(
    project_dir: &str,
    pkg_name: &str,
    configuration: &str,
    sdk: Option<&str>,
    arch: &str,
) -> Command {
    let mut cmd = Command::new("xcodebuild");
    cmd.arg("-project")
        .arg(format!("{pkg_name}.xcodeproj"))
        .arg("-target")
        .arg(pkg_name)
        .arg("-configuration")
        .arg(configuration);
    if let Some(sdk) = sdk {
        cmd.arg("-sdk").arg(sdk);
    }
    cmd.arg("SYMROOT=build")
        .arg("-arch")
        .arg(arch)
        .current_dir(project_dir);
    cmd
}

/// The `.app` bundle `xcodebuild` produces for a macOS package.
pub(crate) fn macos_app_path(pkg_name: &str, release: bool) -> String {
    format!(
        "builds/macos/build/{}/{}.app",
        xcode_configuration(release),
        pkg_name
    )
}

/// Remove `path` if it exists, so a stale executable or asset can never be
/// launched when the packager decides it has nothing to do.
fn clean_bundle(path: &str) -> anyhow::Result<()> {
    let bundle = Path::new(path);
    if bundle.exists() {
        std::fs::remove_dir_all(bundle).with_context(|| format!("cleaning {path}"))?;
    }
    Ok(())
}

/// Compile the Rust library for macOS.
fn build_macos(release: bool, reporter: &dyn Reporter) -> anyhow::Result<()> {
    let mut cargo = Command::new("cargo");
    cargo
        .arg("build")
        .args(["--target", MACOS_RUST_TARGET, "--lib"]);
    if release {
        cargo.arg("--release");
    }
    reporter.run(cargo, Step::new(StepKind::Cargo, "cargo build for macOS"))
}

/// Turn the compiled macOS library into a launchable `.app` bundle and return
/// its path.
///
/// Shared by `aimer assemble macos` and the macOS leg of the `aimer run`
/// pipeline: the static archive is staged into the Xcode project, the bundle is
/// rebuilt from scratch and the registered assets land in `Contents/Resources`.
pub(crate) fn package_macos(
    pkg_name: &str,
    release: bool,
    reporter: &dyn Reporter,
) -> anyhow::Result<String> {
    let lib_name = lib_name_of(pkg_name);
    let src_lib = artifact_path(MACOS_RUST_TARGET, &lib_name, release, ".a");
    copy_lib(
        &src_lib,
        "builds/macos/Libraries",
        &format!("lib{lib_name}.a"),
        reporter,
    )?;

    let artifact = macos_app_path(pkg_name, release);
    clean_bundle(&artifact)?;

    let arch = host_arch();
    reporter.note(format!("Building Xcode project for {arch}..."));
    reporter.run(
        xcode_command(
            "builds/macos",
            pkg_name,
            xcode_configuration(release),
            None,
            arch,
        ),
        Step::new(StepKind::Xcode, "xcodebuild for macOS"),
    )?;

    reporter.stage_assets(&format!("{artifact}/Contents/Resources"))?;
    Ok(artifact)
}

/// Everything that differs between an iOS device build and a simulator build,
/// resolved once from the flavour and the host architecture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IosPlan {
    /// Whether the bundle targets the simulator rather than a device.
    pub simulator: bool,
    /// Rust target triple the static library is compiled for.
    pub rust_target: &'static str,
    /// `xcodebuild -sdk` value, which also names the build subdirectory.
    pub sdk: &'static str,
    /// `xcodebuild -arch` value.
    pub arch: &'static str,
}

impl IosPlan {
    /// Resolve the plan for a device (`simulator = false`) or the simulator on
    /// this host.
    pub(crate) fn resolve(simulator: bool) -> Self {
        let arch = host_arch();
        if simulator {
            Self {
                simulator,
                rust_target: if arch == "x86_64" {
                    "x86_64-apple-ios"
                } else {
                    "aarch64-apple-ios-sim"
                },
                sdk: "iphonesimulator",
                arch,
            }
        } else {
            Self {
                simulator,
                rust_target: "aarch64-apple-ios",
                sdk: "iphoneos",
                arch,
            }
        }
    }

    /// The `.app` bundle `xcodebuild` produces for `pkg_name`.
    pub(crate) fn app_path(&self, pkg_name: &str, release: bool) -> String {
        format!(
            "builds/ios/build/{}-{}/{}.app",
            xcode_configuration(release),
            self.sdk,
            pkg_name
        )
    }
}

/// Compile the Rust library for iOS.
///
/// This goes through `cargo rustc` rather than `cargo build` for one reason:
/// the trailing `--print native-static-libs` makes the compiler write down
/// which system frameworks the crate graph links against, which
/// [`package_ios`] then feeds to Xcode. See [`link_flags`].
fn build_ios(plan: &IosPlan, release: bool, reporter: &dyn Reporter) -> anyhow::Result<()> {
    let mut cargo = Command::new("cargo");
    cargo
        .arg("rustc")
        .arg("--lib")
        .arg("--target")
        .arg(plan.rust_target);
    if release {
        cargo.arg("--release");
    }
    cargo
        .arg("--")
        .arg("--print")
        .arg(link_flags::print_arg(plan.rust_target, release)?);
    reporter.run(cargo, Step::new(StepKind::Cargo, "cargo build for iOS"))
}

/// Turn the compiled iOS library into a launchable `.app` bundle and return its
/// path.
///
/// Shared by `aimer assemble ios` and the iOS leg of the `aimer run` pipeline.
pub(crate) fn package_ios(
    pkg_name: &str,
    plan: &IosPlan,
    release: bool,
    reporter: &dyn Reporter,
) -> anyhow::Result<String> {
    let artifact = plan.app_path(pkg_name, release);
    clean_bundle(&artifact)?;

    let lib_name = lib_name_of(pkg_name);
    let src_lib = artifact_path(plan.rust_target, &lib_name, release, ".a");
    copy_lib(
        &src_lib,
        "builds/ios/Libraries",
        &format!("lib{lib_name}.a"),
        reporter,
    )?;

    // The archive alone does not tell Xcode which frameworks to link, so the
    // list the compiler just wrote is turned into the project's xcconfig.
    link_flags::refresh(
        Path::new(IOS_PROJECT_DIR),
        &link_flags::raw_path(plan.rust_target, release),
    )?;

    reporter.note("Building Xcode project for iOS...".to_string());
    reporter.run(
        xcode_command(
            IOS_PROJECT_DIR,
            pkg_name,
            xcode_configuration(release),
            Some(plan.sdk),
            plan.arch,
        ),
        Step::new(StepKind::Xcode, ("xcodebuild for iOS")),
    )?;

    reporter.stage_assets(&artifact)?;
    Ok(artifact)
}

/// The Rust target and JNI directory matching an Android ABI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AndroidPlan {
    pub rust_target: &'static str,
    pub jni_dir: &'static str,
}

impl AndroidPlan {
    /// Map the `ro.product.cpu.abi` property of a device to a build plan,
    /// defaulting to 64-bit ARM for an unknown ABI.
    pub(crate) fn for_abi(abi: &str) -> Self {
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

    /// Where Gradle expects the shared library for this ABI.
    pub(crate) fn jni_libs_dir(&self) -> String {
        format!("builds/android/app/src/main/jniLibs/{}", self.jni_dir)
    }
}

/// Absolute path of the generated Android project.
pub(crate) fn android_project_dir() -> PathBuf {
    current_dir().unwrap_or_default().join("builds/android")
}

/// A Gradle wrapper invocation assembling the APK for the requested mode.
fn gradle_command(project_dir: &Path, release: bool) -> Command {
    let gradlew = if cfg!(windows) {
        "gradlew.bat"
    } else {
        "gradlew"
    };
    let mut cmd = Command::new(project_dir.join(gradlew));
    cmd.arg(gradle_task(release)).current_dir(project_dir);
    cmd
}

/// The APKs Gradle may have produced for `release`, most preferred first.
///
/// Unsigned release builds (no signing config) are emitted with an `-unsigned`
/// suffix, so both names are candidates.
pub(crate) fn apk_candidates(release: bool) -> Vec<String> {
    if release {
        vec![
            "builds/android/app/build/outputs/apk/release/app-release.apk".to_string(),
            "builds/android/app/build/outputs/apk/release/app-release-unsigned.apk".to_string(),
        ]
    } else {
        vec!["builds/android/app/build/outputs/apk/debug/app-debug.apk".to_string()]
    }
}

/// The APK Gradle produced, or the output directory when none of the known
/// names exists.
pub(crate) fn apk_path(release: bool) -> String {
    apk_candidates(release)
        .into_iter()
        .find(|p| Path::new(p).exists())
        .unwrap_or_else(|| {
            format!(
                "builds/android/app/build/outputs/apk/{}/",
                profile_name(release)
            )
        })
}

/// Compile the Rust library for Android through `cargo ndk`.
fn build_android(plan: &AndroidPlan, release: bool, reporter: &dyn Reporter) -> anyhow::Result<()> {
    let mut cargo = Command::new("cargo");
    cargo
        .arg("ndk")
        .arg("-t")
        .arg(plan.jni_dir)
        .arg("build")
        .arg("--lib");
    if release {
        cargo.arg("--release");
    }
    reporter.run(
        cargo,
        Step::new(StepKind::Cargo, "cargo ndk build for Android"),
    )
}

/// Turn the compiled Android library into an installable APK and return its
/// path.
///
/// Shared by `aimer assemble android` and the Android leg of the `aimer run`
/// pipeline: the shared object is staged into `jniLibs`, the registered assets
/// into the APK's `assets/` source set, then Gradle packs it.
pub(crate) fn package_android(
    pkg_name: &str,
    plan: &AndroidPlan,
    release: bool,
    reporter: &dyn Reporter,
) -> anyhow::Result<String> {
    let lib_name = lib_name_of(pkg_name);
    let src_lib = artifact_path(plan.rust_target, &lib_name, release, ".so");
    copy_lib(
        &src_lib,
        &plan.jni_libs_dir(),
        &format!("lib{lib_name}.so"),
        reporter,
    )?;

    // Stage assets into the APK's `assets/` source set before Gradle packs it,
    // so they are readable at runtime via AssetManager.
    reporter.stage_assets("builds/android/app/src/main/assets")?;

    let project_dir = android_project_dir();
    reporter.note("Building Android project via Gradle...".to_string());

    let mut gradle = gradle_command(&project_dir, release);
    if let Some(java_home) = resolve_compatible_java_home() {
        reporter.note(format!("Using JAVA_HOME: {java_home}"));
        gradle.env("JAVA_HOME", java_home);
    }
    reporter.run(
        gradle,
        Step::new(StepKind::Gradle, "Gradle assemble for Android"),
    )?;

    Ok(apk_path(release))
}

/// Build the static web bundle via `trunk`.
fn assemble_web(release: bool, reporter: &dyn Reporter) -> anyhow::Result<String> {
    // Trunk cleans dist/ before building, so the registered assets are staged
    // next to the sources it copies from instead of into dist/ directly.
    reporter.stage_assets("builds/web")?;

    let llvm_ar = crate::commands::run::web::find_llvm_ar().context("Failed to find llvm-ar")?;
    let mut trunk = Command::new("trunk");
    crate::commands::run::web::configure_trunk(&mut trunk, &llvm_ar);
    trunk.arg("build").current_dir("builds/web");
    if release {
        trunk.arg("--release");
    }
    reporter.run(trunk, Step::new(StepKind::Other, "trunk build for web"))?;

    Ok("builds/web/dist".to_string())
}

/// Build the desktop (Windows/Linux) library. No platform installer template
/// exists yet, so this compiles the artifact and reports its directory.
fn assemble_desktop(
    target: Targets,
    release: bool,
    reporter: &dyn Reporter,
) -> anyhow::Result<String> {
    let mut cargo = Command::new("cargo");
    cargo.arg("build").arg("--lib");
    if release {
        cargo.arg("--release");
    }
    reporter.run(
        cargo,
        Step::new(StepKind::Cargo, format!("cargo build for {target}")),
    )?;

    let artifact = format!("target/{}", profile_name(release));
    reporter.stage_assets(&format!("{artifact}/assets"))?;
    Ok(artifact)
}

/// Locate a Gradle-compatible `JAVA_HOME` on macOS, preferring LTS releases.
pub(crate) fn resolve_compatible_java_home() -> Option<String> {
    if cfg!(target_os = "macos") {
        for version in ["17", "21", "23", "11"] {
            let Ok(output) = Command::new("/usr/libexec/java_home")
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// A [`Reporter`] that records the notes and the steps it was asked to run
    /// instead of touching the machine.
    struct SpyReporter {
        notes: Mutex<Vec<String>>,
        steps: Mutex<Vec<(Step, Vec<String>)>>,
    }

    impl SpyReporter {
        fn new() -> Self {
            Self {
                notes: Mutex::new(Vec::new()),
                steps: Mutex::new(Vec::new()),
            }
        }
    }

    impl Reporter for SpyReporter {
        fn note(&self, message: String) {
            self.notes.lock().unwrap().push(message);
        }

        fn run(&self, cmd: Command, step: Step) -> anyhow::Result<()> {
            self.steps.lock().unwrap().push((step, args_of(&cmd)));
            Ok(())
        }

        fn stage_assets(&self, dest_root: &str) -> anyhow::Result<()> {
            self.note(format!("staged {dest_root}"));
            Ok(())
        }
    }

    /// The arguments of `cmd`, as lossy strings.
    fn args_of(cmd: &Command) -> Vec<String> {
        cmd.get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect()
    }

    #[test]
    fn profile_name_maps_release_flag() {
        assert_eq!(profile_name(true), "release");
        assert_eq!(profile_name(false), "debug");
    }

    #[test]
    fn xcode_configuration_maps_release_flag() {
        assert_eq!(xcode_configuration(true), "Release");
        assert_eq!(xcode_configuration(false), "Debug");
    }

    #[test]
    fn gradle_task_maps_release_flag() {
        assert_eq!(gradle_task(true), "assembleRelease");
        assert_eq!(gradle_task(false), "assembleDebug");
    }

    #[test]
    fn artifact_path_uses_profile_dir() {
        let debug = artifact_path("aarch64-apple-darwin", "my_app", false, ".a");
        let release = artifact_path("aarch64-apple-darwin", "my_app", true, ".a");
        assert!(debug.ends_with("/target/aarch64-apple-darwin/debug/libmy_app.a"));
        assert!(release.ends_with("/target/aarch64-apple-darwin/release/libmy_app.a"));
    }

    #[test]
    fn lib_name_replaces_dashes() {
        assert_eq!(lib_name_of("my-cool-app"), "my_cool_app");
    }

    #[test]
    fn macos_app_path_follows_the_configuration() {
        assert_eq!(
            macos_app_path("my_app", false),
            "builds/macos/build/Debug/my_app.app"
        );
        assert_eq!(
            macos_app_path("my_app", true),
            "builds/macos/build/Release/my_app.app"
        );
    }

    #[test]
    fn device_plan_targets_the_iphoneos_sdk() {
        let plan = IosPlan::resolve(false);
        assert!(!plan.simulator);
        assert_eq!(plan.rust_target, "aarch64-apple-ios");
        assert_eq!(plan.sdk, "iphoneos");
    }

    #[test]
    fn simulator_plan_follows_the_host_architecture() {
        let plan = IosPlan::resolve(true);
        assert!(plan.simulator);
        assert_eq!(plan.sdk, "iphonesimulator");
        let expected = if plan.arch == "x86_64" {
            "x86_64-apple-ios"
        } else {
            "aarch64-apple-ios-sim"
        };
        assert_eq!(plan.rust_target, expected);
    }

    #[test]
    fn ios_app_path_follows_the_sdk_and_configuration() {
        assert_eq!(
            IosPlan::resolve(false).app_path("my_app", false),
            "builds/ios/build/Debug-iphoneos/my_app.app"
        );
        assert_eq!(
            IosPlan::resolve(true).app_path("my_app", true),
            "builds/ios/build/Release-iphonesimulator/my_app.app"
        );
    }

    #[test]
    fn android_plan_maps_every_known_abi() {
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
    fn android_plan_falls_back_to_arm64_for_an_unknown_abi() {
        assert_eq!(
            AndroidPlan::for_abi("mips"),
            AndroidPlan::for_abi(ANDROID_DEFAULT_ABI)
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
    fn xcode_command_carries_the_configuration_and_no_sdk_on_macos() {
        let cmd = xcode_command("builds/macos", "my_app", "Release", None, "arm64");
        let args = args_of(&cmd);
        assert!(args.contains(&"-configuration".to_string()));
        assert!(args.contains(&"Release".to_string()));
        assert!(!args.contains(&"-sdk".to_string()));
        assert!(args.contains(&"my_app.xcodeproj".to_string()));
    }

    #[test]
    fn xcode_command_carries_the_sdk_on_ios() {
        let cmd = xcode_command(
            "builds/ios",
            "my_app",
            "Debug",
            Some("iphonesimulator"),
            "arm64",
        );
        let args = args_of(&cmd);
        assert!(args.contains(&"-sdk".to_string()));
        assert!(args.contains(&"iphonesimulator".to_string()));
    }

    #[test]
    fn gradle_command_uses_the_task_for_the_mode() {
        let dir = Path::new("builds/android");
        assert_eq!(
            args_of(&gradle_command(dir, false)),
            vec!["assembleDebug".to_string()]
        );
        assert_eq!(
            args_of(&gradle_command(dir, true)),
            vec!["assembleRelease".to_string()]
        );
    }

    #[test]
    fn apk_candidates_cover_the_unsigned_release_name() {
        assert_eq!(apk_candidates(false).len(), 1);
        let release = apk_candidates(true);
        assert_eq!(release.len(), 2);
        assert!(release[1].ends_with("app-release-unsigned.apk"));
    }

    #[test]
    fn apk_path_falls_back_to_the_output_directory() {
        // Nothing is built in the test working directory, so the probe misses.
        assert_eq!(
            apk_path(false),
            "builds/android/app/build/outputs/apk/debug/"
        );
    }

    #[test]
    fn build_macos_asks_cargo_for_the_requested_profile() {
        for release in [false, true] {
            let reporter = SpyReporter::new();
            build_macos(release, &reporter).unwrap();
            let steps = reporter.steps.lock().unwrap();
            assert_eq!(steps.len(), 1);
            assert_eq!(steps[0].0.kind, StepKind::Cargo);
            assert_eq!(
                steps[0].1.contains(&"--release".to_string()),
                release,
                "release = {release}"
            );
        }
    }

    #[test]
    fn build_ios_asks_cargo_for_the_requested_profile() {
        let plan = IosPlan::resolve(true);
        for release in [false, true] {
            let reporter = SpyReporter::new();
            build_ios(&plan, release, &reporter).unwrap();
            let steps = reporter.steps.lock().unwrap();
            assert!(steps[0].1.contains(&plan.rust_target.to_string()));
            assert_eq!(steps[0].1.contains(&"--release".to_string()), release);
        }
    }

    #[test]
    fn build_android_targets_the_abi_of_the_plan() {
        let plan = AndroidPlan::for_abi("x86_64");
        let reporter = SpyReporter::new();
        build_android(&plan, true, &reporter).unwrap();
        let steps = reporter.steps.lock().unwrap();
        assert_eq!(
            steps[0].1,
            vec!["ndk", "-t", "x86_64", "build", "--lib", "--release"]
        );
    }

    #[test]
    fn execute_rejects_unknown_platform() {
        let err = execute("playstation".to_string(), false).unwrap_err();
        assert!(err.to_string().contains("unknown target"));
    }

    #[test]
    fn execute_rejects_terminated_platform() {
        let err = execute("terminated".to_string(), false).unwrap_err();
        assert!(err.to_string().contains("not an assemblable platform"));
    }
}
