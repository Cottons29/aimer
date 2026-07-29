//! The system libraries an Apple app has to hand the linker so that the Rust
//! static library resolves.
//!
//! `rustc` knows the answer exactly: every `#[link(name = "...", kind =
//! "framework")]` in the crate graph — `objc2-ui-kit`, `objc2-core-haptics`,
//! `wgpu` and friends — ends up in its `native-static-libs` list. That
//! knowledge does **not** survive into the archive, though: `libapp.a` carries
//! no `LC_LINKER_OPTION`, so the Xcode target linking it has to be told the
//! same list separately, or the build dies with
//! `Undefined symbols for architecture arm64` long after the Rust side
//! compiled cleanly.
//!
//! Keeping that list hardcoded in the generated `project.pbxproj` means every
//! new Apple binding added anywhere in the dependency tree silently breaks the
//! app link. Instead the compiler is asked on every build
//! (`--print native-static-libs=<path>`, see [`RAW_FILE`]) and its answer is
//! rendered into [`XCCONFIG_FILE`], an `.xcconfig` the Xcode project uses as
//! the base configuration of its app target. `OTHER_LDFLAGS` there only
//! interpolates [`LDFLAGS_SETTING`], so the link line always matches what the
//! Rust crate graph actually needs — including from a plain Xcode GUI build,
//! which reads the last generated file.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::commands::assemble::artifact_path;

/// Name of the file `rustc --print native-static-libs=<path>` drops the raw,
/// unfiltered flag list into.
///
/// It lives beside the static archive it describes, under
/// `target/<triple>/<profile>/`: both are outputs of the same cargo
/// invocation, both are replaced on every build, and neither belongs in the
/// checked-in platform project.
pub(crate) const RAW_FILE: &str = "native-static-libs.txt";

/// The generated `.xcconfig`, relative to the platform project directory.
///
/// Unlike [`RAW_FILE`] this one is meant to be committed: it is what makes a
/// checkout link correctly in Xcode before `aimer` has ever been run on it.
pub(crate) const XCCONFIG_FILE: &str = "RustLinkFlags.xcconfig";

/// The build setting [`XCCONFIG_FILE`] defines and `OTHER_LDFLAGS`
/// interpolates.
pub(crate) const LDFLAGS_SETTING: &str = "AIMER_RUST_LDFLAGS";

/// The list a freshly scaffolded iOS project starts with, used until the first
/// build replaces it with the crate graph's actual answer.
///
/// It is deliberately the union of what an empty Aimer app needs: enough to
/// link the renderer, the windowing layer and the native bindings the umbrella
/// `aimer` crate re-exports.
pub(crate) const DEFAULT_IOS_FLAGS: &[&str] = &[
    "-framework UIKit",
    "-framework Metal",
    "-framework MetalKit",
    "-framework CoreVideo",
    "-framework CoreGraphics",
    "-framework CoreText",
    "-framework CoreFoundation",
    "-framework CoreHaptics",
    "-framework Foundation",
    "-framework QuartzCore",
    "-framework Security",
    "-lobjc",
    "-lc++",
];

/// The `--print` argument that makes `rustc` write the flag list of a
/// `rust_target` build down.
///
/// `rustc` resolves the path against its own working directory, which is the
/// cargo workspace root rather than the platform project, so [`raw_path`]
/// hands it an absolute one.
pub(crate) fn print_arg(rust_target: &str, release: bool) -> anyhow::Result<String> {
    let raw = raw_path(rust_target, release);
    let parent = raw
        .parent()
        .expect("the raw path always sits in a profile directory");
    fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    Ok(format!("native-static-libs={}", raw.display()))
}

/// Path of the raw flag list a `rust_target` build writes.
pub(crate) fn raw_path(rust_target: &str, release: bool) -> PathBuf {
    // `artifact_path` names the static archive of this very build; the flag
    // list describing it is its neighbour.
    let archive = artifact_path(rust_target, "placeholder", release, ".a");
    Path::new(&archive)
        .parent()
        .expect("the archive always sits in a profile directory")
        .join(RAW_FILE)
}

/// Path of the generated xcconfig of the project in `project_dir`.
pub(crate) fn xcconfig_path(project_dir: &Path) -> PathBuf {
    project_dir.join(XCCONFIG_FILE)
}

/// Regenerate [`XCCONFIG_FILE`] from the flag list the last build wrote.
///
/// A missing or empty raw file is not an error: the project keeps whatever
/// xcconfig it already has, which is either the scaffolded default or the
/// result of the previous build. Overwriting it with nothing would turn a
/// stale link line into a broken one.
pub(crate) fn refresh(project_dir: &Path, raw: &Path) -> anyhow::Result<()> {
    let Ok(contents) = fs::read_to_string(raw) else {
        return Ok(());
    };
    let flags = dedup(&contents);
    if flags.is_empty() {
        return Ok(());
    }
    write(project_dir, &flags)
}

/// Write the default flag list, for a project that has never been built.
pub(crate) fn scaffold(project_dir: &Path) -> anyhow::Result<()> {
    let flags: Vec<String> = DEFAULT_IOS_FLAGS.iter().map(|f| f.to_string()).collect();
    write(project_dir, &flags)
}

/// Render `flags` into [`XCCONFIG_FILE`] under `project_dir`.
fn write(project_dir: &Path, flags: &[String]) -> anyhow::Result<()> {
    let path = xcconfig_path(project_dir);
    fs::write(&path, render(flags)).with_context(|| format!("writing {}", path.display()))
}

