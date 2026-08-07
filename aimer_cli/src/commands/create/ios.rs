use std::fs;
use std::path::Path;

use crate::commands::assemble::link_flags;

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

    // The project links whatever `RustLinkFlags.xcconfig` says; until the
    // first `aimer` build derives the real list from the crate graph, the
    // defaults keep an Xcode-only build working.
    link_flags::scaffold(&ios_dir).unwrap();

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::assemble::link_flags::{LDFLAGS_SETTING, XCCONFIG_FILE};

    #[test]
    fn the_scaffold_ships_the_generated_link_configuration() {
        let dir = scaffolded("my_app");

        let xcconfig = fs::read_to_string(dir.join("builds/ios").join(XCCONFIG_FILE)).unwrap();
        assert!(xcconfig.contains(&format!("{LDFLAGS_SETTING} = ")), "{xcconfig}");
        assert!(xcconfig.contains("-framework CoreHaptics"), "{xcconfig}");
    }

    #[test]
    fn the_project_links_what_the_configuration_says() {
        let dir = scaffolded("my_app");

        let pbxproj =
            fs::read_to_string(dir.join("builds/ios/my_app.xcodeproj/project.pbxproj")).unwrap();
        // The frameworks are never spelled out in the project: they come from
        // the generated xcconfig, so a new Apple binding on the Rust side
        // needs no Xcode change.
        assert!(pbxproj.contains(&format!("$({LDFLAGS_SETTING})")), "{pbxproj}");
        assert!(pbxproj.contains("baseConfigurationReference"), "{pbxproj}");
        assert!(!pbxproj.contains("\"-framework\","), "{pbxproj}");
    }

    #[test]
    fn the_scaffold_ships_the_revisioned_ios_text_bridge() {
        let dir = scaffolded("my_app");

        let swift = fs::read_to_string(dir.join("builds/ios/my_app/main.swift")).unwrap();
        for required in [
            "@_silgen_name(\"trigger_rust_text_editing_delta\")",
            "@_cdecl(\"aimer_ios_sync_text_state\")",
            "textView.selectedRange",
            "textViewDidChangeSelection",
        ] {
            assert!(swift.contains(required), "missing {required} in generated main.swift");
        }
        assert!(!swift.contains("private let placeholder"), "{swift}");
    }

    /// A scratch project directory with the iOS scaffold in it.
    fn scaffolded(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "aimer-create-ios-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        create(&dir, name, "com.example.app");
        dir
    }
}
