use crate::commands::run::helpers::host_arch;
use crate::commands::run::utilities::get_project_root;
use crate::config::resolve_package_name;
use crate::errors::AimerError;
use crate::targets::Targets;
use anyhow::{Context, bail};
use std::env::current_dir;
use std::path::Path;
use std::process::{Command, Stdio};
use log::info;

/// Non-interactive bundling entry point used by `aimer assemble <platform>`.
///
/// Unlike `aimer build` (which only compiles the Rust library) this command
/// produces the *distributable platform bundle* — a `.app` on macOS/iOS, an
/// `.apk` on Android, or the static web `dist/` directory — in either debug or
/// release mode. It mirrors the build/package steps used by the interactive
/// `aimer run` pipeline, but runs synchronously with inherited stdio so it is
/// friendly to CI logs.
pub fn execute(platform: String, release: bool) -> anyhow::Result<()> {
    let target =
        Targets::try_from(platform.as_str()).map_err(|_| AimerError::UnknownTarget(platform.clone()))?;

    println!(
        "Assembling '{target}' bundle in {} mode...",
        profile_name(release)
    );

    let pkg_name = resolve_package_name(Path::new("."));

    let artifact = match target {
        Targets::Macos => assemble_macos(&pkg_name, release)?,
        Targets::Ios | Targets::IosSimulator => assemble_ios(&pkg_name, target, release)?,
        Targets::Android | Targets::AndroidSimulator => assemble_android(&pkg_name, release)?,
        Targets::Web => assemble_web(release)?,
        Targets::Windows | Targets::Linux => assemble_desktop(target, release)?,
        Targets::Terminated => bail!("'terminated' is not an assemblable platform"),
    };

    println!("Bundle assembled successfully: {artifact}");
    Ok(())
}

/// The Cargo profile directory name for the requested build mode.
fn profile_name(release: bool) -> &'static str {
    if release { "release" } else { "debug" }
}

/// The Xcode `-configuration` value for the requested build mode.
fn xcode_configuration(release: bool) -> &'static str {
    if release { "Release" } else { "Debug" }
}

/// The Gradle assemble task for the requested build mode.
fn gradle_task(release: bool) -> &'static str {
    if release { "assembleRelease" } else { "assembleDebug" }
}

/// Run `cmd` to completion with inherited stdio, bailing with context when it
/// fails to start or exits with a non-zero status.
fn run_step(mut cmd: Command, action: &str) -> anyhow::Result<()> {
    cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    let status = cmd
        .status()
        .with_context(|| format!("failed to start {action}"))?;
    if !status.success() {
        bail!("{action} failed");
    }
    Ok(())
}

