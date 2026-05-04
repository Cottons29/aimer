fn main() {
    let port = std::env::var("DEFAULT_INSPECTOR_PORT").unwrap_or_else(|err| {
        eprintln!("Failed to read DEFAULT_INSPECTOR_PORT: {}", err);
        "9229".to_string()
    });
    let address = std::env::var("DEFAULT_INSPECTOR_ADDRESS").unwrap_or_else(|err| {
        eprintln!("Failed to read DEFAULT_INSPECTOR_ADDRESS: {}", err);
        "127.0.0.1".to_string()
    });
    println!("cargo:rustc-env=DEFAULT_INSPECTOR_PORT={}", port);
    println!("cargo:rustc-env=DEFAULT_INSPECTOR_ADDRESS={}", address);
    println!("export DEFAULT_INSPECTOR_PORT={}", port);
    println!("export DEFAULT_INSPECTOR_ADDRESS={}", address);
}
