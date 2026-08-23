//! Physical-iOS feasibility entry point for Aimer's interpreted WASM runtime.

use aimer_anteros::{Runtime, RuntimeConfig, RuntimeErrorKind};

const ANSWER_MODULE: &[u8] = &[
    0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7F,
    0x03, 0x02, 0x01, 0x00, 0x07, 0x0A, 0x01, 0x06, 0x61, 0x6E, 0x73, 0x77, 0x65, 0x72, 0x00,
    0x00, 0x0A, 0x06, 0x01, 0x04, 0x00, 0x41, 0x2A, 0x0B,
];
const INFINITE_LOOP_MODULE: &[u8] = &[
    0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7F,
    0x03, 0x02, 0x01, 0x00, 0x07, 0x08, 0x01, 0x04, 0x73, 0x70, 0x69, 0x6E, 0x00, 0x00, 0x0A,
    0x0B, 0x01, 0x09, 0x00, 0x03, 0x40, 0x0C, 0x00, 0x0B, 0x41, 0x00, 0x0B,
];

/// Runs deterministic execution and fuel-exhaustion checks on an iOS device.
///
/// A return value of zero means both checks passed. Non-zero values identify
/// the first failed stage so the Swift shell can report a stable result without
/// interpreting Rust errors across the FFI boundary.
#[unsafe(no_mangle)]
pub extern "C" fn aimer_wasm_device_proof() -> u32 {
    let runtime = Runtime::new(runtime_config(1_000));
    match runtime.invoke_i32(ANSWER_MODULE, "answer") {
        Ok(42) => {}
        Ok(_) => return 1,
        Err(_) => return 2,
    }

    let runtime = Runtime::new(runtime_config(100));
    match runtime.invoke_i32(INFINITE_LOOP_MODULE, "spin") {
        Err(error) if error.kind() == RuntimeErrorKind::FuelExhausted => 0,
        Err(_) => 3,
        Ok(_) => 4,
    }
}

fn runtime_config(fuel_per_call: u64) -> RuntimeConfig {
    RuntimeConfig::new()
        .fuel_per_call(fuel_per_call)
        .max_module_bytes(1_024)
        .max_memory_pages(1)
        .max_table_elements(1)
        .max_call_depth(64)
}