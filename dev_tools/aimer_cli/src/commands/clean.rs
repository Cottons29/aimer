use anyhow::Context;
use std::path::Path;
use std::process::Command;

/// Remove build artifacts: the `builds/` output tree and Cargo's `target/`.
pub fn execute() -> anyhow::Result<()> {
    let build_paths = [
        Path::new("builds/macos/build"),
        Path::new("builds/ios/build"),
        Path::new("builds/web/pkg"),
        Path::new("builds/web/node_modules"),
        Path::new("builds/web/package-lock.json"),
        Path::new("builds/macos/Libraries"),
        Path::new("builds/ios/Libraries"),
        Path::new("builds/android/app/build"),
        Path::new("builds/android/app/src/main/jniLibs"),
    ];

    for build in build_paths {
        if build.exists() {
            if build.is_dir() {
                std::fs::remove_dir_all(build).context("removing build directory")?;
            } else if build.is_file() {
                std::fs::remove_file(build).context("removing build file")?;
            }

            println!("Removed {}", build.display());
        } else {
            println!("No {} directory to remove.", build.display());
        }
    }

    // Delegate target/ cleanup to cargo so it respects workspace layout.
    if Path::new("Cargo.toml").exists() {
        let status =
            Command::new("cargo").arg("clean").status().context("running `cargo clean`")?;
        if status.success() {
            println!("Ran `cargo clean`.");
        } else {
            eprintln!("`cargo clean` exited with a non-zero status.");
        }
    }

    println!("Clean complete.");
    Ok(())
}
