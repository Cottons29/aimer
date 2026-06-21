use std::fs;
use std::path::Path;

pub fn create(dir: &Path) {
    fs::create_dir_all(dir.join("builds/linux")).unwrap();
    fs::write(
        dir.join("builds/linux/app.desktop"),
        r#"[Desktop Entry]
Name=AimerApp
Comment=Aimer Application
Exec=aimer_app
Icon=aimer
Terminal=false
Type=Application
Categories=Utility;
"#,
    ).unwrap();

    // Default application icon
    fs::write(
        dir.join("builds/linux/app.png"),
        include_bytes!("../../../templates/icons/icon_512.png"),
    ).unwrap();
}
