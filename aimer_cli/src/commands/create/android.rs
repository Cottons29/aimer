use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Gradle distribution the bundled wrapper (`gradlew`, `gradle-wrapper.jar`)
/// was generated from.
///
/// Keep this in sync with `templates/android/dot_gradle/*`: the wrapper scripts
/// and jar are shipped verbatim, so bumping only this constant would leave the
/// project with a mismatched launcher.
const GRADLE_VERSION: &str = "9.6.1";

/// SHA-256 of `gradle-{GRADLE_VERSION}-bin.zip`, as published on
/// <https://services.gradle.org/distributions/>.
///
/// Pinning it makes the wrapper reject a tampered or truncated distribution
/// instead of executing it.
const GRADLE_DISTRIBUTION_SHA256: &str =
    "9c0f7faeeb306cb14e4279a3e084ca6b596894089a0638e68a07c945a32c9e14";

/// Android Gradle plugin applied by the generated root build script.
///
/// AGP 9 ships built-in Kotlin support, which is why no separate
/// `org.jetbrains.kotlin.android` plugin is declared anywhere in the scaffold —
/// applying it would clash with AGP's own `kotlin` extension.
const ANDROID_GRADLE_PLUGIN_VERSION: &str = "9.3.1";

pub fn create(dir: &Path, name: &str, group: &str) {
    fs::create_dir_all(dir.join("builds/android/app/src/main/res/values")).unwrap();
    fs::create_dir_all(dir.join("builds/android/gradle/wrapper")).unwrap();

    // Custom `NativeActivity` subclass that adds a hidden `EditText` so the system
    // IME has a real `InputConnection` to compose into. This is what makes
    // CJK / emoji / autocorrect input work on Android (a bare `NativeActivity`
    // surface cannot receive composed text). It forwards committed text into Rust
    // through the `nativeInsertText` / `nativeBackspace` JNI bridge.
    let aimer_activity_dir = dir.join("builds/android/app/src/main/kotlin/com/aimer");
    fs::create_dir_all(&aimer_activity_dir).unwrap();
    fs::write(
        aimer_activity_dir.join("AimerActivity.kt"),
        include_str!("../../../templates/android/AimerActivity.kt.template"),
    )
    .unwrap();

    let gradlew_path = dir.join("builds/android/gradlew");
    fs::write(
        &gradlew_path,
        include_str!("../../../templates/android/dot_gradle/gradlew"),
    )
    .unwrap();

    #[cfg(unix)]
    {
        if let Ok(mut perms) = fs::metadata(&gradlew_path).map(|m| m.permissions()) {
            perms.set_mode(0o755);
            let _ = fs::set_permissions(&gradlew_path, perms);
        }
    }

    fs::write(
        dir.join("builds/android/gradlew.bat"),
        include_str!("../../../templates/android/dot_gradle/gradlew.bat"),
    )
    .unwrap();

    fs::write(
        dir.join("builds/android/gradle/wrapper/gradle-wrapper.properties"),
        gradle_wrapper_properties(),
    )
    .unwrap();

    fs::write(
        dir.join("builds/android/gradle/wrapper/gradle-wrapper.jar"),
        include_bytes!("../../../templates/android/dot_gradle/gradle-wrapper.jar"),
    )
    .unwrap();

    fs::write(
        dir.join("builds/android/gradle.properties"),
        "android.useAndroidX=true\n",
    )
    .unwrap();

    fs::write(
        dir.join("builds/android/build.gradle.kts"),
        format!(
            "plugins {{\n    id(\"com.android.application\") version \"{ANDROID_GRADLE_PLUGIN_VERSION}\" apply false\n}}\n"
        ),
    )
    .unwrap();

    fs::write(
        dir.join("builds/android/settings.gradle.kts"),
        include_str!("../../../templates/android/settings.gradle.kts.template"),
    )
    .unwrap();

    let project_name = name;
    let lib_name = project_name.replace("-", "_");

    fs::write(
        dir.join("builds/android/app/build.gradle.kts"),
        include_str!("../../../templates/android/build.gradle.kts.template")
            .replace("${group}", group),
    )
    .unwrap();

    fs::write(
        dir.join("builds/android/app/src/main/AndroidManifest.xml"),
        include_str!("../../../templates/android/AndroidManifest.xml.template")
            .replace("${app_name}", &lib_name),
    )
    .unwrap();

    fs::write(
        dir.join("builds/android/app/src/main/res/values/strings.xml"),
        format!(
            "<resources>\n    <string name=\"app_name\">{}</string>\n</resources>\n",
            project_name
        ),
    )
    .unwrap();

    fs::write(
        dir.join("builds/android/app/src/main/res/values/themes.xml"),
        r#"<resources>
    <style name="AimerFullscreenTheme" parent="Theme.AppCompat.Light.NoActionBar">

    </style>
</resources>
"#,
    )
    .unwrap();

    //<item name="android:windowFullscreen">true</item>
    //<item name="android:windowNoTitle">true</item>

    // Default launcher icons
    let mipmap_sizes: &[(&str, &[u8])] = &[
        (
            "mipmap-mdpi",
            include_bytes!("../../../templates/icons/icon_48.png"),
        ),
        (
            "mipmap-hdpi",
            include_bytes!("../../../templates/icons/icon_72.png"),
        ),
        (
            "mipmap-xhdpi",
            include_bytes!("../../../templates/icons/icon_96.png"),
        ),
        (
            "mipmap-xxhdpi",
            include_bytes!("../../../templates/icons/icon_144.png"),
        ),
        (
            "mipmap-xxxhdpi",
            include_bytes!("../../../templates/icons/icon_192.png"),
        ),
    ];
    for (folder, data) in mipmap_sizes {
        let mipmap_dir = dir.join(format!("builds/android/app/src/main/res/{}", folder));
        fs::create_dir_all(&mipmap_dir).unwrap();
        fs::write(mipmap_dir.join("ic_launcher.png"), data).unwrap();
    }
}

