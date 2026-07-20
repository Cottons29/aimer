use std::fs;
use std::path::Path;

pub fn create(dir: &Path, name: &str, group: &str) {
    let project_name = name;
    let project_name_lib = project_name.replace("-", "_");
    let ios_dir = dir.join("builds/ios");
    fs::create_dir_all(&ios_dir).unwrap();
    fs::create_dir_all(ios_dir.join(format!("{}.xcodeproj", project_name))).unwrap();

    let xcode_proj_template = include_str!("../../../templates/ios/project.pbxproj.template")
        .replace("${project_name}", project_name)
        .replace("${project_name_lib}", &project_name_lib);

    fs::write(
        ios_dir.join(format!("{}.xcodeproj/project.pbxproj", project_name)),
        xcode_proj_template,
    )
    .unwrap();

    let app_dir = ios_dir.join(project_name);
    fs::create_dir_all(&app_dir).unwrap();

    fs::write(
        app_dir.join("Info.plist"),
        include_str!("../../../templates/ios/Info.plist.template")
            .replace("${project_name}", project_name)
            .replace("${group}", group),
    )
    .unwrap();

    fs::write(
        app_dir.join("main.swift"),
        include_str!("../../../templates/ios/main.swift.template"),
    )
    .unwrap();

    // Default AppIcon asset catalog
    let appiconset_dir = app_dir.join("Assets.xcassets/AppIcon.appiconset");
    fs::create_dir_all(&appiconset_dir).unwrap();
    fs::write(
        appiconset_dir.join("icon_1024.png"),
        include_bytes!("../../../templates/icons/icon_1024.png"),
    )
    .unwrap();
    fs::write(
        appiconset_dir.join("Contents.json"),
        r#"{
  "images" : [
    {
      "filename" : "icon_1024.png",
      "idiom" : "universal",
      "platform" : "ios",
      "size" : "1024x1024"
    }
  ],
  "info" : {
    "author" : "aimer",
    "version" : 1
  }
}
"#,
    )
    .unwrap();
}
