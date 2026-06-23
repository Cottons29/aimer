use crate::config::AimerManifest;
use anyhow::Context;
use std::path::Path;

/// Outcome of staging the registered `[assets]` into a platform directory.
///
/// The lists hold the *relative* asset paths (the verbatim keys from
/// `aimer.toml`) so callers can log them however suits their output channel
/// (plain `println!` for `assemble`, the TUI console for `run`).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct AssetCopyReport {
    /// Files that were (re)copied because they were new or had changed.
    pub copied: Vec<String>,
    /// Files left untouched because the destination was already up to date.
    pub skipped: Vec<String>,
    /// Registered files that do not exist on disk.
    pub missing: Vec<String>,
}

/// Whether `src` needs to be copied over `dest`.
///
/// A copy is required when the destination is missing, when the file sizes
/// differ, or when the source is newer than the destination. This keeps `run`
/// fast by re-copying only assets that are new or have actually changed.
fn needs_copy(src: &Path, dest: &Path) -> bool {
    let (Ok(src_meta), Ok(dest_meta)) = (src.metadata(), dest.metadata()) else {
        // Destination missing (or unreadable) -> must copy.
        return true;
    };
    if src_meta.len() != dest_meta.len() {
        return true;
    }
    match (src_meta.modified(), dest_meta.modified()) {
        (Ok(src_modified), Ok(dest_modified)) => src_modified > dest_modified,
        // If timestamps are unavailable, err on the side of copying.
        _ => true,
    }
}

/// Copy every file registered under `[assets]` in `aimer.toml` into
/// `dest_root`, preserving each file's relative path.
///
/// The relative path is kept verbatim so the string used in `aimer.toml`
/// doubles as the runtime lookup key for `ImageSource::Asset`. Copying is
/// *incremental*: a file is only written when its destination is missing or
/// out of date (see [`needs_copy`]). Missing source files are recorded in the
/// returned [`AssetCopyReport`] rather than aborting the whole bundle.
pub fn copy_assets_into(dest_root: &str) -> anyhow::Result<AssetCopyReport> {
    let files: Vec<String> = AimerManifest::load_from(Path::new("."))
        .ok()
        .flatten()
        .map(|m| m.asset_files().to_vec())
        .unwrap_or_default();

    copy_files(&files, Path::new("."), Path::new(dest_root))
}

/// Incrementally copy `files` (relative paths) from `src_root` into
/// `dest_root`, preserving their relative layout.
///
/// Split out from [`copy_assets_into`] so the copy logic can be unit-tested
/// against explicit roots without mutating the process-wide current directory.
fn copy_files(files: &[String], src_root: &Path, dest_root: &Path) -> anyhow::Result<AssetCopyReport> {
    let mut report = AssetCopyReport::default();
    if files.is_empty() {
        return Ok(report);
    }

    for rel in files {
        let src = src_root.join(rel);
        if !src.exists() {
            report.missing.push(rel.clone());
            continue;
        }
        let dest = dest_root.join(rel);
        if !needs_copy(&src, &dest) {
            report.skipped.push(rel.clone());
            continue;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::copy(&src, &dest)
            .with_context(|| format!("copying asset '{}' -> '{}'", src.display(), dest.display()))?;
        report.copied.push(rel.clone());
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_new_assets_and_records_missing() {
        let src_root = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();

        std::fs::create_dir_all(src_root.path().join("assets/sub")).unwrap();
        std::fs::write(src_root.path().join("assets/sub/logo.png"), b"PNG").unwrap();

        let files = vec!["assets/sub/logo.png".to_string(), "assets/missing.png".to_string()];
        let report = copy_files(&files, src_root.path(), dest.path()).unwrap();

        assert!(dest.path().join("assets/sub/logo.png").exists());
        assert_eq!(report.copied, ["assets/sub/logo.png".to_string()]);
        assert_eq!(report.missing, ["assets/missing.png".to_string()]);
        assert!(report.skipped.is_empty());
    }

    #[test]
    fn skips_unchanged_assets_on_second_run() {
        let src_root = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();

        std::fs::create_dir_all(src_root.path().join("assets")).unwrap();
        std::fs::write(src_root.path().join("assets/logo.png"), b"PNG").unwrap();
        let files = vec!["assets/logo.png".to_string()];

        // First copy stages the file...
        let first = copy_files(&files, src_root.path(), dest.path()).unwrap();
        assert_eq!(first.copied, ["assets/logo.png".to_string()]);

        // ...a second copy with no changes should skip it.
        let second = copy_files(&files, src_root.path(), dest.path()).unwrap();
        assert!(second.copied.is_empty());
        assert_eq!(second.skipped, ["assets/logo.png".to_string()]);
    }

    #[test]
    fn recopies_changed_assets() {
        let src_root = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();

        std::fs::create_dir_all(src_root.path().join("assets")).unwrap();
        let src = src_root.path().join("assets/logo.png");
        std::fs::write(&src, b"PNG").unwrap();
        let files = vec!["assets/logo.png".to_string()];

        copy_files(&files, src_root.path(), dest.path()).unwrap();

        // Change the size so `needs_copy` detects it regardless of timestamp
        // granularity.
        std::fs::write(&src, b"PNG-CHANGED").unwrap();
        let report = copy_files(&files, src_root.path(), dest.path()).unwrap();
        assert_eq!(report.copied, ["assets/logo.png".to_string()]);
        assert_eq!(
            std::fs::read(dest.path().join("assets/logo.png")).unwrap(),
            b"PNG-CHANGED"
        );
    }
}
