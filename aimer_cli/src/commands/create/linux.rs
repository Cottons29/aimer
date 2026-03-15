use std::fs;
use std::path::PathBuf;

pub fn create(dir: &PathBuf) {
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
}
