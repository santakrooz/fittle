//! Any keyword/value/comment text: formatting yields 80-char records that
//! parse back to the same value.
#![no_main]
use fittle_core::edit::{NewValue, format_card};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else { return };
    if !s.bytes().all(|b| (0x20..=0x7E).contains(&b)) {
        return;
    }
    let (value, comment) = s.split_once('|').unwrap_or((s, ""));
    let v = NewValue::String(value.to_string());
    let recs = format_card("FUZZKEY", &v, Some(comment));
    assert!(recs.iter().all(|r| r.len() == 80));
    let mut block: Vec<u8> = b"SIMPLE  =                    T".iter().copied().chain(std::iter::repeat_n(b' ', 50)).collect();
    block.extend(b"BITPIX  =                    8".iter().copied().chain(std::iter::repeat_n(b' ', 50)));
    block.extend(b"NAXIS   =                    0".iter().copied().chain(std::iter::repeat_n(b' ', 50)));
    for r in &recs {
        block.extend(r.as_bytes());
    }
    block.extend(b"END".iter().copied().chain(std::iter::repeat_n(b' ', 77)));
    block.resize(block.len().div_ceil(2880) * 2880, b' ');
    let fits = fittle_core::Fits::from_bytes(&block).expect("valid FITS");
    assert_eq!(fits.hdus[0].header().string("FUZZKEY"), Some(value.trim_end()));
});
