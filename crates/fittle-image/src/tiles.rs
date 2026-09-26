//! Tile-compressed images (`ZIMAGE = T` binary tables, e.g. `.fz`).
//!
//! Decodes RICE_1, GZIP_1 and GZIP_2 tiles, including quantized floating
//! point (`ZQUANTIZ`: no dither, subtractive dither 1 and 2) with per-tile
//! `ZSCALE`/`ZZERO` and the lossless `GZIP_COMPRESSED_DATA` fallback that
//! cfitsio writes for tiles it cannot quantize. Follows the FITS tiled-image
//! convention (FITS standard §10) and cfitsio's behaviour.

use std::io::Read;
use std::sync::OnceLock;

use fittle_core::{Hdu, Header};

use crate::decode::DecodeError;
use crate::rice;

/// Stored pixel values of a decoded image, before BZERO/BSCALE.
#[derive(Debug, Clone, PartialEq)]
pub enum Samples {
    Int(Vec<i64>),
    Float(Vec<f64>),
}

/// A decompressed image: `bitpix` is the uncompressed type (ZBITPIX).
#[derive(Debug, Clone, PartialEq)]
pub struct TileImage {
    pub bitpix: i64,
    pub dims: Vec<usize>,
    pub samples: Samples,
}

impl TileImage {
    /// The data unit as it is stored uncompressed (big-endian, unpadded).
    pub fn to_be_bytes(&self) -> Vec<u8> {
        match (&self.samples, self.bitpix) {
            (Samples::Int(v), 8) => v.iter().map(|&x| x as u8).collect(),
            (Samples::Int(v), 16) => v.iter().flat_map(|&x| (x as i16).to_be_bytes()).collect(),
            (Samples::Int(v), 32) => v.iter().flat_map(|&x| (x as i32).to_be_bytes()).collect(),
            (Samples::Int(v), _) => v.iter().flat_map(|&x| x.to_be_bytes()).collect(),
            (Samples::Float(v), -64) => v.iter().flat_map(|&x| x.to_be_bytes()).collect(),
            (Samples::Float(v), _) => v.iter().flat_map(|&x| (x as f32).to_be_bytes()).collect(),
        }
    }
}

/// Integer standing for a null pixel in quantized tiles (cfitsio NULL_VALUE).
const NULL_VALUE: i64 = -2147483647;
/// Integer standing for an exact 0.0 under SUBTRACTIVE_DITHER_2.
const ZERO_VALUE: i64 = -2147483646;
const N_RANDOM: usize = 10000;

/// cfitsio's dither table (`fits_init_randoms`): Park–Miller, seed 1.
fn randoms() -> &'static [f32] {
    static R: OnceLock<Vec<f32>> = OnceLock::new();
    R.get_or_init(|| {
        let (a, m) = (16807.0f64, 2147483647.0f64);
        let mut seed = 1.0f64;
        (0..N_RANDOM)
            .map(|_| {
                let temp = a * seed;
                seed = temp - m * ((temp / m) as i64 as f64);
                (seed / m) as f32
            })
            .collect()
    })
}

#[derive(Clone, Copy, PartialEq)]
enum Method {
    Rice,
    Gzip1,
    Gzip2,
}

#[derive(Clone, Copy, PartialEq)]
enum Quantize {
    None,
    NoDither,
    Dither1,
    Dither2,
}

/// One binary-table column: byte offset in the row and its TFORM code.
#[derive(Clone, Copy)]
struct Column {
    offset: usize,
    code: char,
}

struct Table<'a> {
    data: &'a [u8],
    row_len: usize,
    heap_start: usize,
}

impl Table<'_> {
    fn cell(&self, row: usize, col: Column, len: usize) -> Result<&[u8], DecodeError> {
        let at = row * self.row_len + col.offset;
        self.data.get(at..at + len).ok_or(DecodeError::Truncated)
    }

    /// Bytes of a variable-length array cell (`P` or `Q` descriptor).
    fn array(&self, row: usize, col: Column) -> Result<&[u8], DecodeError> {
        let (count, offset) = if col.code == 'Q' {
            let d = self.cell(row, col, 16)?;
            (be_u(&d[..8]), be_u(&d[8..16]))
        } else {
            let d = self.cell(row, col, 8)?;
            (be_u(&d[..4]), be_u(&d[4..8]))
        };
        let start = self.heap_start + offset;
        self.data
            .get(start..start + count)
            .ok_or(DecodeError::Truncated)
    }

    fn f64(&self, row: usize, col: Column) -> Result<f64, DecodeError> {
        Ok(match col.code {
            'E' => f32::from_be_bytes(self.cell(row, col, 4)?.try_into().unwrap()) as f64,
            'J' => i32::from_be_bytes(self.cell(row, col, 4)?.try_into().unwrap()) as f64,
            'K' => i64::from_be_bytes(self.cell(row, col, 8)?.try_into().unwrap()) as f64,
            _ => f64::from_be_bytes(self.cell(row, col, 8)?.try_into().unwrap()),
        })
    }
}

