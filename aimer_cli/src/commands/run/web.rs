use std::env;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};

use crossbeam::channel::Sender;

use crate::commands::assemble::copy_assets_into;
use crate::commands::run::Device;
use crate::commands::run::cargo_build::{
    stream_stderr_as_app_log, stream_stdout_as_app_log, wait_for_child,
};
use crate::commands::run::console::{RunnerEvent, Status};
use crate::commands::run::helpers::{build_log, fail, set_status, spawn_streamed};

pub fn find_llvm_ar() -> Option<PathBuf> {
    // 1. Explicit override via environment variable
    if let Ok(path) = env::var("LLVM_AR") {
        let p = PathBuf::from(path);
        if p.is_file() {
            return Some(p);
        }
    }

    // 2. Look for versioned or plain names on PATH
    let candidates = [
        "llvm-ar",
        "llvm-ar-18",
        "llvm-ar-17",
        "llvm-ar-16",
        "llvm-ar-15",
        "llvm-ar-14",
    ];

    if let Ok(path_var) = env::var("PATH") {
        for dir in env::split_paths(&path_var) {
            for name in &candidates {
                let full = dir.join(name);
                if full.is_file() {
                    return Some(full);
                }
            }
        }
    }

    // 3. Fall back to `which`/`where` as a last resort
    let which_cmd = if cfg!(windows) { "where" } else { "which" };
    if let Ok(output) = Command::new(which_cmd)
        .arg("llvm-ar")
        .output()
    {
        if output.status.success() {
            let found = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .map(str::trim)
                .map(PathBuf::from);
            if let Some(p) = found {
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }

    // 4. Common install locations
    let common_paths = [
        "/usr/bin/llvm-ar",
        "/usr/local/bin/llvm-ar",
        "/opt/homebrew/opt/llvm/bin/llvm-ar",
        "/opt/llvm/bin/llvm-ar",
    ];
    common_paths
        .iter()
        .map(Path::new)
        .find(|p| p.is_file())
        .map(|p| p.to_path_buf())
}

pub fn configure_trunk(command: &mut Command, llvm_ar: &Path) {
    command
        .env("AR_wasm32_unknown_unknown", llvm_ar)
        .env("NO_COLOR", "true");
}

pub fn spawn_web_runner(
    _: Device,
    _: String,
    tx: Sender<RunnerEvent>,
    current_child_clone: Arc<Mutex<Option<Child>>>,
    _: IpAddr,
    _: u16,
) {
    // Write [[copy]] entries into Trunk.toml so that trunk itself copies
    // registered assets into dist/ during its build. Trunk cleans dist/
    // before building, so manual pre-staging would be wiped.
    set_status(&tx, Status::Building(0));
    // if let Err(e) = crate::commands::assets::sync_trunk_copy_entries() {
    //     fail(&tx, format!("Warning: failed to sync trunk asset entries: {e}"));
    //     return;
    // }

    let artifact = "builds/web";
    let Ok(_) = copy_assets_into(artifact) else {
        println!("Failed to copy assets into {artifact}");
        return;
    };
    #[cfg(target_os = "macos")]
    let Some(llvm_ar) = find_llvm_ar() else {
        fail(&tx, "Failed to find llvm-ar".to_string());
        return;
    };

    set_status(&tx, Status::Launching);
    build_log(&tx, "Starting trunk server...");

    let mut trunk = Command::new("trunk");
    #[cfg(target_os = "macos")]
    configure_trunk(&mut trunk, &llvm_ar);
    trunk
        .arg("serve")
        .current_dir("builds/web");

    if !spawn_streamed(
        trunk,
        &tx,
        &current_child_clone,
        "Failed to run trunk serve",
        Status::Error,
        stream_stdout_as_app_log,
        stream_stderr_as_app_log,
    ) {
        return;
    }

    set_status(&tx, Status::Running);

    wait_for_child(&current_child_clone);

    set_status(&tx, Status::Idling);
}

#[cfg(test)]
mod tests {
    use crate::commands::run::web::find_llvm_ar;

    #[test]
    fn test_find_llvm_ar() {
        match find_llvm_ar() {
            Some(path) => println!("Found llvm-ar at: {}", path.display()),
            None => println!("llvm-ar not found"),
        }
    }
}
