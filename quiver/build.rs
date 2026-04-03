fn main() {
    let port = match std::env::var("DEFAULT_INSPECTOR_PORT") {
        Ok(port) => port,
        Err(err) => {
            eprintln!("Failed to read DEFAULT_INSPECTOR_PORT: {}", err);
            std::process::exit(1);
        }
    };
    let address = match std::env::var("DEFAULT_INSPECTOR_ADDRESS") {
        Ok(addr) => addr,
        Err(err) => {
            eprintln!("Failed to read DEFAULT_INSPECTOR_ADDRESS: {}", err);
            std::process::exit(1);
        }
    };
    println!("cargo:rustc-env=DEFAULT_INSPECTOR_PORT={}", port);
    println!("cargo:rustc-env=DEFAULT_INSPECTOR_ADDRESS={}", address);
    println!("export DEFAULT_INSPECTOR_PORT={}", port);
    println!("export DEFAULT_INSPECTOR_ADDRESS={}", address);
}
