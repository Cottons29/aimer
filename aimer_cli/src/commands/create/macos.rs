use std::fs;
use std::path::Path;

pub fn create(dir: &Path, name: &str, group: &str) {
    let project_name = name;
    let project_name_lib = project_name.replace("-", "_");
    let macos_dir = dir.join("builds/macos");
    fs::create_dir_all(&macos_dir).unwrap();
    fs::create_dir_all(macos_dir.join(format!("{}.xcodeproj", project_name))).unwrap();

    fs::write(
        macos_dir.join(format!("{}.xcodeproj/project.pbxproj", project_name)),
        include_str!("../../../templates/macos/project.pbxproj.template")
            .replace("${project_name}", project_name)
            .replace("${project_name_lib}", &project_name_lib),
    )
    .unwrap();

    let app_dir = macos_dir.join(project_name);
    fs::create_dir_all(&app_dir).unwrap();

    fs::write(
        app_dir.join("Info.plist"),
        include_str!("../../../templates/macos/info.plist.template")
            .replace("${project_name}", project_name)
            .replace("${group}", group),
    )
    .unwrap();

    fs::write(
        app_dir.join("main.swift"),
        r#"
import Foundation

@_silgen_name("__generated_entrance_point")
func __generated_entrance_point()
__generated_entrance_point()
"#,
    )
    .unwrap();

    // Default AppIcon asset catalog
    let appiconset_dir = app_dir.join("Assets.xcassets/AppIcon.appiconset");
    fs::create_dir_all(&appiconset_dir).unwrap();

    let icon_sizes: &[(&str, &[u8])] = &[
        (
            "icon_16.png",
            include_bytes!("../../../templates/icons/icon_16.png"),
        ),
        (
            "icon_32.png",
            include_bytes!("../../../templates/icons/icon_32.png"),
        ),
        (
            "icon_64.png",
            include_bytes!("../../../templates/icons/icon_64.png"),
        ),
        (
            "icon_128.png",
            include_bytes!("../../../templates/icons/icon_128.png"),
        ),
        (
            "icon_256.png",
            include_bytes!("../../../templates/icons/icon_256.png"),
        ),
        (
            "icon_512.png",
            include_bytes!("../../../templates/icons/icon_512.png"),
        ),
        (
            "icon_1024.png",
            include_bytes!("../../../templates/icons/icon_1024.png"),
        ),
    ];
    for (name, data) in icon_sizes {
        fs::write(appiconset_dir.join(name), data).unwrap();
    }

    fs::write(
        appiconset_dir.join("Contents.json"),
        r#"{
  "images" : [
    {
      "filename" : "icon_16.png",
      "idiom" : "mac",
      "scale" : "1x",
      "size" : "16x16"
    },
    {
      "filename" : "icon_32.png",
      "idiom" : "mac",
      "scale" : "2x",
      "size" : "16x16"
    },
    {
      "filename" : "icon_32.png",
      "idiom" : "mac",
      "scale" : "1x",
      "size" : "32x32"
    },
    {
      "filename" : "icon_64.png",
      "idiom" : "mac",
      "scale" : "2x",
      "size" : "32x32"
    },
    {
      "filename" : "icon_128.png",
      "idiom" : "mac",
      "scale" : "1x",
      "size" : "128x128"
    },
    {
      "filename" : "icon_256.png",
      "idiom" : "mac",
      "scale" : "2x",
      "size" : "128x128"
    },
    {
      "filename" : "icon_256.png",
      "idiom" : "mac",
      "scale" : "1x",
      "size" : "256x256"
    },
    {
      "filename" : "icon_512.png",
      "idiom" : "mac",
      "scale" : "2x",
      "size" : "256x256"
    },
    {
      "filename" : "icon_512.png",
      "idiom" : "mac",
      "scale" : "1x",
      "size" : "512x512"
    },
    {
      "filename" : "icon_1024.png",
      "idiom" : "mac",
      "scale" : "2x",
      "size" : "512x512"
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
