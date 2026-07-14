use crate::config::AimerManifest;
use anyhow::Context;
use std::path::Path;

/// Marker comment that delimits the auto-generated `[[copy]]` section in
/// `Trunk.toml`. Everything from this line to the end of the file is replaced
/// on each sync so that stale entries are cleaned up.
const TRUNK_COPY_MARKER: &str = "# --- aimer assets (auto-generated) ---";

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

/// Update `builds/web/Trunk.toml` with `[[copy]]` entries for every file
/// registered under `[assets]` in `aimer.toml`.
///
/// Trunk cleans its output directory (`dist/`) before each build, so any
/// assets staged there manually would be wiped. The `[[copy]]` directive is
/// Trunk's native mechanism for including extra files — it copies them into
/// `dist/` as part of the build, which works for both `trunk build` and
/// `trunk serve`.
///
/// Paths are written relative to the Trunk.toml location (`builds/web/`), so
/// an asset registered as `assets/logo.png` becomes `../../assets/logo.png`.
///
/// The generated section is delimited by [`TRUNK_COPY_MARKER`] so it can be
/// safely replaced on subsequent runs without touching user customisations in
/// the rest of the file.
pub fn sync_trunk_copy_entries() -> anyhow::Result<()> {
    sync_trunk_copy_entries_in(Path::new("."))
}

/// Internal implementation that accepts an explicit directory for testability.
fn sync_trunk_copy_entries_in(dir: &Path) -> anyhow::Result<()> {
    let trunk_toml = dir.join("builds/web/Trunk.toml");
    if !trunk_toml.exists() {
        return Ok(());
    }

    let assets: Vec<String> = AimerManifest::load_from(dir)
        .ok()
        .flatten()
        .map(|m| m.asset_files().to_vec())
        .unwrap_or_default();

    let existing = std::fs::read_to_string(&trunk_toml)
        .with_context(|| format!("reading {}", trunk_toml.display()))?;

    // Strip any previously auto-generated section (marker to EOF).
    let base = if let Some(idx) = existing.find(TRUNK_COPY_MARKER) {
        existing[..idx].trim_end().to_string()
    } else {
        existing.trim_end().to_string()
    };

    let mut new_contents = base;
    if !assets.is_empty() {
        new_contents.push_str("\n\n");
        new_contents.push_str(TRUNK_COPY_MARKER);
        new_contents.push('\n');
        for asset in &assets {
            new_contents.push_str(&format!("[[copy]]\nfile = \"../../{asset}\"\n"));
        }
    }

    std::fs::write(&trunk_toml, new_contents)
        .with_context(|| format!("writing {}", trunk_toml.display()))?;
    Ok(())
}

/// Incrementally copy `files` (relative paths) from `src_root` into
/// `dest_root`, preserving their relative layout.
///
/// Split out from [`copy_assets_into`] so the copy logic can be unit-tested
/// against explicit roots without mutating the process-wide current directory.
fn copy_files(
    files: &[String],
    src_root: &Path,
    dest_root: &Path,
) -> anyhow::Result<AssetCopyReport> {
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
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::copy(&src, &dest).with_context(|| {
            format!("copying asset '{}' -> '{}'", src.display(), dest.display())
        })?;
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
        assert_eq!(std::fs::read(dest.path().join("assets/logo.png")).unwrap(), b"PNG-CHANGED");
    }

    // ── sync_trunk_copy_entries ─────────────────────────────────────────

    /// Helper: set up a temp project with an `aimer.toml` and a
    /// `builds/web/Trunk.toml`, then run `sync_trunk_copy_entries_in`.
    fn setup_trunk_sync(aimer_assets: Option<&str>, trunk_initial: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let web_dir = dir.path().join("builds/web");
        std::fs::create_dir_all(&web_dir).unwrap();
        std::fs::write(web_dir.join("Trunk.toml"), trunk_initial).unwrap();

        let mut toml = String::from("[package]\nname = \"test\"\nversion = \"0.1.0\"\n");
        if let Some(assets) = aimer_assets {
            toml.push_str("\n[assets]\nfiles = [\n");
            for a in assets.split(',') {
                toml.push_str(&format!("  \"{a}\",\n"));
            }
            toml.push_str("]\n");
        }
        std::fs::write(dir.path().join("aimer.toml"), &toml).unwrap();
        dir
    }

    #[test]
    fn trunk_sync_adds_copy_entries() {
        let dir = setup_trunk_sync(
            Some("assets/logo.png,assets/sprites/player.png"),
            "[watch]\nwatch = [\"../../src\"]\n\n[serve]\nport = 3000\n",
        );
        sync_trunk_copy_entries_in(dir.path()).unwrap();

        let contents = std::fs::read_to_string(dir.path().join("builds/web/Trunk.toml")).unwrap();
        assert!(contents.contains(TRUNK_COPY_MARKER));
        assert!(contents.contains("[[copy]]\nfile = \"../../assets/logo.png\""));
        assert!(contents.contains("[[copy]]\nfile = \"../../assets/sprites/player.png\""));
        // Original content preserved.
        assert!(contents.contains("[serve]\nport = 3000"));
    }

    #[test]
    fn trunk_sync_replaces_stale_entries() {
        let existing = format!(
            "[watch]\nwatch = [\"../../src\"]\n\n{TRUNK_COPY_MARKER}\n[[copy]]\nfile = \"../../old.png\"\n"
        );
        let dir = setup_trunk_sync(Some("assets/new.png"), &existing);
        sync_trunk_copy_entries_in(dir.path()).unwrap();

        let contents = std::fs::read_to_string(dir.path().join("builds/web/Trunk.toml")).unwrap();
        assert!(!contents.contains("old.png"), "stale entry should be removed");
        assert!(contents.contains("[[copy]]\nfile = \"../../assets/new.png\""));
    }

    #[test]
    fn trunk_sync_no_assets_removes_marker() {
        let existing = format!(
            "[serve]\nport = 3000\n\n{TRUNK_COPY_MARKER}\n[[copy]]\nfile = \"../../stale.png\"\n"
        );
        let dir = setup_trunk_sync(None, &existing);
        sync_trunk_copy_entries_in(dir.path()).unwrap();

        let contents = std::fs::read_to_string(dir.path().join("builds/web/Trunk.toml")).unwrap();
        assert!(!contents.contains(TRUNK_COPY_MARKER));
        assert!(!contents.contains("[[copy]]"));
        assert!(contents.contains("[serve]\nport = 3000"));
    }

    #[test]
    fn trunk_sync_missing_trunk_toml_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        // No builds/web/Trunk.toml — should succeed silently.
        sync_trunk_copy_entries_in(dir.path()).unwrap();
    }
}