/// Absolute path of the compiled Rust artifact for `rust_target`/`profile`.
fn artifact_path(rust_target: &str, lib_name: &str, release: bool, extension: &str) -> String {
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

/// Copy a freshly compiled native library into `dest_dir`, creating the
/// directory tree first.
fn copy_lib(src: &str, dest_dir: &str, lib_file: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(dest_dir).with_context(|| format!("creating {dest_dir}"))?;
    let dest = format!("{dest_dir}/{lib_file}");
    std::fs::copy(src, &dest).with_context(|| format!("copying static library '{src}' -> '{dest}'"))?;
    println!("Copied library to {dest}");
    Ok(())
}

/// Stage every file registered under `[assets]` in `aimer.toml` into
/// `dest_root` (incrementally) and report the result to stdout.
///
/// Delegates to [`crate::commands::assets::copy_assets_into`], which only
/// re-copies files that are new or have changed; here we just surface what
/// happened via plain `println!`/`eprintln!` suitable for CI logs.
pub(crate) fn copy_assets_into(dest_root: &str) -> anyhow::Result<()> {
    println!("Copying assets into {dest_root}");
    let report = crate::commands::assets::copy_assets_into(dest_root)?;
    for rel in &report.copied {
        info!("Copied asset {rel} -> {}", Path::new(dest_root).join(rel).display());
    }
    for rel in &report.missing {
        eprintln!("warning: registered asset '{rel}' not found; skipping");
    }
    Ok(())
}

/// Build the macOS `.app` bundle via `cargo` + `xcodebuild`.
fn assemble_macos(pkg_name: &str, release: bool) -> anyhow::Result<String> {
    let rust_target = "aarch64-apple-darwin";
    let lib_name = pkg_name.replace('-', "_");

    let mut cargo = Command::new("cargo");
    cargo.arg("build").args(["--target", rust_target, "--lib"]);
    if release {
        cargo.arg("--release");
    }
    run_step(cargo, "cargo build for macOS")?;

    let src_lib = artifact_path(rust_target, &lib_name, release, ".a");
    copy_lib(&src_lib, "builds/macos/Libraries", &format!("lib{lib_name}.a"))?;

    let configuration = xcode_configuration(release);
    let mut xcode = Command::new("xcodebuild");
    xcode
        .arg("-project")
        .arg(format!("{pkg_name}.xcodeproj"))
        .arg("-target")
        .arg(pkg_name)
        .arg("-configuration")
        .arg(configuration)
        .arg("SYMROOT=build")
        .arg("-arch")
        .arg(host_arch())
        .current_dir("builds/macos");
    run_step(xcode, "xcodebuild for macOS")?;

    let artifact = format!("builds/macos/build/{configuration}/{pkg_name}.app");
    copy_assets_into(&format!("{artifact}/Contents/Resources"))?;
    Ok(artifact)
}

/// Build the iOS `.app` bundle (device or simulator) via `cargo` + `xcodebuild`.
fn assemble_ios(pkg_name: &str, target: Targets, release: bool) -> anyhow::Result<String> {
    let arch = host_arch();
    let (rust_target, sdk, subdir_suffix) = match target {
        Targets::IosSimulator => {
            let rust_target = if arch == "x86_64" {
                "x86_64-apple-ios"
            } else {
                "aarch64-apple-ios-sim"
            };
            (rust_target, "iphonesimulator", "iphonesimulator")
        }
        _ => ("aarch64-apple-ios", "iphoneos", "iphoneos"),
    };
    let lib_name = pkg_name.replace('-', "_");

    let mut cargo = Command::new("cargo");
    cargo.arg("build").arg("--lib").arg("--target").arg(rust_target);
    if release {
        cargo.arg("--release");
    }
    run_step(cargo, "cargo build for iOS")?;

    let src_lib = artifact_path(rust_target, &lib_name, release, ".a");
    copy_lib(&src_lib, "builds/ios/Libraries", &format!("lib{lib_name}.a"))?;

    let configuration = xcode_configuration(release);
    let mut xcode = Command::new("xcodebuild");
    xcode
        .arg("-project")
        .arg(format!("{pkg_name}.xcodeproj"))
        .arg("-target")
        .arg(pkg_name)
        .arg("-configuration")
        .arg(configuration)
        .arg("-sdk")
        .arg(sdk)
        .arg("SYMROOT=build")
        .arg("-arch")
        .arg(arch)
        .current_dir("builds/ios");
    run_step(xcode, "xcodebuild for iOS")?;

    let artifact = format!("builds/ios/build/{configuration}-{subdir_suffix}/{pkg_name}.app");
    copy_assets_into(&artifact)?;
    Ok(artifact)
}

/// Build the Android `.apk` via `cargo ndk` + Gradle.
fn assemble_android(pkg_name: &str, release: bool) -> anyhow::Result<String> {
    let rust_target = "aarch64-linux-android";
    let jni_dir = "arm64-v8a";
    let lib_name = pkg_name.replace('-', "_");

    let mut cargo = Command::new("cargo");
    cargo
        .arg("ndk")
        .arg("-t")
        .arg(jni_dir)
        .arg("build")
        .arg("--lib");
    if release {
        cargo.arg("--release");
    }
    run_step(cargo, "cargo ndk build for Android")?;

    let src_lib = artifact_path(rust_target, &lib_name, release, ".so");
    copy_lib(
        &src_lib,
        &format!("builds/android/app/src/main/jniLibs/{jni_dir}"),
        &format!("lib{lib_name}.so"),
    )?;

    // Stage assets into the APK's `assets/` source set before Gradle packs it.
    copy_assets_into("builds/android/app/src/main/assets")?;

    let android_dir = current_dir()
        .context("resolving current directory")?
        .join("builds/android");
    let gradlew = if cfg!(windows) { "gradlew.bat" } else { "gradlew" };

    let mut gradle = Command::new(android_dir.join(gradlew));
    gradle.arg(gradle_task(release)).current_dir(&android_dir);
    if let Some(java_home) = resolve_compatible_java_home() {
        println!("Using JAVA_HOME: {java_home}");
        gradle.env("JAVA_HOME", java_home);
    }
    run_step(gradle, "Gradle assemble for Android")?;

    // Locate the produced APK. Unsigned release builds (no signing config) are
    // emitted with an `-unsigned` suffix, so probe the known candidates.
    let dir = if release { "release" } else { "debug" };
    let candidates = if release {
        vec![
            "builds/android/app/build/outputs/apk/release/app-release.apk".to_string(),
            "builds/android/app/build/outputs/apk/release/app-release-unsigned.apk".to_string(),
        ]
    } else {
        vec!["builds/android/app/build/outputs/apk/debug/app-debug.apk".to_string()]
    };

    Ok(candidates
        .into_iter()
        .find(|p| Path::new(p).exists())
        .unwrap_or_else(|| format!("builds/android/app/build/outputs/apk/{dir}/")))
}

/// Build the static web bundle via `trunk`.
fn assemble_web(release: bool) -> anyhow::Result<String> {
    // Tell trunk about registered assets so it copies them into dist/ as part
    // of its build. Trunk cleans dist/ first, so manual pre-staging would be
    // wiped — the `[[copy]]` directive is the native mechanism.

    // copy_assets_into(&format!("/assets"))?;

    let artifact = "builds/web";
    // if !Path::new(artifact).exists() {
    //     fs::crea
    // }
    copy_assets_into(artifact)?;

    let mut trunk = Command::new("trunk");
    trunk.arg("build").current_dir("builds/web");
    if release {
        trunk.arg("--release");
    }
    run_step(trunk, "trunk build for web")?;

    Ok("builds/web/dist".to_string())
}

/// Build the desktop (Windows/Linux) library. No platform installer template
/// exists yet, so this compiles the artifact and reports its directory.
fn assemble_desktop(target: Targets, release: bool) -> anyhow::Result<String> {
    let mut cargo = Command::new("cargo");
    cargo.arg("build").arg("--lib");
    if release {
        cargo.arg("--release");
    }
    run_step(cargo, &format!("cargo build for {target}"))?;

    let artifact = format!("target/{}", profile_name(release));
    copy_assets_into(&format!("{artifact}/assets"))?;
    Ok(artifact)
}

/// Locate a Gradle-compatible `JAVA_HOME` on macOS, preferring LTS releases.
fn resolve_compatible_java_home() -> Option<String> {
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
    use super::*;

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
