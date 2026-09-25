//! Tile-compressed images (`ZIMAGE = T` binary tables, e.g. `.fz`).
//!
//! M0 supports integer images compressed with RICE_1, which covers fpack's
//! default for capture-app subs. GZIP and quantized float tiles come later.

use fittle_core::{Hdu, Header};

use crate::decode::DecodeError;
use crate::rice;

/// One binary-table column's byte offset and variable-length descriptor kind.
struct Column {
    offset: usize,
    /// Descriptor width: 8 for `P`, 16 for `Q`.
    descriptor: usize,
}

/// Decode every tile and assemble raw stored values (before BZERO/BSCALE),
/// in FITS pixel order (axis 1 fastest).
pub(crate) fn decode(hdu: &Hdu, data: &[u8]) -> Result<Vec<i64>, DecodeError> {
    let h = hdu.header();
    let unsupported = |why: String| DecodeError::Unsupported(why);

    let cmp = h.string("ZCMPTYPE").unwrap_or("").trim().to_string();
    if cmp != "RICE_1" {
        return Err(unsupported(format!("compression {cmp:?}")));
    }
    let zbitpix = h
        .int("ZBITPIX")
        .ok_or_else(|| unsupported("missing ZBITPIX".into()))?;
    let bytepix = match zbitpix {
        8 => 1,
        16 => 2,
        32 => 4,
        b => return Err(unsupported(format!("RICE_1 with ZBITPIX {b}"))),
    };
    let blocksize = zval(h, "BLOCKSIZE").unwrap_or(32) as usize;
    let bytepix = zval(h, "BYTEPIX").map_or(bytepix, |v| v as usize);

    let shape = &hdu.shape;
    let tile: Vec<usize> = (1..=shape.len())
        .map(|n| {
            h.int(&format!("ZTILE{n}"))
                .unwrap_or(if n == 1 { shape[0] as i64 } else { 1 }) as usize
        })
        .collect();
    let dims: Vec<usize> = shape.iter().map(|&d| d as usize).collect();
    let total: usize = dims.iter().product();

    let col = find_column(h, "COMPRESSED_DATA")
        .ok_or_else(|| unsupported("no COMPRESSED_DATA column".into()))?;
    let row_len = h.int("NAXIS1").unwrap_or(0) as usize;
    let rows = h.int("NAXIS2").unwrap_or(0) as usize;
    let heap_start = h.int("THEAP").map_or(row_len * rows, |v| v as usize);

    // Tiles per axis, first axis fastest.
    let ntiles: Vec<usize> = dims
        .iter()
        .zip(&tile)
        .map(|(d, t)| d.div_ceil(*t))
        .collect();
    if ntiles.iter().product::<usize>() != rows {
        return Err(unsupported("tile count does not match table rows".into()));
    }

    let mut out = vec![0i64; total];
    let mut tile_idx = vec![0usize; dims.len()];
    for row in 0..rows {
        let at = row * row_len + col.offset;
        let desc = data
            .get(at..at + col.descriptor)
            .ok_or(DecodeError::Truncated)?;
        let (count, offset) = if col.descriptor == 8 {
            (be_u(&desc[..4]), be_u(&desc[4..8]))
        } else {
            (be_u(&desc[..8]), be_u(&desc[8..16]))
        };
        let start = heap_start + offset;
        let bytes = data
            .get(start..start + count)
            .ok_or(DecodeError::Truncated)?;

        // This tile's extent along each axis.
        let lo: Vec<usize> = tile_idx.iter().zip(&tile).map(|(i, t)| i * t).collect();
        let len: Vec<usize> = lo
            .iter()
            .zip(&tile)
            .zip(&dims)
            .map(|((l, t), d)| (*t).min(d - l))
            .collect();
        let n: usize = len.iter().product();

        let raw = rice::decompress(bytes, bytepix, blocksize, n)
            .map_err(|e| unsupported(e.to_string()))?;
        scatter(&raw, zbitpix, &lo, &len, &dims, &mut out);

        // Advance the tile index, first axis fastest.
        for (i, t) in tile_idx.iter_mut().enumerate() {
            *t += 1;
            if *t < ntiles[i] {
                break;
            }
            *t = 0;
        }
    }
    Ok(out)
}

/// Copy a decoded tile into the full image.
fn scatter(
    raw: &[u32],
    zbitpix: i64,
    lo: &[usize],
    len: &[usize],
    dims: &[usize],
    out: &mut [i64],
) {
    let signed = |v: u32| -> i64 {
        match zbitpix {
            8 => v as u8 as i64,
            16 => v as u16 as i16 as i64,
            _ => v as i32 as i64,
        }
    };
    let mut pos = vec![0usize; len.len()];
    for &v in raw {
        let mut index = 0;
        let mut stride = 1;
        for a in 0..dims.len() {
            index += (lo[a] + pos[a]) * stride;
            stride *= dims[a];
        }
        out[index] = signed(v);
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

/// Locate a column by `TTYPEn`, computing its byte offset from the
/// preceding `TFORMn` widths.
fn find_column(h: &Header, name: &str) -> Option<Column> {
    let fields = h.int("TFIELDS")? as usize;
    let mut offset = 0;
    for n in 1..=fields {
        let form = h.string(&format!("TFORM{n}"))?.trim().to_string();
        let (width, descriptor) = tform_width(&form)?;
        if h.string(&format!("TTYPE{n}")).map(str::trim) == Some(name) {
            return descriptor.map(|d| Column {
                offset,
                descriptor: d,
            });
        }
        offset += width;
    }
    None
}

/// Byte width of a TFORM, and the descriptor size if it is a `P`/`Q` array.
fn tform_width(form: &str) -> Option<(usize, Option<usize>)> {
    let digits = form.bytes().take_while(u8::is_ascii_digit).count();
    let repeat: usize = if digits == 0 {
        1
    } else {
        form[..digits].parse().ok()?
    };
    let code = form[digits..].chars().next()?;
    let each = match code {
        'P' => return Some((8 * repeat, Some(8))),
        'Q' => return Some((16 * repeat, Some(16))),
        'X' => return Some((repeat.div_ceil(8), None)),
        'L' | 'B' | 'A' => 1,
        'I' => 2,
        'J' | 'E' => 4,
        'K' | 'D' | 'C' => 8,
        'M' => 16,
        _ => return None,
    };
    Some((each * repeat, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tforms() {
        assert_eq!(tform_width("1PB(95)"), Some((8, Some(8))));
        assert_eq!(tform_width("1QB"), Some((16, Some(16))));
        assert_eq!(tform_width("3E"), Some((12, None)));
        assert_eq!(tform_width("J"), Some((4, None)));
        assert_eq!(tform_width("12X"), Some((2, None)));
    }
}
