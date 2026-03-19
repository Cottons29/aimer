use std::fs;
use std::path::PathBuf;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub fn create(dir: &PathBuf) {
    fs::create_dir_all(dir.join("builds/android/app/src/main/java/com/example/app")).unwrap();
    fs::create_dir_all(dir.join("builds/android/app/src/main/res/values")).unwrap();
    fs::create_dir_all(dir.join("builds/android/gradle/wrapper")).unwrap();

    let gradlew_path = dir.join("builds/android/gradlew");
    fs::write(&gradlew_path, include_str!("../../../templates/android/dot_gradle/gradlew")).unwrap();

    #[cfg(unix)]
    {
        if let Ok(mut perms) = fs::metadata(&gradlew_path).map(|m| m.permissions()) {
            perms.set_mode(0o755);
            let _ = fs::set_permissions(&gradlew_path, perms);
        }
    }

    fs::write(dir.join("builds/android/gradlew.bat"), include_str!("../../../templates/android/dot_gradle/gradlew.bat")).unwrap();

    fs::write(
        dir.join("builds/android/gradle/wrapper/gradle-wrapper.properties"),
        include_str!("../../../templates/android/dot_gradle/gradle-wrapper.properties"),
    )
    .unwrap();

    fs::write(
        dir.join("builds/android/gradle/wrapper/gradle-wrapper.jar"),
        include_bytes!("../../../templates/android/dot_gradle/gradle-wrapper.jar"),
    )
    .unwrap();

    fs::write(dir.join("builds/android/gradle.properties"), "android.useAndroidX=true\n").unwrap();

    fs::write(
        dir.join("builds/android/build.gradle.kts"),
        r#"
plugins {
    id("com.android.application") version "8.2.0" apply false
    id("org.jetbrains.kotlin.android") version "1.9.20" apply false
}
"#,
    )
    .unwrap();

    fs::write(
        dir.join("builds/android/settings.gradle.kts"),
        include_str!("../../../templates/android/settings.gradle.kts.template"),
    )
    .unwrap();

    let project_name = dir.file_name().unwrap().to_str().unwrap();
    let lib_name = project_name.replace("-", "_");

    fs::write(
        dir.join("builds/android/app/build.gradle.kts"),
        include_str!("../../../templates/android/build.gradle.kts.template"),
    )
    .unwrap();

    fs::write(
        dir.join("builds/android/app/src/main/AndroidManifest.xml"),
        include_str!("../../../templates/android/AndroidManifest.xml.template").replace("${app_name}", &lib_name),
    )
    .unwrap();

    fs::write(
        dir.join("builds/android/app/src/main/res/values/strings.xml"),
        format!("<resources>\n    <string name=\"app_name\">{}</string>\n</resources>\n", project_name),
    )
    .unwrap();

    // Default launcher icons
    let mipmap_sizes: &[(&str, &[u8])] = &[
        ("mipmap-mdpi", include_bytes!("../../../templates/icons/icon_48.png")),
        ("mipmap-hdpi", include_bytes!("../../../templates/icons/icon_72.png")),
        ("mipmap-xhdpi", include_bytes!("../../../templates/icons/icon_96.png")),
        ("mipmap-xxhdpi", include_bytes!("../../../templates/icons/icon_144.png")),
        ("mipmap-xxxhdpi", include_bytes!("../../../templates/icons/icon_192.png")),
    ];
    for (folder, data) in mipmap_sizes {
        let mipmap_dir = dir.join(format!("builds/android/app/src/main/res/{}", folder));
        fs::create_dir_all(&mipmap_dir).unwrap();
        fs::write(mipmap_dir.join("ic_launcher.png"), data).unwrap();
    }
}
