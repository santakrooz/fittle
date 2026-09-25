//! Any byte string: parsing and explaining must never panic.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(fits) = fittle_core::Fits::from_bytes(data) {
        let _ = fittle_core::info_from(&fits, "fuzz.fits");
    }
});