/// Decode every tile and assemble the image in FITS pixel order (axis 1
/// fastest), as stored values (before BZERO/BSCALE).
pub fn decode(hdu: &Hdu, data: &[u8]) -> Result<TileImage, DecodeError> {
    let h = hdu.header();
    let unsupported = |why: String| DecodeError::Unsupported(why);

    let cmp = h.string("ZCMPTYPE").unwrap_or("").trim().to_string();
    let method = match cmp.as_str() {
        "RICE_1" | "RICE_ONE" => Method::Rice,
        "GZIP_1" => Method::Gzip1,
        "GZIP_2" => Method::Gzip2,
        other => return Err(unsupported(format!("compression {other:?}"))),
    };
    let zbitpix = h
        .int("ZBITPIX")
        .ok_or_else(|| unsupported("missing ZBITPIX".into()))?;
    if ![8, 16, 32, 64, -32, -64].contains(&zbitpix) {
        return Err(unsupported(format!("ZBITPIX {zbitpix}")));
    }
    let float = zbitpix < 0;

    let columns = columns(h).ok_or_else(|| unsupported("unreadable table columns".into()))?;
    let col = |name: &str| columns.iter().find(|(n, _)| n == name).map(|(_, c)| *c);
    let compressed =
        col("COMPRESSED_DATA").ok_or_else(|| unsupported("no COMPRESSED_DATA column".into()))?;
    let gzip_fallback = col("GZIP_COMPRESSED_DATA");
    let (zscale, zzero, zblank_col) = (col("ZSCALE"), col("ZZERO"), col("ZBLANK"));
    let quantize = match h.string("ZQUANTIZ").map(str::trim) {
        // Only floats with per-tile scaling are quantized.
        _ if !float || zscale.is_none() => Quantize::None,
        Some("SUBTRACTIVE_DITHER_1") => Quantize::Dither1,
        Some("SUBTRACTIVE_DITHER_2") => Quantize::Dither2,
        Some("NO_DITHER") | None => Quantize::NoDither,
        Some(q) => return Err(unsupported(format!("ZQUANTIZ {q:?}"))),
    };
    let zblank_key = h.int("ZBLANK");
    let dither0 = h.int("ZDITHER0").unwrap_or(1);

    // Pixel width inside a tile: quantized floats are 32-bit integers.
    let quantized = quantize != Quantize::None;
    let natural = (zbitpix.unsigned_abs() / 8) as usize;
    let bytepix = match method {
        Method::Rice => {
            zval(h, "BYTEPIX").map_or(if quantized { 4 } else { natural }, |v| v as usize)
        }
        _ if quantized => 4,
        _ => natural,
    };
    if float && !quantized && method == Method::Rice {
        return Err(unsupported("RICE_1 floats without quantization".into()));
    }
    let blocksize = zval(h, "BLOCKSIZE").unwrap_or(32) as usize;

    let dims: Vec<usize> = hdu.shape.iter().map(|&d| d as usize).collect();
    let tile: Vec<usize> = (1..=dims.len())
        .map(|n| {
            h.int(&format!("ZTILE{n}"))
                .unwrap_or(if n == 1 { dims[0] as i64 } else { 1 })
                .max(1) as usize
        })
        .collect();
    let total: usize = dims.iter().product();
    let rows = h.int("NAXIS2").unwrap_or(0) as usize;
    let row_len = h.int("NAXIS1").unwrap_or(0) as usize;
    let table = Table {
        data,
        row_len,
        heap_start: h.int("THEAP").map_or(row_len * rows, |v| v as usize),
    };
    let ntiles: Vec<usize> = dims
        .iter()
        .zip(&tile)
        .map(|(d, t)| d.div_ceil(*t))
        .collect();
    if ntiles.iter().product::<usize>() != rows {
        return Err(unsupported("tile count does not match table rows".into()));
    }

    let mut ints = if float { Vec::new() } else { vec![0i64; total] };
    let mut floats = if float { vec![0f64; total] } else { Vec::new() };
    let mut tile_idx = vec![0usize; dims.len()];
    for row in 0..rows {
        let lo: Vec<usize> = tile_idx.iter().zip(&tile).map(|(i, t)| i * t).collect();
        let len: Vec<usize> = lo
            .iter()
            .zip(&tile)
            .zip(&dims)
            .map(|((l, t), d)| (*t).min(d - l))
            .collect();
        let n: usize = len.iter().product();
        advance(&mut tile_idx, &ntiles);

        let bytes = table.array(row, compressed)?;
        if float && (bytes.is_empty() || !quantized) {
            // Lossless floats: this tile's raw values, gzipped (the
            // GZIP_COMPRESSED_DATA fallback, or an unquantized GZIP image).
            let (src, shuffled) = if bytes.is_empty() {
                let g = gzip_fallback
                    .ok_or_else(|| unsupported("empty tile without GZIP_COMPRESSED_DATA".into()))?;
                (table.array(row, g)?, false)
            } else {
                (bytes, method == Method::Gzip2)
            };
            let mut raw = gunzip(src, natural * n)?;
            if shuffled {
                raw = unshuffle(&raw, natural);
            }
            scatter(&floats_from(&raw, natural), &lo, &len, &dims, &mut floats);
            continue;
        }
        let stored: Vec<i64> = match method {
            Method::Rice => rice::decompress(bytes, bytepix, blocksize, n)
                .map_err(|e| unsupported(e.to_string()))?
                .into_iter()
                .map(|v| signed(v as u64, bytepix))
                .collect(),
            Method::Gzip1 | Method::Gzip2 => {
                let mut raw = gunzip(bytes, bytepix * n)?;
                if method == Method::Gzip2 {
                    raw = unshuffle(&raw, bytepix);
                }
                raw.chunks_exact(bytepix)
                    .map(|c| signed(c.iter().fold(0u64, |a, &b| (a << 8) | b as u64), bytepix))
                    .collect()
            }
        };
        if stored.len() != n {
            return Err(DecodeError::Truncated);
        }
        if !float {
            scatter(&stored, &lo, &len, &dims, &mut ints);
            continue;
        }
        let scale = zscale
            .map(|c| table.f64(row, c))
            .transpose()?
            .unwrap_or(1.0);
        let zero = zzero.map(|c| table.f64(row, c)).transpose()?.unwrap_or(0.0);
        let blank = match zblank_col {
            Some(c) => table.f64(row, c)? as i64,
            None => zblank_key.unwrap_or(NULL_VALUE),
        };
        let vals = unquantize(&stored, quantize, [scale, zero], blank, row, dither0);
        scatter(&vals, &lo, &len, &dims, &mut floats);
    }
    Ok(TileImage {
        bitpix: zbitpix,
        dims,
        samples: if float {
            Samples::Float(floats)
        } else {
            Samples::Int(ints)
        },
    })
}

