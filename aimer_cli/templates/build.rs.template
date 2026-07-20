fn main() {
    // On iOS the Swift app layer provides the frame-scheduler entry points
    // (`aimer_ios_request_frame` / `aimer_ios_pause_frames`) that the Rust code
    // calls into (see `main.swift`). Those symbols live in the final app binary,
    // so when Cargo builds the `cdylib` crate-type they are still undefined and
    // the dylib link fails with "Undefined symbols for architecture arm64".
    //
    // Tell the linker that these specific symbols may be resolved dynamically at
    // load time (`-U <symbol>`), which is the targeted, modern alternative to
    // the broad `-undefined dynamic_lookup`. This only affects the `cdylib`
    // link step; the `staticlib` that the actual app bundle links is unaffected
    // (its undefined symbols are resolved against Swift at the final app link).
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "ios" {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-U,_aimer_ios_request_frame");
        println!("cargo:rustc-cdylib-link-arg=-Wl,-U,_aimer_ios_pause_frames");
    }
}
