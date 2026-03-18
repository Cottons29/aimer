#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
fn main() {
    aimer_cli::start_cli()
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn main() {}
