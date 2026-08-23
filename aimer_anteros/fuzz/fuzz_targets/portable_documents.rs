#![no_main]

use aimer_anteros_fuzz::fuzz_portable_document;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| fuzz_portable_document(data));