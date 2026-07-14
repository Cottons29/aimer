use crate::config::AimerManifest;
use anyhow::{Context, bail};
use std::fs;
use std::path::Path;

/// Migrate platform build scaffolds to the latest version.
///
/// Reads `Aimer.toml` to get the project `name` and `group`, then regenerates
/// the build scaffold for the requested target (or all targets) using the
/// templates bundled in the current CLI version.
pub fn execute(target: String) -> anyhow::Result<()> {
    execute_in(target, Path::new("."))
}

/// Internal implementation that accepts an explicit directory for testability.
fn execute_in(target: String, dir: &Path) -> anyhow::Result<()> {
    let manifest = AimerManifest::load_from(dir)
        .context("failed to read aimer.toml")?
        .context("no Aimer.toml found — run this command from an Aimer project root")?;

    // Resolve to an absolute path. The `create::*` scaffolders derive the
    // project name from `dir.file_name()`, but a relative `.` (the usual case
    // when running from the project root) has no file name and would otherwise
    // make scaffolding fail, so the target folders never get generated.
    let canonical = dir
        .canonicalize()
        .with_context(|| format!("resolving project directory {}", dir.display()))?;
    let dir = canonical.as_path();

    let name = manifest.package.name.clone();
    let group = if manifest.package.group.is_empty() {
        "com.example.app".to_string()
    } else {
        manifest.package.group.clone()
    };

    match target.as_str() {
        "web" => migrate_web(dir, &name, &group)?,
        "macos" => migrate_platform(dir, "macos", &name, &group, &create::macos::create)?,
        "ios" => migrate_platform(dir, "ios", &name, &group, &create::ios::create)?,
        "android" => migrate_platform(dir, "android", &name, &group, &create::android::create)?,
        "windows" => migrate_platform(dir, "windows", &name, &group, &create::window::create)?,
        "linux" => migrate_platform(dir, "linux", &name, &group, &create::linux::create)?,
        "all" => {
            migrate_web(dir, &name, &group)?;
            migrate_platform(dir, "macos", &name, &group, &create::macos::create)?;
            migrate_platform(dir, "ios", &name, &group, &create::ios::create)?;
            migrate_platform(dir, "android", &name, &group, &create::android::create)?;
            migrate_platform(dir, "windows", &name, &group, &create::window::create)?;
            migrate_platform(dir, "linux", &name, &group, &create::linux::create)?;
        }
        other => bail!(
            "unknown target '{other}'. Valid targets: macos, windows, linux, android, ios, web, all"
        ),
    }

    println!("Migration complete for '{target}'. Project: {name} (group: {group})");
    Ok(())
}

/// Migrate the web scaffold.
///
/// Selectively removes only the scaffold files (Trunk.toml, index.html,
/// favicon, apple-touch-icon) so that build artifacts in `pkg/` and `dist/`
/// are preserved, then regenerates from the latest templates.
fn migrate_web(dir: &Path, name: &str, group: &str) -> anyhow::Result<()> {
    let web_dir = dir.join("builds/web");
    if web_dir.exists() {
        for file in &["Trunk.toml", "index.html", "favicon.ico", "apple-touch-icon.png"] {
            let path = web_dir.join(file);
            if path.exists() {
                fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
            }
        }
    }
    create::web::create(dir, name, group);
    println!("  ✔ web scaffold migrated");
    Ok(())
}

/// Migrate a non-web platform scaffold.
///
/// Removes the entire `builds/<platform>/` directory and regenerates it from
/// the latest templates.
fn migrate_platform(
    dir: &Path,
    platform: &str,
    name: &str,
    group: &str,
    create_fn: &dyn Fn(&Path, &str, &str),
) -> anyhow::Result<()> {
    let platform_dir = dir.join("builds").join(platform);
    if platform_dir.exists() {
        fs::remove_dir_all(&platform_dir)
            .with_context(|| format!("removing builds/{platform}/"))?;
    }
    create_fn(dir, name, group);
    println!("  ✔ {platform} scaffold migrated");
    Ok(())
}

// Re-export create modules so we can call their `create()` functions.
use crate::commands::create;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_web_preserves_pkg_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        // Simulate an existing project with a web scaffold and a pkg/ dir.
        let web_dir = dir.join("builds/web");
        fs::create_dir_all(web_dir.join("pkg")).unwrap();
        fs::write(web_dir.join("Trunk.toml"), "old content").unwrap();
        fs::write(web_dir.join("index.html"), "old content").unwrap();
        fs::write(web_dir.join("pkg/artifact.wasm"), "wasm data").unwrap();

        migrate_web(dir, "demo-app", "com.example.demo").unwrap();

        // Scaffold files are regenerated.
        let html = fs::read_to_string(web_dir.join("index.html")).unwrap();
        assert!(
            html.contains("data-trunk"),
            "index.html should be regenerated with Trunk attributes"
        );

        // Build artifacts are preserved.
        assert!(web_dir.join("pkg/artifact.wasm").exists(), "pkg/ artifacts should be preserved");
    }

    #[test]
    fn migrate_platform_removes_and_regenerates() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        // Simulate an existing linux scaffold.
        let linux_dir = dir.join("builds/linux");
        fs::create_dir_all(&linux_dir).unwrap();
        fs::write(linux_dir.join("old_file.txt"), "old").unwrap();

        migrate_platform(dir, "linux", "demo-app", "com.example.demo", &create::linux::create)
            .unwrap();

        // Old files are gone, new scaffold is present.
        assert!(!linux_dir.join("old_file.txt").exists(), "old files should be removed");
        assert!(linux_dir.join("app.desktop").exists(), "new scaffold should be generated");
    }

    #[test]
    fn migrate_rejects_unknown_target() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        fs::write(dir.join("aimer.toml"), "[application]\nname = \"test\"\nversion = \"0.1.0\"\n")
            .unwrap();

        let result = execute_in("playstation".to_string(), dir);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unknown target"));
    }

    #[test]
    fn migrate_requires_aimer_toml() {
        let tmp = tempfile::tempdir().unwrap();
        let result = execute_in("web".to_string(), tmp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no Aimer.toml"));
    }
}
