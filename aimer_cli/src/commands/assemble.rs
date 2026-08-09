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
        Targets::Windows | Targets::Linux => {
            build_desktop(target, &pkg_name, release, &reporter)?;
            package_desktop(target, &pkg_name, release, &reporter)?
        }
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

/// Fail unless `target` is the desktop platform of the host.
///
/// The desktop leg is a host build: cargo compiles for the default triple and
/// the bundle is laid out for the operating system it was produced on. Asking
/// for the other desktop platform can therefore only ever yield an artifact
/// that cannot run, so it is rejected up front instead.
///
/// # Errors
///
/// Returns an error naming both the requested target and the host when they do
/// not match, or when the host is not a desktop platform at all.
pub(crate) fn ensure_host_desktop(target: Targets) -> anyhow::Result<()> {
    match Targets::host_desktop() {
        Some(host) if host == target => Ok(()),
        _ => bail!(
            "cross-compiling to {target} from {} is not supported; \
             build desktop targets on a matching host",
            std::env::consts::OS
        ),
    }
}

/// The portable desktop bundle of `pkg_name`, relative to the project root.
///
/// The bundle is a plain folder — executable, staged assets and the platform
/// scaffold files side by side — so it can be zipped and handed over as is.
///
/// # Examples
///
/// ```ignore
/// assert_eq!(
///     desktop_bundle_path(Targets::Linux, "my_app", false),
///     "builds/linux/bundle/debug/my_app"
/// );
/// ```
pub(crate) fn desktop_bundle_path(target: Targets, pkg_name: &str, release: bool) -> String {
    format!(
        "builds/{target}/bundle/{}/{pkg_name}",
        profile_name(release)
    )
}

/// The executable cargo produces for a host build of `pkg_name`.
///
/// Cargo names the default binary after the package, dashes included, so the
/// raw package name is used here rather than [`lib_name_of`].
pub(crate) fn desktop_exe_path(pkg_name: &str, release: bool) -> String {
    let project_path = get_project_root(true).unwrap_or(PathBuf::new()).display().to_string();
    format!(
        "{project_path}/target/{}/{pkg_name}{}",
        profile_name(release),
        std::env::consts::EXE_SUFFIX
    )

}

