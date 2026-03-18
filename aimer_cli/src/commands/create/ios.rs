use std::fs;
use std::path::PathBuf;

pub fn create(dir: &PathBuf) {
    let project_name = dir.file_name().unwrap().to_str().unwrap();
    let project_name_lib = project_name.replace("-", "_");
    let ios_dir = dir.join("builds/ios");
    fs::create_dir_all(&ios_dir).unwrap();
    fs::create_dir_all(ios_dir.join(format!("{}.xcodeproj", project_name))).unwrap();
    
    let xcode_proj_template: &str = &*include_str!("../../../templates/ios/project.pbxproj.template")
        .replace("${project_name}", project_name)
        .replace("${project_name_lib}", &project_name_lib);
    
    fs::write(
        ios_dir.join(format!("{}.xcodeproj/project.pbxproj", project_name)),
        xcode_proj_template
    )
    .unwrap();

    let app_dir = ios_dir.join(project_name);
    fs::create_dir_all(&app_dir).unwrap();
    
    fs::write(
        app_dir.join("Info.plist"),
        include_str!("../../../templates/ios/Info.plist.template")
    ).unwrap();

    fs::write(
        app_dir.join("main.swift"),
        include_str!("../../../templates/ios/main.swift.template")
    ).unwrap();
}
