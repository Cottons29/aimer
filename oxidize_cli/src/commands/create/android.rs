use std::fs;
use std::path::PathBuf;

pub fn create(dir: &PathBuf) {
    fs::create_dir_all(dir.join("builds/android/app/src/main/java/com/example/app")).unwrap();
    fs::create_dir_all(dir.join("builds/android/app/src/main/res/values")).unwrap();

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

    fs::write(
        dir.join("builds/android/app/build.gradle.kts"),
        r#"plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "com.example.app"
    compileSdk = 34

    defaultConfig {
        applicationId = "com.example.app"
        minSdk = 24
        targetSdk = 34
        versionCode = 1
        versionName = "1.0"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    kotlinOptions {
        jvmTarget = "1.8"
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.12.0")
    implementation("androidx.appcompat:appcompat:1.6.1")
    implementation("com.google.android.material:material:1.11.0")
}
"#,
    )
    .unwrap();

    fs::write(
        dir.join("builds/android/app/src/main/AndroidManifest.xml"),
        r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="com.example.app">

    <application
        android:allowBackup="true"
        android:icon="@mipmap/ic_launcher"
        android:label="@string/app_name"
        android:roundIcon="@mipmap/ic_launcher_round"
        android:supportsRtl="true"
        android:theme="@style/Theme.AppCompat.Light.NoActionBar">
        <activity
            android:name=".MainActivity"
            android:exported="true">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>

</manifest>
"#,
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
