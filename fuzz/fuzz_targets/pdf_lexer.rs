#![no_main]

use libfuzzer_sys::fuzz_target;

// Placeholder target: implemented in the fuzz-targets subphase.
fuzz_target!(|data: &[u8]| {
    let _ = data;
});
