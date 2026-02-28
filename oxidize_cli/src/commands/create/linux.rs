use std::fs;
use std::path::PathBuf;

pub fn create(dir: &PathBuf) {
    fs::create_dir_all(dir.join("builds/linux")).unwrap();
    fs::write(
        dir.join("builds/linux/app.desktop"),
        r#"[Desktop Entry]
Name=OxidizeApp
Comment=Oxidize Application
Exec=oxidize_app
Icon=oxidize
Terminal=false
Type=Application
Categories=Utility;
"#,
    ).unwrap();
}