fn advance(tile_idx: &mut [usize], ntiles: &[usize]) {
    for (i, t) in tile_idx.iter_mut().enumerate() {
        *t += 1;
        if *t < ntiles[i] {
            return;
        }
        *t = 0;
    }
}

fn signed(v: u64, bytepix: usize) -> i64 {
    match bytepix {
        1 => v as u8 as i64,
        2 => v as u16 as i16 as i64,
        4 => v as u32 as i32 as i64,
        _ => v as i64,
    }
}

/// Quantized integers back to floats (cfitsio `unquantize_*`); `row` is the
/// 0-based tile number.
fn unquantize(
    q: &[i64],
    method: Quantize,
    [scale, zero]: [f64; 2],
    blank: i64,
    row: usize,
    dither0: i64,
) -> Vec<f64> {
    let r = randoms();
    let mut iseed = (row as i64 + dither0 - 1).rem_euclid(N_RANDOM as i64) as usize;
    let mut next = (r[iseed] * 500.0) as usize;
    q.iter()
        .map(|&v| {
            let out = if v == blank {
                f64::NAN
            } else if method == Quantize::Dither2 && v == ZERO_VALUE {
                0.0
            } else if method == Quantize::NoDither {
                v as f64 * scale + zero
            } else {
                (v as f64 - r[next] as f64 + 0.5) * scale + zero
            };
            if method != Quantize::NoDither {
                next += 1;
                if next == N_RANDOM {
                    iseed = (iseed + 1) % N_RANDOM;
                    next = (r[iseed] * 500.0) as usize;
                }
            }
            out
        })
        .collect()
}

fn gunzip(bytes: &[u8], expect: usize) -> Result<Vec<u8>, DecodeError> {
    let mut out = Vec::with_capacity(expect);
    flate2::read::MultiGzDecoder::new(bytes)
        .read_to_end(&mut out)
        .map_err(|e| DecodeError::Unsupported(format!("gzip tile: {e}")))?;
    if out.len() < expect {
        return Err(DecodeError::Truncated);
    }
    out.truncate(expect);
    Ok(out)
}

