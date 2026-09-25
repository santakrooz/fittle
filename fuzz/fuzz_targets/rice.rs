//! Any compressed tile: the Rice decoder returns Ok or Err, never panics.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 3 {
        return;
    }
    let bytepix = [1, 2, 4][data[0] as usize % 3];
    let blocksize = [16, 32][data[1] as usize % 2];
    let n = (data[2] as usize) * 17 % 4096;
    let _ = fittle_image::rice::decompress(&data[3..], bytepix, blocksize, n);
});
