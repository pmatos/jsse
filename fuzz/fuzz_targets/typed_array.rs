#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    jsse_fuzz::typed_array::fuzz_typed_array_bytes(data);
});
