use std::fs;
use std::path::Path;

/// Scaffold `builds/linux/` for a new project.
///
/// Both files are consumed by
/// [`package_desktop`](crate::commands::assemble::package_desktop), which copies
/// them into the portable bundle next to the executable: the entry is named
/// after the project so it matches the binary cargo produces, and its `Exec`
/// and `Icon` keys are rewritten to the bundled names on the way.
pub fn create(dir: &Path, name: &str, _group: &str) {
    fs::create_dir_all(dir.join("builds/linux")).unwrap();
    fs::write(
        dir.join(format!("builds/linux/{name}.desktop")),
        format!(
            r#"[Desktop Entry]
Name={name}
Comment={name}
Exec={name}
Icon={name}
Terminal=false
Type=Application
Categories=Utility;
"#
        ),
    )
    .unwrap();

    // Default application icon
    fs::write(
        dir.join("builds/linux/app.png"),
        include_bytes!("../../../templates/icons/icon_512.png"),
    )
    .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_desktop_entry_is_named_after_the_project() {
        let dir = tempfile::tempdir().unwrap();
        create(dir.path(), "my_app", "com.example");

        let entry = dir.path().join("builds/linux/my_app.desktop");
        assert!(entry.is_file());
        let contents = std::fs::read_to_string(entry).unwrap();
        assert!(contents.contains("Exec=my_app"), "{contents}");
        assert!(contents.contains("Icon=my_app"), "{contents}");
        assert!(!contents.contains("aimer_app"), "{contents}");
        // The icon ships next to the entry, so both land in the bundle.
        assert!(dir.path().join("builds/linux/app.png").is_file());
    }
}
