//! Rice coding for FITS tile-compressed images (`ZCMPTYPE = 'RICE_1'`).
//!
//! Port of the algorithm in the FITS tiled-image convention (as implemented
//! by cfitsio's `fits_rcomp` / `fits_rdecomp`), in safe Rust with
//! bounds-checked input.

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RiceError {
    #[error("compressed tile ended early")]
    Truncated,
    #[error("unsupported BYTEPIX {0}")]
    BytePix(usize),
}

struct Bytes<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Bytes<'_> {
    fn next(&mut self) -> Result<u64, RiceError> {
        let b = *self.data.get(self.pos).ok_or(RiceError::Truncated)?;
        self.pos += 1;
        Ok(b as u64)
    }
}

/// Decode `n` pixels of `bytepix` bytes each. Returns raw unsigned bit
/// patterns (callers reinterpret as i8/i16/i32 per ZBITPIX).
pub fn decompress(
    input: &[u8],
    bytepix: usize,
    blocksize: usize,
    n: usize,
) -> Result<Vec<u32>, RiceError> {
    let (fsbits, fsmax): (i32, i64) = match bytepix {
        1 => (3, 6),
        2 => (4, 14),
        4 => (5, 25),
        b => return Err(RiceError::BytePix(b)),
    };
    let bbits: i32 = 1 << fsbits;
    let mask: u64 = if bytepix == 4 {
        u32::MAX as u64
    } else {
        (1u64 << (8 * bytepix)) - 1
    };
    let mut c = Bytes {
        data: input,
        pos: 0,
    };

    // First pixel is stored verbatim, big-endian.
    let mut lastpix: u64 = 0;
    for _ in 0..bytepix {
        lastpix = (lastpix << 8) | c.next()?;
    }

    let mut out = Vec::with_capacity(n);
    let mut b: u64 = c.next()?;
    let mut nbits: i32 = 8;
    let mut i = 0;
    while i < n {
        nbits -= fsbits;
        while nbits < 0 {
            b = (b << 8) | c.next()?;
            nbits += 8;
        }
        let fs = (b >> nbits) as i64 - 1;
        b &= (1u64 << nbits) - 1;
        let imax = (i + blocksize).min(n);

        if fs < 0 {
            // Low-entropy block: every difference is zero.
            out.extend(std::iter::repeat_n(lastpix as u32, imax - i));
            i = imax;
        } else if fs == fsmax {
            // High-entropy block: differences stored as raw bbits-bit values.
            while i < imax {
                let mut k = bbits - nbits;
                let mut diff = b << k;
                k -= 8;
                while k >= 0 {
                    b = c.next()?;
                    diff |= b << k;
                    k -= 8;
                }
                if nbits > 0 {
                    b = c.next()?;
                    diff |= b >> (-k);
                    b &= (1u64 << nbits) - 1;
                } else {
                    b = 0;
                }
                lastpix = unmap(diff, lastpix, mask);
                out.push(lastpix as u32);
                i += 1;
            }
        } else {
            let fs = fs as i32;
            while i < imax {
                while b == 0 {
                    nbits += 8;
                    b = c.next()?;
                }
                // Count leading zeros within the nbits window.
                let width = 64 - b.leading_zeros() as i32;
                let nzero = nbits - width;
                nbits -= nzero + 1;
                b ^= 1u64 << nbits;
                nbits -= fs;
                while nbits < 0 {
                    b = (b << 8) | c.next()?;
                    nbits += 8;
                }
                let diff = ((nzero as u64) << fs) | (b >> nbits);
                b &= (1u64 << nbits) - 1;
                lastpix = unmap(diff, lastpix, mask);
                out.push(lastpix as u32);
                i += 1;
            }
        }
    }
    Ok(out)
}

/// Accumulates bits most-significant first.
struct Bits {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    /// Append the low `bits` (≤ 32) bits of `value`.
    fn put(&mut self, value: u64, bits: u32) {
        self.acc = (self.acc << bits) | (value & ((1u64 << bits) - 1));
        self.n += bits;
        while self.n >= 8 {
            self.n -= 8;
            self.out.push((self.acc >> self.n) as u8);
        }
        self.acc &= (1u64 << self.n) - 1;
    }

    fn zeros(&mut self, mut count: u64) {
        while count > 0 {
            let k = count.min(32) as u32;
            self.put(0, k);
            count -= k as u64;
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push((self.acc << (8 - self.n)) as u8);
        }
        self.out
    }
}

