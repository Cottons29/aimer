use std::fs;
use std::path::Path;

pub fn create(dir: &Path, name: &str, _group: &str) {
    fs::create_dir_all(dir.join("builds/linux")).unwrap();
    fs::write(
        dir.join("builds/linux/app.desktop"),
        format!(
            r#"[Desktop Entry]
Name={name}
Comment={name}
Exec=aimer_app
Icon=aimer
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
