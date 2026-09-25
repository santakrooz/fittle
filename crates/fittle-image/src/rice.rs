//! Rice decompression for FITS tile-compressed images (`ZCMPTYPE = 'RICE_1'`).
//!
//! Port of the algorithm in the FITS tiled-image convention (as implemented
//! by cfitsio's `fits_rdecomp`), in safe Rust with bounds-checked input.

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
    fn truncated_input_is_an_error() {
        assert_eq!(decompress(&[0x12], 2, 32, 4), Err(RiceError::Truncated));
    }
}
