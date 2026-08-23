#![no_main]

use aimer_anteros_fuzz::fuzz_envelope;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| fuzz_envelope(data));