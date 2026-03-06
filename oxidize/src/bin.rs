
#[cfg(target_os = "windows")]
fn main() {
    oxidize_cli::start_cli()
}
#[cfg(target_os = "macos")]
fn main() {
    oxidize_cli::start_cli()
}
#[cfg(target_os = "linux")]
fn main() {
    oxidize_cli::start_cli()
}