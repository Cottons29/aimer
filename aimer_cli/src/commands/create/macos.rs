use std::fs;
use std::path::PathBuf;

pub fn create(dir: &PathBuf) {
    let project_name = dir.file_name().unwrap().to_str().unwrap();
    let project_name_lib = project_name.replace("-", "_");
    let macos_dir = dir.join("builds/macos");
    fs::create_dir_all(&macos_dir).unwrap();
    fs::create_dir_all(macos_dir.join(format!("{}.xcodeproj", project_name))).unwrap();

    fs::write(
        macos_dir.join(format!("{}.xcodeproj/project.pbxproj", project_name)),
        include_str!("../../../templates/macos/project.pbxproj.template").replace("${project_name}", project_name).replace("${project_name_lib}", &project_name_lib)
    )
    .unwrap();

    let app_dir = macos_dir.join(project_name);
    fs::create_dir_all(&app_dir).unwrap();

    fs::write(
        app_dir.join("Info.plist"),
        include_str!("../../../templates/macos/info.plist.template"),
    ).unwrap();

    fs::write(
        app_dir.join("main.swift"),
        r#"
import Foundation

@_silgen_name("__generated_entrance_point")
func __generated_entrance_point()
__generated_entrance_point()
"#,
    ).unwrap();
}
