fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "ios" {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-U,_aimer_ios_request_frame");
        println!("cargo:rustc-cdylib-link-arg=-Wl,-U,_aimer_ios_pause_frames");
    }
}