/// GZIP_2 stores byte k of every pixel together (most significant first).
fn unshuffle(raw: &[u8], width: usize) -> Vec<u8> {
    let n = raw.len() / width;
    let mut out = vec![0u8; raw.len()];
    for k in 0..width {
        for i in 0..n {
            out[i * width + k] = raw[k * n + i];
        }
    }
    out
}

/// Inverse of `unshuffle`, for encoding.
pub(crate) fn shuffle(raw: &[u8], width: usize) -> Vec<u8> {
    let n = raw.len() / width;
    let mut out = vec![0u8; raw.len()];
    for k in 0..width {
        for i in 0..n {
            out[k * n + i] = raw[i * width + k];
        }
    }
    out
}

fn floats_from(raw: &[u8], width: usize) -> Vec<f64> {
    if width == 8 {
        raw.chunks_exact(8)
            .map(|c| f64::from_be_bytes(c.try_into().unwrap()))
            .collect()
    } else {
        raw.chunks_exact(4)
            .map(|c| f32::from_be_bytes(c.try_into().unwrap()) as f64)
            .collect()
    }
}

/// Copy a decoded tile into the full image.
fn scatter<T: Copy>(tile: &[T], lo: &[usize], len: &[usize], dims: &[usize], out: &mut [T]) {
    let mut pos = vec![0usize; len.len()];
    for &v in tile {
        let mut index = 0;
        let mut stride = 1;
        for a in 0..dims.len() {
            index += (lo[a] + pos[a]) * stride;
            stride *= dims[a];
        }
        out[index] = v;
        for a in 0..len.len() {
            pos[a] += 1;
            if pos[a] < len[a] {
                break;
            }
            pos[a] = 0;
        }
    }
}

fn zval(h: &Header, name: &str) -> Option<i64> {
    (1..=16).find_map(|i| {
        let key = h.string(&format!("ZNAME{i}"))?;
        (key.trim() == name)
            .then(|| h.int(&format!("ZVAL{i}")))
            .flatten()
    })
}

fn be_u(b: &[u8]) -> usize {
    b.iter().fold(0usize, |acc, &x| (acc << 8) | x as usize)
}

/// Every column's name, byte offset and TFORM code.
fn columns(h: &Header) -> Option<Vec<(String, Column)>> {
    let fields = h.int("TFIELDS")? as usize;
    let mut offset = 0;
    let mut out = Vec::with_capacity(fields);
    for n in 1..=fields {
        let form = h.string(&format!("TFORM{n}"))?.trim().to_string();
        let (width, code) = tform_width(&form)?;
        let name = h
            .string(&format!("TTYPE{n}"))
            .unwrap_or("")
            .trim()
            .to_string();
        out.push((name, Column { offset, code }));
        offset += width;
    }
    Some(out)
}

/// Byte width of a TFORM and its type code (`P`/`Q` for array descriptors).
fn tform_width(form: &str) -> Option<(usize, char)> {
    let digits = form.bytes().take_while(u8::is_ascii_digit).count();
    let repeat: usize = if digits == 0 {
        1
    } else {
        form[..digits].parse().ok()?
    };
    let code = form[digits..].chars().next()?;
    let each = match code {
        'P' => 8,
        'Q' => 16,
        'X' => return Some((repeat.div_ceil(8), code)),
        'L' | 'B' | 'A' => 1,
        'I' => 2,
        'J' | 'E' => 4,
        'K' | 'D' | 'C' => 8,
        'M' => 16,
        _ => return None,
    };
    Some((each * repeat, code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tforms() {
        assert_eq!(tform_width("1PB(95)"), Some((8, 'P')));
        assert_eq!(tform_width("1QB"), Some((16, 'Q')));
        assert_eq!(tform_width("3E"), Some((12, 'E')));
        assert_eq!(tform_width("J"), Some((4, 'J')));
        assert_eq!(tform_width("12X"), Some((2, 'X')));
    }

    #[test]
    fn dither_table_matches_cfitsio() {
        // cfitsio checks that its table ends on this seed.
        let r = randoms();
        assert_eq!(r.len(), N_RANDOM);
        assert!((r[N_RANDOM - 1] as f64 - 1043618065.0 / 2147483647.0).abs() < 1e-7);
    }

    #[test]
    fn shuffle_round_trip() {
        let raw: Vec<u8> = (0..24).collect();
        assert_eq!(unshuffle(&shuffle(&raw, 4), 4), raw);
        assert_eq!(shuffle(&[1, 2, 3, 4], 2), [1, 3, 2, 4]);
    }
}