/// Collapse the raw `native-static-libs` list into the flags to pass on, in
/// first-seen order.
///
/// `rustc` reports the list as the linker wants to *see* it — one entry per
/// crate that asked for it, so `Foundation` and `UIKit` show up many times over
/// — and splits every framework across two tokens. Duplicates are harmless to
/// `ld` but make the setting unreadable, so a `-framework Foo` pair is rejoined
/// into a single entry and each entry is kept only once.
fn dedup(raw: &str) -> Vec<String> {
    let mut flags: Vec<String> = Vec::new();
    let mut tokens = raw.split_whitespace();
    while let Some(token) = tokens.next() {
        let flag = if token == "-framework" {
            match tokens.next() {
                Some(name) => format!("-framework {name}"),
                // A trailing `-framework` with no name is malformed output;
                // dropping it is better than emitting a dangling flag.
                None => break,
            }
        } else {
            token.to_string()
        };
        if !flags.contains(&flag) {
            flags.push(flag);
        }
    }
    flags
}

/// The contents of [`XCCONFIG_FILE`] for `flags`.
fn render(flags: &[String]) -> String {
    format!(
        "// Generated by `aimer`. Do not edit: this file is rewritten on every\n\
         // iOS build from `rustc --print native-static-libs`, i.e. from the\n\
         // frameworks the Rust crate graph actually links against.\n\
         //\n\
         // The app target uses this file as its base configuration and only\n\
         // interpolates `$({setting})` into `OTHER_LDFLAGS`, so adding an\n\
         // Apple binding to the Rust side never needs an Xcode change.\n\
         {setting} = {flags}\n",
        setting = LDFLAGS_SETTING,
        flags = flags.join(" "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedup_rejoins_framework_pairs() {
        assert_eq!(
            dedup("-framework CoreHaptics -lobjc -framework UIKit"),
            vec![
                "-framework CoreHaptics".to_string(),
                "-lobjc".to_string(),
                "-framework UIKit".to_string(),
            ]
        );
    }

    #[test]
    fn dedup_keeps_the_first_occurrence_only() {
        let flags = dedup("-framework Foundation -lSystem -framework Foundation -lSystem");

        assert_eq!(
            flags,
            vec!["-framework Foundation".to_string(), "-lSystem".to_string()]
        );
    }

    #[test]
    fn dedup_survives_a_dangling_framework_token() {
        assert_eq!(dedup("-lobjc -framework"), vec!["-lobjc".to_string()]);
    }

    #[test]
    fn dedup_of_nothing_is_nothing() {
        assert!(dedup("").is_empty());
        assert!(dedup("   \n ").is_empty());
    }

    #[test]
    fn render_defines_the_setting_on_one_line() {
        let rendered = render(&["-framework UIKit".to_string(), "-lobjc".to_string()]);

        assert!(rendered.contains("AIMER_RUST_LDFLAGS = -framework UIKit -lobjc\n"));
    }

    #[test]
    fn the_default_list_covers_core_haptics() {
        // `aimer` re-exports `aimer_native`, whose haptics live in
        // CoreHaptics: a scaffolded project must link it before it is ever
        // built by the CLI.
        assert!(DEFAULT_IOS_FLAGS.contains(&"-framework CoreHaptics"));
    }

    #[test]
    fn refresh_rewrites_the_xcconfig_from_the_raw_list() {
        let dir = tempdir();
        let raw = dir.join(RAW_FILE);
        fs::write(&raw, "-framework CoreHaptics -framework UIKit\n").unwrap();

        refresh(&dir, &raw).unwrap();

        let rendered = fs::read_to_string(xcconfig_path(&dir)).unwrap();
        assert!(rendered.contains("AIMER_RUST_LDFLAGS = -framework CoreHaptics -framework UIKit"));
    }

    #[test]
    fn refresh_keeps_the_previous_xcconfig_when_nothing_was_reported() {
        let dir = tempdir();
        let raw = dir.join(RAW_FILE);
        scaffold(&dir).unwrap();
        let scaffolded = fs::read_to_string(xcconfig_path(&dir)).unwrap();

        // No raw file at all, then an empty one: neither may blank the link
        // line of a project that used to build.
        refresh(&dir, &raw).unwrap();
        assert_eq!(fs::read_to_string(xcconfig_path(&dir)).unwrap(), scaffolded);

        fs::write(&raw, "\n").unwrap();
        refresh(&dir, &raw).unwrap();
        assert_eq!(fs::read_to_string(xcconfig_path(&dir)).unwrap(), scaffolded);
    }

    #[test]
    fn the_raw_list_sits_next_to_the_static_archive() {
        let debug = raw_path("aarch64-apple-ios", false);
        let release = raw_path("aarch64-apple-ios", true);

        assert!(debug.is_absolute(), "{debug:?}");
        assert!(debug.ends_with("target/aarch64-apple-ios/debug/native-static-libs.txt"));
        assert!(release.ends_with("target/aarch64-apple-ios/release/native-static-libs.txt"));
    }

    #[test]
    fn the_print_argument_names_the_raw_list() {
        let arg = print_arg("aarch64-apple-ios", false).unwrap();

        let path = arg.strip_prefix("native-static-libs=").unwrap();
        assert_eq!(Path::new(path), raw_path("aarch64-apple-ios", false));
        assert!(Path::new(path).is_absolute(), "{arg}");
    }

    /// A unique, empty scratch directory, removed when the test binary exits.
    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "aimer-link-flags-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