/// The `gradle-wrapper.properties` shipped with a freshly scaffolded project.
///
/// Generated from [`GRADLE_VERSION`] and [`GRADLE_DISTRIBUTION_SHA256`] rather
/// than embedded as a template, so the distribution the wrapper downloads and
/// the checksum it verifies can never drift apart.
fn gradle_wrapper_properties() -> String {
    format!(
        "distributionBase=GRADLE_USER_HOME\n\
         distributionPath=wrapper/dists\n\
         distributionSha256Sum={GRADLE_DISTRIBUTION_SHA256}\n\
         distributionUrl=https\\://services.gradle.org/distributions/gradle-{GRADLE_VERSION}-bin.zip\n\
         networkTimeout=10000\n\
         retries=0\n\
         retryBackOffMs=500\n\
         validateDistributionUrl=true\n\
         zipStoreBase=GRADLE_USER_HOME\n\
         zipStorePath=wrapper/dists\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scaffold a throwaway Android project and hand its `builds/android`
    /// directory to `assert`.
    fn with_scaffold(assert: impl FnOnce(&Path)) {
        let tmp = tempfile::tempdir().unwrap();
        create(tmp.path(), "demo-app", "com.example.demo");
        assert(&tmp.path().join("builds/android"));
    }

    #[test]
    fn activity_is_generated_as_kotlin() {
        with_scaffold(|android| {
            assert!(
                android
                    .join("app/src/main/kotlin/com/aimer/AimerActivity.kt")
                    .exists(),
                "the activity must be scaffolded as Kotlin"
            );
            assert!(
                !android
                    .join("app/src/main/java/com/aimer/AimerActivity.java")
                    .exists(),
                "the Java activity must no longer be scaffolded"
            );
        });
    }

    #[test]
    fn kotlin_activity_keeps_the_jni_entry_points() {
        with_scaffold(|android| {
            let source =
                fs::read_to_string(android.join("app/src/main/kotlin/com/aimer/AimerActivity.kt"))
                    .unwrap();

            // The JNI export in `aimer_quiver` resolves against a static native,
            // which in Kotlin means an `@JvmStatic external` companion member.
            assert!(source.contains("@JvmStatic"));
            assert!(source.contains("external fun nativeTextEditingDelta("));
            for parameter in [
                "sessionId: Long",
                "revision: Long",
                "replaceStart: Int",
                "replaceEnd: Int",
                "replacementText: String",
                "selectionStart: Int",
                "selectionEnd: Int",
                "composingStart: Int",
                "composingEnd: Int",
            ] {
                assert!(source.contains(parameter), "missing JNI parameter {parameter}");
            }
            assert!(!source.contains("external fun nativeInsertText"));
            assert!(!source.contains("external fun nativeSetComposingText"));
            assert!(!source.contains("external fun nativeBackspace"));
        });
    }

    #[test]
    fn kotlin_activity_mirrors_revisioned_editor_state() {
        with_scaffold(|android| {
            let source =
                fs::read_to_string(android.join("app/src/main/kotlin/com/aimer/AimerActivity.kt"))
                    .unwrap();

            for required in [
                "fun syncTextState(",
                "private var sessionId = 0L",
                "private var revision = 0L",
                "beforeTextChanged",
                "afterTextChanged",
                "onSelectionChanged",
                "BaseInputConnection.getComposingSpanStart",
                "BaseInputConnection.getComposingSpanEnd",
                "setComposingText",
                "setComposingRegion",
                "InputType.TYPE_CLASS_NUMBER",
                "InputType.TYPE_TEXT_VARIATION_PASSWORD",
                "PasswordTransformationMethod.getInstance()",
                "restartInput(view)",
                "revision += 1",
                "revision < this.revision",
                "reportDelta(selectionStart, selectionStart, \"\")",
                "suppressCallbacks = true",
                "suppressCallbacks = false",
            ] {
                assert!(source.contains(required), "missing {required} in generated activity");
            }
            assert!(!source.contains("PLACEHOLDER"));
        });
    }

    #[test]
    fn gradle_wrapper_is_pinned_to_the_current_gradle() {
        with_scaffold(|android| {
            let properties =
                fs::read_to_string(android.join("gradle/wrapper/gradle-wrapper.properties"))
                    .unwrap();
            assert!(
                properties.contains(&format!("gradle-{GRADLE_VERSION}-bin.zip")),
                "wrapper must request Gradle {GRADLE_VERSION}: {properties}"
            );
            assert!(
                properties.contains("distributionSha256Sum="),
                "the distribution must be checksum-verified"
            );
        });
    }

    #[test]
    fn root_build_script_applies_only_the_android_plugin() {
        with_scaffold(|android| {
            let root = fs::read_to_string(android.join("build.gradle.kts")).unwrap();
            assert!(root.contains(&format!(
                "id(\"com.android.application\") version \"{ANDROID_GRADLE_PLUGIN_VERSION}\""
            )));
            // AGP 9 ships built-in Kotlin; applying the standalone plugin on top
            // fails with a duplicate `kotlin` extension.
            assert!(
                !root.contains("org.jetbrains.kotlin.android"),
                "the Kotlin Android plugin must not be applied alongside AGP 9"
            );
        });
    }

    #[test]
    fn app_build_script_uses_the_built_in_kotlin_dsl() {
        with_scaffold(|android| {
            let app = fs::read_to_string(android.join("app/build.gradle.kts")).unwrap();
            assert!(!app.contains("org.jetbrains.kotlin.android"));
            assert!(
                !app.contains("kotlinOptions"),
                "`kotlinOptions` was removed in AGP 9"
            );
            assert!(app.contains("compilerOptions"));
            assert!(app.contains("JvmTarget.JVM_17"));
            assert!(app.contains("applicationId = \"com.example.demo\""));
        });
    }

    #[test]
    fn manifest_leaves_the_application_id_to_gradle() {
        with_scaffold(|android| {
            let manifest =
                fs::read_to_string(android.join("app/src/main/AndroidManifest.xml")).unwrap();
            // A `package` attribute is ignored (and warned about) since AGP 8.
            assert!(!manifest.contains("package="));
            assert!(manifest.contains("android:name=\"com.aimer.AimerActivity\""));
            // The native library name still has to be spelled out for
            // `NativeActivity` to find the Rust `cdylib`.
            assert!(manifest.contains("android:value=\"demo_app\""));
        });
    }

    #[test]
    fn kotlin_source_set_is_registered_for_the_application() {
        with_scaffold(|android| {
            // The activity lives in `src/main/kotlin`, which AGP only compiles
            // when the source set is declared.
            let app = fs::read_to_string(android.join("app/build.gradle.kts")).unwrap();
            assert!(app.contains("src/main/kotlin"));
        });
    }
}
