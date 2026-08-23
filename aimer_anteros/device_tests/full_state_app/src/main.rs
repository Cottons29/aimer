#[cfg(not(target_arch = "wasm32"))]
fn main() {
    aimer_full_state_app::__generated_entrance_point();
}

#[cfg(target_arch = "wasm32")]
fn main() {
    unreachable!("the native proof shell is not built for WebAssembly");
}
