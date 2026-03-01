use std::fs;
use std::path::PathBuf;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub fn create(dir: &PathBuf) {
    fs::create_dir_all(dir.join("builds/android/app/src/main/java/com/example/app")).unwrap();
    fs::create_dir_all(dir.join("builds/android/app/src/main/res/values")).unwrap();
    fs::create_dir_all(dir.join("builds/android/gradle/wrapper")).unwrap();

    let gradlew_path = dir.join("builds/android/gradlew");
    fs::write(&gradlew_path, include_str!("assets/gradlew")).unwrap();
    
    #[cfg(unix)]
    {
        if let Ok(mut perms) = fs::metadata(&gradlew_path).map(|m| m.permissions()) {
            perms.set_mode(0o755);
            let _ = fs::set_permissions(&gradlew_path, perms);
        }
    }

    fs::write(
        dir.join("builds/android/gradlew.bat"),
        include_str!("assets/gradlew.bat"),
    )
    .unwrap();

    fs::write(
        dir.join("builds/android/gradle/wrapper/gradle-wrapper.properties"),
        include_str!("assets/gradle-wrapper.properties"),
    )
    .unwrap();

    fs::write(
        dir.join("builds/android/gradle/wrapper/gradle-wrapper.jar"),
        include_bytes!("assets/gradle-wrapper.jar"),
    )
    .unwrap();

    fs::write(
        dir.join("builds/android/gradle.properties"),
        "android.useAndroidX=true\n",
    )
    .unwrap();

    fs::write(
        dir.join("builds/android/build.gradle.kts"),
        r#"plugins {
    id("com.android.application") version "8.2.0" apply false
    id("org.jetbrains.kotlin.android") version "1.9.20" apply false
}
"#,
    )
    .unwrap();

    fs::write(
        dir.join("builds/android/settings.gradle.kts"),
        r#"pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}
rootProject.name = "android"
include(":app")
"#,
    )
    .unwrap();

    let project_name = dir.file_name().unwrap().to_str().unwrap();
    let lib_name = project_name.replace("-", "_");

    fs::write(
        dir.join("builds/android/app/build.gradle.kts"),
        format!(r#"plugins {{
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}}

android {{
    namespace = "com.example.app"
    compileSdk = 34

    defaultConfig {{
        applicationId = "com.example.app"
        minSdk = 24
        targetSdk = 34
        versionCode = 1
        versionName = "1.0"
    }}

    buildTypes {{
        release {{
            isMinifyEnabled = false
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }}
    }}
    compileOptions {{
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }}
    kotlinOptions {{
        jvmTarget = "1.8"
    }}
}}

dependencies {{
    implementation("androidx.core:core-ktx:1.12.0")
    implementation("androidx.appcompat:appcompat:1.6.1")
    implementation("com.google.android.material:material:1.11.0")
}}
"#),
    )
    .unwrap();

    fs::write(
        dir.join("builds/android/app/src/main/AndroidManifest.xml"),
        format!(r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="com.example.app">

    <application
        android:allowBackup="true"
        android:label="@string/app_name"
        android:supportsRtl="true"
        android:theme="@style/Theme.AppCompat.Light.NoActionBar">
        <activity
            android:name="android.app.NativeActivity"
            android:exported="true"
            android:configChanges="orientation|keyboardHidden|screenSize">
            <meta-data android:name="android.app.lib_name" android:value="{0}" />
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>

</manifest>
"#, lib_name),
    )
    .unwrap();

    fs::write(
        dir.join("builds/android/app/src/main/res/values/strings.xml"),
        r#"<resources>
    <string name="app_name">Android</string>
</resources>
"#,
    )
    .unwrap();

    fs::write(
        dir.join("builds/android/app/src/main/java/com/example/app/MainActivity.kt"),
        r#"package com.example.app

import androidx.appcompat.app.AppCompatActivity
import android.os.Bundle

class MainActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
    }
}
"#,
    )
    .unwrap();
}