/// Encode pixels (raw bit patterns of `bytepix` bytes) in blocks of
/// `blocksize`. Differences wrap to the pixel width, so any conforming
/// decoder (cfitsio, astropy, Fittle) restores the exact values.
pub fn compress(values: &[u32], bytepix: usize, blocksize: usize) -> Result<Vec<u8>, RiceError> {
    let (fsbits, fsmax): (u32, u32) = match bytepix {
        1 => (3, 6),
        2 => (4, 14),
        4 => (5, 25),
        b => return Err(RiceError::BytePix(b)),
    };
    let bbits = 8 * bytepix as u32;
    let mut w = Bits {
        out: Vec::with_capacity(values.len() * bytepix / 2 + 16),
        acc: 0,
        n: 0,
    };
    let Some(&first) = values.first() else {
        return Ok(Vec::new());
    };
    w.put(first as u64, bbits);
    let sign = |v: u32| -> i64 {
        match bytepix {
            1 => v as u8 as i8 as i64,
            2 => v as u16 as i16 as i64,
            _ => v as i32 as i64,
        }
    };
    let mut last = first;
    let mut diff = vec![0u64; blocksize.max(1)];
    for block in values.chunks(blocksize.max(1)) {
        let mut sum = 0f64;
        for (d, &v) in diff.iter_mut().zip(block) {
            // Difference wrapped to the pixel width, then zig-zag mapped.
            let pd = sign(v.wrapping_sub(last));
            *d = (if pd < 0 { !(pd << 1) } else { pd << 1 }) as u64 & ((1u64 << bbits) - 1);
            sum += *d as f64;
            last = v;
        }
        let n = block.len();
        let dpsum = ((sum - (n / 2) as f64 - 1.0) / n as f64).max(0.0);
        let mut psum = (dpsum as u64) >> 1;
        let mut fs = 0u32;
        while psum > 0 {
            psum >>= 1;
            fs += 1;
        }
        if fs >= fsmax {
            // High entropy: raw differences.
            w.put((fsmax + 1) as u64, fsbits);
            for &d in &diff[..n] {
                w.put(d, bbits);
            }
        } else if fs == 0 && sum == 0.0 {
            // Low entropy: every difference is zero.
            w.put(0, fsbits);
        } else {
            w.put((fs + 1) as u64, fsbits);
            for &d in &diff[..n] {
                w.zeros(d >> fs);
                w.put(1, 1);
                if fs > 0 {
                    w.put(d, fs);
                }
            }
        }
    }
    Ok(w.finish())
}

/// Undo the zig-zag mapping and differencing, wrapping to the pixel width.
fn unmap(diff: u64, lastpix: u64, mask: u64) -> u64 {
    let d = if diff & 1 == 0 {
        diff >> 1
    } else {
        !(diff >> 1)
    };
    d.wrapping_add(lastpix) & mask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_entropy_block_repeats_first_pixel() {
        // First pixel 0x1234, then fs code 0 (=> fs = -1) for a 4-pixel block.
        let data = [0x12, 0x34, 0x00, 0x00];
        assert_eq!(decompress(&data, 2, 32, 4).unwrap(), [0x1234; 4]);
    }

    #[test]
    fn compress_round_trips_every_width() {
        let mut seed = 12345u64;
        let mut rnd = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as u32
        };
        for bytepix in [1usize, 2, 4] {
            let mask = if bytepix == 4 {
                u32::MAX
            } else {
                (1u32 << (8 * bytepix)) - 1
            };
            let cases: Vec<Vec<u32>> = vec![
                vec![7; 100],                                                 // low entropy
                (0..300).map(|_| rnd() & mask).collect(),                     // high entropy
                (0..257).map(|i| (1000 + (rnd() % 40) + i) & mask).collect(), // smooth
                vec![mask, 0, mask, 0, 1, mask - 1], // wrap-around extremes
                vec![42],
            ];
            for v in cases {
                let c = compress(&v, bytepix, 32).unwrap();
                assert_eq!(
                    decompress(&c, bytepix, 32, v.len()).unwrap(),
                    v,
                    "bytepix {bytepix}"
                );
            }
        }
    }

    #[test]
    fn truncated_input_is_an_error() {
        assert_eq!(decompress(&[0x12], 2, 32, 4), Err(RiceError::Truncated));
    }
}