/// The file name the executable carries inside the bundle.
#[inline]
pub(crate) fn desktop_exe_name(pkg_name: &str) -> String {
    format!("{pkg_name}{}", std::env::consts::EXE_SUFFIX)
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
        Step::new(StepKind::Xcode, "xcodebuild for iOS"),
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

/// Compile the application binary for the host desktop.
///
/// Unlike every other platform the desktop leg builds a `--bin` rather than a
/// `--lib`: there is no platform shell to link the library into, the compiled
/// executable *is* the app.
pub(crate) fn build_desktop(
    target: Targets,
    pkg_name: &str,
    release: bool,
    reporter: &dyn Reporter,
) -> anyhow::Result<()> {
    ensure_host_desktop(target)?;
    let mut cargo = Command::new("cargo");
    cargo.arg("build").args(["--bin", pkg_name]);
    if release {
        cargo.arg("--release");
    }
    reporter.run(
        cargo,
        Step::new(StepKind::Cargo, format!("cargo build for {target}")),
    )
}

/// Stage the executable, the registered assets and the platform scaffold files
/// into the portable bundle, returning its path.
///
/// Shared by `aimer assemble windows|linux` and the desktop leg of the
/// `aimer run` pipeline. The bundle is rebuilt from scratch every time, so a
/// stale executable can never be launched after a failed build.
pub(crate) fn package_desktop(
    target: Targets,
    pkg_name: &str,
    release: bool,
    reporter: &dyn Reporter,
) -> anyhow::Result<String> {
    ensure_host_desktop(target)?;
    package_desktop_in(Path::new("."), target, pkg_name, release, reporter)
}

/// [`package_desktop`], rooted at `root` instead of the current directory and
/// without the host check.
///
/// The returned bundle path stays relative to `root`, which is what the console
/// and the assemble summary print. Only the layout is decided here, so the
/// staging can be exercised for either desktop platform from any host.
pub(crate) fn package_desktop_in(
    root: &Path,
    target: Targets,
    pkg_name: &str,
    release: bool,
    reporter: &dyn Reporter,
) -> anyhow::Result<String> {
    let bundle = desktop_bundle_path(target, pkg_name, release);
    let bundle_dir = root.join(&bundle);
    clean_bundle(&bundle_dir.to_string_lossy())?;
    std::fs::create_dir_all(&bundle_dir)
        .with_context(|| format!("creating {}", bundle_dir.display()))?;

    let src = root.join(desktop_exe_path(pkg_name, release));
    let dest = bundle_dir.join(desktop_exe_name(pkg_name));
    // `fs::copy` carries the permission bits over, so the copy stays executable.
    std::fs::copy(&src, &dest).with_context(|| {
        format!(
            "copying executable '{}' -> '{}'",
            src.display(),
            dest.display()
        )
    })?;
    reporter.note(format!("Copied executable to {}", dest.display()));

    copy_desktop_extras(root, target, pkg_name, &bundle_dir, reporter)?;

    // The runtime looks assets up next to the executable, so the bundle root is
    // exactly where they have to land.
    reporter.stage_assets(&bundle)?;
    Ok(bundle)
}

/// Copy the platform scaffold files of `target` into the bundle.
///
/// Linux gets its icon and a desktop entry whose `Exec`/`Icon` lines name the
/// bundled binary; Windows gets its icon and the manifest under the
/// `<exe>.manifest` name the loader picks up as an external manifest. A scaffold
/// file that was never generated is reported and skipped rather than failing the
/// bundle — the executable alone is still runnable.
fn copy_desktop_extras(
    root: &Path,
    target: Targets,
    pkg_name: &str,
    bundle_dir: &Path,
    reporter: &dyn Reporter,
) -> anyhow::Result<()> {
    let scaffold = root.join(format!("builds/{target}"));
    match target {
        Targets::Linux => {
            copy_extra(&scaffold.join("app.png"), &bundle_dir.join("app.png"), reporter)?;
            let entry = [
                scaffold.join(format!("{pkg_name}.desktop")),
                scaffold.join("app.desktop"),
            ]
            .into_iter()
            .find(|p| p.exists());
            if let Some(entry) = entry {
                let contents = std::fs::read_to_string(&entry)
                    .with_context(|| format!("reading {}", entry.display()))?;
                let dest = bundle_dir.join(format!("{pkg_name}.desktop"));
                std::fs::write(&dest, retarget_desktop_entry(&contents, pkg_name))
                    .with_context(|| format!("writing {}", dest.display()))?;
                reporter.note(format!("Copied desktop entry to {}", dest.display()));
            } else {
                reporter.note(format!(
                    "warning: no desktop entry in {}; skipping",
                    scaffold.display()
                ));
            }
        }
        Targets::Windows => {
            copy_extra(&scaffold.join("app.ico"), &bundle_dir.join("app.ico"), reporter)?;
            copy_extra(
                &scaffold.join("app.manifest"),
                &bundle_dir.join(format!("{}.manifest", desktop_exe_name(pkg_name))),
                reporter,
            )?;
        }
        _ => {}
    }
    Ok(())
}

/// Copy one optional scaffold file, reporting a missing source instead of
/// failing.
fn copy_extra(src: &Path, dest: &Path, reporter: &dyn Reporter) -> anyhow::Result<()> {
    if !src.exists() {
        reporter.note(format!(
            "warning: scaffold file '{}' not found; skipping",
            src.display()
        ));
        return Ok(());
    }
    std::fs::copy(src, dest)
        .with_context(|| format!("copying '{}' -> '{}'", src.display(), dest.display()))?;
    reporter.note(format!("Copied {}", dest.display()));
    Ok(())
}

/// Point the `Exec` and `Icon` lines of a desktop entry at the bundled binary.
///
/// The scaffolded entry describes the project as it sits in the repository; in
/// the bundle the executable lives next to the entry, so both keys become the
/// package name.
fn retarget_desktop_entry(contents: &str, pkg_name: &str) -> String {
    let mut out = String::with_capacity(contents.len());
    for line in contents.lines() {
        if line.starts_with("Exec=") {
            out.push_str(&format!("Exec={pkg_name}"));
        } else if line.starts_with("Icon=") {
            out.push_str("Icon=app");
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// JDK feature releases the Android scaffold builds with, most preferred first.
///
/// Gradle 9 and the Android Gradle plugin both require JDK 17 as a minimum, so
/// older runtimes (8, 11) are deliberately never selected — picking one would
/// only trade a clear "no JDK found" into an opaque Gradle failure.
const SUPPORTED_JDK_VERSIONS: &[&str] = &["17", "21", "23"];

/// Locate a Gradle-compatible `JAVA_HOME` on macOS, preferring LTS releases.
pub(crate) fn resolve_compatible_java_home() -> Option<String> {
    if cfg!(target_os = "macos") {
        for version in SUPPORTED_JDK_VERSIONS {
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
    fn only_jdk_17_and_newer_are_considered() {
        assert!(!SUPPORTED_JDK_VERSIONS.is_empty());
        for version in SUPPORTED_JDK_VERSIONS {
            let feature: u32 = version.parse().expect("JDK feature release");
            assert!(feature >= 17, "Gradle 9 rejects JDK {version}");
        }
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

    // ── Desktop bundles ──────────────────────────────────────────────

    #[test]
    fn desktop_bundle_path_follows_the_platform_and_profile() {
        assert_eq!(
            desktop_bundle_path(Targets::Linux, "my_app", false),
            "builds/linux/bundle/debug/my_app"
        );
        assert_eq!(
            desktop_bundle_path(Targets::Windows, "my_app", true),
            "builds/windows/bundle/release/my_app"
        );
    }

    #[test]
    fn desktop_exe_path_keeps_the_package_name_verbatim() {
        // Cargo names the default binary after the package, dashes included.
        let proj_root = get_project_root(true).unwrap_or_default();
        assert_eq!(
            desktop_exe_path("my-cool-app", false),
            proj_root.join(format!(
                "target/debug/my-cool-app{}",
                std::env::consts::EXE_SUFFIX
            ))
        );
        assert_eq!(
            desktop_exe_path("my-cool-app", true),
            proj_root.join(format!(
                "target/release/my-cool-app{}",
                std::env::consts::EXE_SUFFIX
            ))
        );
    }

    /// A project tree whose compiled binary sits where [`desktop_exe_path`]
    /// resolves it.
    ///
    /// [`desktop_exe_path`] anchors the executable at the workspace root (via
    /// `cargo locate-project`), so the fake binary is staged into the real
    /// `target/` rather than under the per-test temp dir, which only holds the
    /// bundle layout [`package_desktop_in`] lays out.
    fn desktop_project(pkg_name: &str) -> tempfile::TempDir {
        // Several tests stage the same binary name, so write it under a lock —
        // a parallel test must never read a half-written file.
        static STAGE_EXECUTABLE: Mutex<()> = Mutex::new(());
        let _guard = STAGE_EXECUTABLE.lock().unwrap();

        let exe = PathBuf::from(desktop_exe_path(pkg_name, false));
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"#!/bin/sh\ntrue\n").unwrap();
        tempfile::tempdir().expect("temp project")
    }

    /// Write the Linux scaffold `create::linux` generates for `pkg_name`.
    fn linux_scaffold(root: &Path, pkg_name: &str) {
        let dir = root.join("builds/linux");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{pkg_name}.desktop")),
            "[Desktop Entry]\nName=Demo\nExec=aimer_app\nIcon=aimer\n",
        )
        .unwrap();
        std::fs::write(dir.join("app.png"), b"png").unwrap();
    }

    #[test]
    fn packaging_stages_the_executable_and_the_linux_scaffold() {
        let pkg = "my_app";
        let project = desktop_project(pkg);
        linux_scaffold(project.path(), pkg);
        let reporter = SpyReporter::new();

        let bundle =
            package_desktop_in(project.path(), Targets::Linux, pkg, false, &reporter).unwrap();

        assert_eq!(bundle, "builds/linux/bundle/debug/my_app");
        let dir = project.path().join(&bundle);
        assert!(dir.join(desktop_exe_name(pkg)).is_file());
        assert!(dir.join("app.png").is_file());
        let entry = std::fs::read_to_string(dir.join("my_app.desktop")).unwrap();
        assert!(entry.contains("Exec=my_app"), "{entry}");
        assert!(!entry.contains("aimer_app"), "{entry}");
        assert!(entry.contains("Name=Demo"), "{entry}");
        // The assets land next to the executable, where the runtime looks.
        assert!(
            reporter
                .notes
                .lock()
                .unwrap()
                .contains(&format!("staged {bundle}"))
        );
    }

    #[test]
    fn packaging_stages_the_windows_scaffold_next_to_the_executable() {
        let pkg = "my_app";
        let project = desktop_project(pkg);
        let dir = project.path().join("builds/windows");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("app.ico"), b"ico").unwrap();
        std::fs::write(dir.join("app.manifest"), b"<assembly/>").unwrap();
        let reporter = SpyReporter::new();

        let bundle =
            package_desktop_in(project.path(), Targets::Windows, pkg, false, &reporter).unwrap();

        let bundle_dir = project.path().join(&bundle);
        assert!(bundle_dir.join("app.ico").is_file());
        // Windows reads an external manifest named after the executable.
        assert!(
            bundle_dir
                .join(format!("{}.manifest", desktop_exe_name(pkg)))
                .is_file()
        );
    }

    #[test]
    fn packaging_keeps_a_dashed_package_name() {
        let pkg = "my-cool-app";
        let project = desktop_project(pkg);
        let reporter = SpyReporter::new();

        let bundle =
            package_desktop_in(project.path(), Targets::Linux, pkg, false, &reporter).unwrap();

        assert!(
            project
                .path()
                .join(&bundle)
                .join(desktop_exe_name(pkg))
                .is_file()
        );
    }

    #[test]
    fn packaging_removes_a_stale_bundle_first() {
        let pkg = "my_app";
        let project = desktop_project(pkg);
        let bundle = project
            .path()
            .join(desktop_bundle_path(Targets::Linux, pkg, false));
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(bundle.join("stale.txt"), b"old").unwrap();
        let reporter = SpyReporter::new();

        package_desktop_in(project.path(), Targets::Linux, pkg, false, &reporter).unwrap();

        assert!(!bundle.join("stale.txt").exists());
        assert!(bundle.join(desktop_exe_name(pkg)).is_file());
    }

    #[test]
    fn packaging_without_a_compiled_binary_names_the_missing_executable() {
        let project = tempfile::tempdir().unwrap();
        let reporter = SpyReporter::new();

        // This name is never staged by `desktop_project`, so the resolved
        // executable is guaranteed absent.
        let err = package_desktop_in(
            project.path(),
            Targets::Linux,
            "my_app_missing",
            false,
            &reporter,
        )
        .expect_err("no compiled binary");

        assert!(err.to_string().contains("copying executable"), "{err}");
    }

    #[test]
    fn packaging_survives_a_project_without_scaffold_files() {
        let pkg = "my_app";
        let project = desktop_project(pkg);
        let reporter = SpyReporter::new();

        let bundle =
            package_desktop_in(project.path(), Targets::Linux, pkg, false, &reporter).unwrap();

        assert!(
            project
                .path()
                .join(&bundle)
                .join(desktop_exe_name(pkg))
                .is_file()
        );
        assert!(
            reporter
                .notes
                .lock()
                .unwrap()
                .iter()
                .any(|n| n.contains("warning:"))
        );
    }

    #[test]
    fn a_desktop_build_for_another_os_is_refused() {
        let reporter = SpyReporter::new();
        for target in [Targets::Windows, Targets::Linux] {
            if Targets::host_desktop() == Some(target) {
                continue;
            }
            let err = build_desktop(target, "my_app", false, &reporter)
                .expect_err("cross-OS desktop build");
            let message = err.to_string();
            assert!(message.contains(&target.to_string()), "{message}");
            assert!(message.contains(std::env::consts::OS), "{message}");
            assert!(package_desktop(target, "my_app", false, &reporter).is_err());
        }
        // Nothing was run and nothing was written for a refused target.
        assert!(reporter.steps.lock().unwrap().is_empty());
    }

    #[test]
    fn the_desktop_build_asks_cargo_for_the_application_binary() {
        let Some(host) = Targets::host_desktop() else {
            return;
        };
        let reporter = SpyReporter::new();
        build_desktop(host, "my-cool-app", true, &reporter).unwrap();

        let steps = reporter.steps.lock().unwrap();
        let (step, args) = steps.first().expect("a cargo step");
        assert_eq!(step.kind, StepKind::Cargo);
        assert_eq!(
            args,
            &vec![
                "build".to_string(),
                "--bin".to_string(),
                "my-cool-app".to_string(),
                "--release".to_string(),
            ]
        );
    }

    #[test]
    fn retargeting_a_desktop_entry_points_at_the_bundled_binary() {
        let entry = retarget_desktop_entry(
            "[Desktop Entry]\nName=Demo\nExec=aimer_app\nIcon=aimer\nType=Application\n",
            "my_app",
        );

        assert_eq!(
            entry,
            "[Desktop Entry]\nName=Demo\nExec=my_app\nIcon=app\nType=Application\n"
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
