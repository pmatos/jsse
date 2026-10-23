#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    jsse_fuzz::gc_stress::fuzz_gc_stress_bytes(data);
});
