//! `fpack` / `funpack`: lossless tile compression of image HDUs (FITS
//! tiled-image convention, as written by cfitsio's fpack) and its inverse.
//!
//! Integers use RICE_1 (fpack's default), floats and 64-bit integers use
//! GZIP_2. Floats are never quantized, so every pixel survives exactly.
//! Outputs are new files: written beside the target, verified by decoding
//! them again, then moved into place without replacing anything.

use std::io::Write;
use std::path::{Path, PathBuf};

use fittle_core::checksum;
use fittle_core::{Fits, Hdu, HduKind, Header, Value};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::decode::DecodeError;
use crate::rice;
use crate::tiles;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    Rice,
    Gzip1,
    Gzip2,
}

impl Method {
    fn keyword(self) -> &'static str {
        match self {
            Method::Rice => "RICE_1",
            Method::Gzip1 => "GZIP_1",
            Method::Gzip2 => "GZIP_2",
        }
    }

    /// fpack's lossless choice for a pixel type.
    pub fn auto(bitpix: i64) -> Method {
        if matches!(bitpix, 8 | 16 | 32) {
            Method::Rice
        } else {
            Method::Gzip2
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PackOptions {
    /// Force a method (RICE_1 needs 8/16/32-bit integers). Default: automatic.
    pub method: Option<Method>,
    /// Image rows per tile (default 1, fpack's default).
    pub tile_rows: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HduReport {
    pub index: usize,
    /// `compressed`, `decompressed` or `copied`.
    pub action: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    pub bytes_before: u64,
    pub bytes_after: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackReport {
    pub source: String,
    pub path: String,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub hdus: Vec<HduReport>,
    /// Every image was decoded from the output and matched the input.
    pub verified: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum PackError {
    /// Nothing to do or not allowed (exit code 2).
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Fits(#[from] fittle_core::Error),
    #[error("{0}")]
    Decode(#[from] DecodeError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("verification failed: {0}; nothing was written")]
    Verify(String),
}

const BLOCK: usize = 2880;

fn card(key: &str, value: Value, comment: &str) -> String {
    use fittle_core::edit::{NewValue, format_card};
    let v = match value {
        Value::Logical(b) => NewValue::Logical(b),
        Value::Integer(i) => NewValue::Integer(i),
        Value::Float(f) => NewValue::Float(f),
        Value::String(s) => NewValue::String(s),
        _ => unreachable!("structural cards are simple values"),
    };
    format_card(key, &v, (!comment.is_empty()).then_some(comment)).join("")
}

fn int(key: &str, v: i64, c: &str) -> String {
    card(key, Value::Integer(v), c)
}
fn string(key: &str, v: &str, c: &str) -> String {
    card(key, Value::String(v.into()), c)
}
fn logical(key: &str, v: bool, c: &str) -> String {
    card(key, Value::Logical(v), c)
}

fn indexed(key: &str, stems: &[&str]) -> bool {
    stems.iter().any(|p| {
        key.len() > p.len()
            && key.starts_with(p)
            && key[p.len()..].bytes().all(|b| b.is_ascii_digit())
    })
}

/// Structural keywords of an uncompressed image HDU.
fn image_structural(k: &str) -> bool {
    matches!(
        k,
        "SIMPLE"
            | "XTENSION"
            | "BITPIX"
            | "NAXIS"
            | "EXTEND"
            | "PCOUNT"
            | "GCOUNT"
            | "CHECKSUM"
            | "DATASUM"
            | "END"
    ) || indexed(k, &["NAXIS"])
}

/// Keywords of a compressed-image table that don't carry over to the image.
fn compressed_structural(k: &str) -> bool {
    matches!(
        k,
        "XTENSION"
            | "BITPIX"
            | "NAXIS"
            | "PCOUNT"
            | "GCOUNT"
            | "TFIELDS"
            | "THEAP"
            | "CHECKSUM"
            | "DATASUM"
            | "END"
            | "ZIMAGE"
            | "ZBITPIX"
            | "ZNAXIS"
            | "ZCMPTYPE"
            | "ZSIMPLE"
            | "ZEXTEND"
            | "ZTENSION"
            | "ZPCOUNT"
            | "ZGCOUNT"
            | "ZQUANTIZ"
            | "ZDITHER0"
            | "ZBLANK"
            | "ZHECKSUM"
            | "ZDATASUM"
            | "ZMASKCMP"
    ) || indexed(
        k,
        &[
            "NAXIS", "ZNAXIS", "ZTILE", "ZNAME", "ZVAL", "TTYPE", "TFORM", "TUNIT", "TDIM",
            "TSCAL", "TZERO", "TNULL", "TDISP",
        ],
    )
}

/// Header records → whole blocks, END included.
fn blocks(records: &[String]) -> Vec<u8> {
    let mut b: Vec<u8> = records
        .iter()
        .flat_map(|r| format!("{r:<80}").into_bytes())
        .collect();
    b.extend(format!("{:<80}", "END").bytes());
    b.resize(b.len().div_ceil(BLOCK) * BLOCK, b' ');
    b
}

fn pad(mut data: Vec<u8>) -> Vec<u8> {
    data.resize(data.len().div_ceil(BLOCK) * BLOCK, 0);
    data
}

/// Header + data, with CHECKSUM/DATASUM when `seal` (placeholders appended).
fn hdu_bytes(mut records: Vec<String>, data: Vec<u8>, seal: bool) -> Vec<u8> {
    let data = pad(data);
    if seal {
        let sum = checksum::sum32(&data, 0);
        records.push(string("CHECKSUM", "0000000000000000", "HDU checksum"));
        records.push(string("DATASUM", &sum.to_string(), "data unit checksum"));
        let mut head = blocks(&records);
        checksum::seal(&mut head, sum);
        head.extend(data);
        return head;
    }
    let mut out = blocks(&records);
    out.extend(data);
    out
}

fn raw_hdu(bytes: &[u8], hdu: &Hdu) -> Vec<u8> {
    let start = hdu.header_offset as usize;
    let end = (hdu.data_offset + hdu.data_padded()) as usize;
    bytes[start..end.min(bytes.len())].to_vec()
}

fn data_unit<'a>(bytes: &'a [u8], hdu: &Hdu) -> Result<&'a [u8], PackError> {
    bytes
        .get(hdu.data_offset as usize..(hdu.data_offset + hdu.data_bytes) as usize)
        .ok_or(PackError::Decode(DecodeError::Truncated))
}

fn is_image(hdu: &Hdu) -> bool {
    matches!(hdu.kind, HduKind::Primary | HduKind::Image) && hdu.data_bytes > 0
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::GzEncoder::new(
        Vec::with_capacity(bytes.len() / 2),
        flate2::Compression::default(),
    );
    e.write_all(bytes).expect("in-memory write");
    e.finish().expect("in-memory write")
}

/// Compress one image HDU into a tile-compressed binary table.
fn compress_hdu(
    h: &Header,
    hdu: &Hdu,
    data: &[u8],
    opts: &PackOptions,
) -> Result<(Vec<u8>, Method), PackError> {
    let bitpix = hdu.bitpix.unwrap_or(0);
    let method = opts.method.unwrap_or(Method::auto(bitpix));
    if method == Method::Rice && !matches!(bitpix, 8 | 16 | 32) {
        return Err(PackError::Invalid(format!(
            "RICE_1 needs 8, 16 or 32-bit integers; this image is BITPIX {bitpix} (use GZIP_2)"
        )));
    }
    let width = (bitpix.unsigned_abs() / 8) as usize;
    let dims: Vec<usize> = hdu.shape.iter().map(|&d| d as usize).collect();
    let row = dims[0] * width;
    let rows_total: usize = dims[1..].iter().product::<usize>().max(1);
    let tile_rows = opts
        .tile_rows
        .unwrap_or(1)
        .clamp(1, dims.get(1).copied().unwrap_or(1).max(1));
    // Tiles are full rows, `tile_rows` at a time within each plane.
    let height = dims.get(1).copied().unwrap_or(1);
    let planes = rows_total / height.max(1);
    let mut ranges = Vec::new();
    for p in 0..planes {
        let mut y = 0;
        while y < height {
            let n = tile_rows.min(height - y);
            let start = (p * height + y) * row;
            ranges.push(start..start + n * row);
            y += n;
        }
    }
    let tiles: Vec<Vec<u8>> = ranges
        .par_iter()
        .map(|r| {
            let t = &data[r.clone()];
            match method {
                Method::Rice => {
                    let v: Vec<u32> = t
                        .chunks_exact(width)
                        .map(|c| c.iter().fold(0u32, |a, &b| (a << 8) | b as u32))
                        .collect();
                    rice::compress(&v, width, 32).expect("width checked above")
                }
                Method::Gzip1 => gzip(t),
                Method::Gzip2 => gzip(&tiles::shuffle(t, width)),
            }
        })
        .collect();

    let heap: usize = tiles.iter().map(Vec::len).sum();
    let wide = heap > i32::MAX as usize;
    let desc = if wide { 16 } else { 8 };
    let mut table = Vec::with_capacity(tiles.len() * desc + heap);
    let mut offset = 0usize;
    for t in &tiles {
        if wide {
            table.extend((t.len() as u64).to_be_bytes());
            table.extend((offset as u64).to_be_bytes());
        } else {
            table.extend((t.len() as u32).to_be_bytes());
            table.extend((offset as u32).to_be_bytes());
        }
        offset += t.len();
    }
    for t in tiles.iter() {
        table.extend_from_slice(t);
    }
    let max = tiles.iter().map(Vec::len).max().unwrap_or(0);

    let primary = hdu.kind == HduKind::Primary;
    let mut rec = vec![
        string("XTENSION", "BINTABLE", "binary table extension"),
        int("BITPIX", 8, "8-bit bytes"),
        int("NAXIS", 2, "2-dimensional binary table"),
        int("NAXIS1", desc as i64, "width of table in bytes"),
        int("NAXIS2", tiles.len() as i64, "number of rows in table"),
        int("PCOUNT", heap as i64, "size of special data area"),
        int("GCOUNT", 1, "one data group"),
        int("TFIELDS", 1, "number of fields in each row"),
        string("TTYPE1", "COMPRESSED_DATA", "label for field 1"),
        string(
            "TFORM1",
            &format!("1{}B({max})", if wide { 'Q' } else { 'P' }),
            "data format of field",
        ),
        logical("ZIMAGE", true, "extension contains compressed image"),
        int("ZBITPIX", bitpix, "data type of original image"),
        int("ZNAXIS", dims.len() as i64, "dimension of original image"),
    ];
    for (i, d) in dims.iter().enumerate() {
        rec.push(int(
            &format!("ZNAXIS{}", i + 1),
            *d as i64,
            "length of original image axis",
        ));
    }
    for i in 0..dims.len() {
        let t = match i {
            0 => dims[0],
            1 => tile_rows,
            _ => 1,
        };
        rec.push(int(
            &format!("ZTILE{}", i + 1),
            t as i64,
            "size of tiles to be compressed",
        ));
    }
    rec.push(string(
        "ZCMPTYPE",
        method.keyword(),
        "compression algorithm",
    ));
    if method == Method::Rice {
        rec.push(string("ZNAME1", "BLOCKSIZE", "compression block size"));
        rec.push(int("ZVAL1", 32, "pixels per block"));
        rec.push(string(
            "ZNAME2",
            "BYTEPIX",
            "bytes per pixel (1, 2, 4, or 8)",
        ));
        rec.push(int(
            "ZVAL2",
            width as i64,
            "bytes per pixel (1, 2, 4, or 8)",
        ));
    }
    if primary {
        rec.push(logical(
            "ZSIMPLE",
            true,
            "file does conform to FITS standard",
        ));
        if let Some(e) = h.logical("EXTEND") {
            rec.push(logical("ZEXTEND", e, "FITS dataset may contain extensions"));
        }
    } else {
        rec.push(string("ZTENSION", "IMAGE", "image extension"));
        rec.push(int(
            "ZPCOUNT",
            h.int("PCOUNT").unwrap_or(0),
            "number of parameters",
        ));
        rec.push(int(
            "ZGCOUNT",
            h.int("GCOUNT").unwrap_or(1),
            "number of groups",
        ));
    }
    // Checksums: the original DATASUM covers only pixels, which round-trip
    // exactly, so it travels as ZDATASUM and funpack restores a sealed image.
    // The table itself is left unsealed: astropy re-derives compressed
    // headers before checking CHECKSUM, so a sealed table written in any
    // other card layout reads as corrupt there.
    if let Some(d) = h.string("DATASUM") {
        rec.push(string(
            "ZDATASUM",
            d,
            "data unit checksum of the original image",
        ));
    }
    for c in &h.cards {
        if !image_structural(&c.keyword) {
            rec.extend(c.raw.iter().cloned());
        }
    }
    Ok((hdu_bytes(rec, table, false), method))
}

/// Rebuild an image HDU from a compressed table.
fn decompress_hdu(h: &Header, img: &tiles::TileImage, primary: bool, extend: bool) -> Vec<u8> {
    let mut rec = if primary {
        vec![logical("SIMPLE", true, "conforms to FITS standard")]
    } else {
        vec![string("XTENSION", "IMAGE", "image extension")]
    };
    rec.push(int("BITPIX", img.bitpix, "array data type"));
    rec.push(int(
        "NAXIS",
        img.dims.len() as i64,
        "number of array dimensions",
    ));
    for (i, d) in img.dims.iter().enumerate() {
        rec.push(int(&format!("NAXIS{}", i + 1), *d as i64, ""));
    }
    if primary {
        if extend || h.logical("ZEXTEND").unwrap_or(false) {
            rec.push(logical("EXTEND", true, ""));
        }
    } else {
        rec.push(int(
            "PCOUNT",
            h.int("ZPCOUNT").unwrap_or(0),
            "number of parameters",
        ));
        rec.push(int(
            "GCOUNT",
            h.int("ZGCOUNT").unwrap_or(1),
            "number of groups",
        ));
    }
    for c in &h.cards {
        let k = c.keyword.as_str();
        if compressed_structural(k) {
            continue;
        }
        if k == "EXTNAME" && h.string("EXTNAME").map(str::trim) == Some("COMPRESSED_IMAGE") {
            continue;
        }
        rec.extend(c.raw.iter().cloned());
    }
    let sealed = ["ZHECKSUM", "ZDATASUM", "CHECKSUM"]
        .iter()
        .any(|k| h.get(k).is_some());
    hdu_bytes(rec, img.to_be_bytes(), sealed)
}

/// Write `bytes` to a temp sibling of `out`, check it with `verify`, then
/// move it into place without replacing an existing file.
fn commit(
    out: &Path,
    bytes: &[u8],
    verify: impl Fn(&Path) -> Result<(), PackError>,
) -> Result<(), PackError> {
    if out.exists() {
        return Err(PackError::Invalid(format!(
            "{} already exists; Fittle never overwrites",
            out.display()
        )));
    }
    let tmp = out.with_file_name(format!(
        ".{}.fittle-part",
        out.file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default()
    ));
    std::fs::write(&tmp, bytes)?;
    if let Err(e) = verify(&tmp) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    match std::fs::hard_link(&tmp, out) {
        Ok(()) => std::fs::remove_file(&tmp)?,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let _ = std::fs::remove_file(&tmp);
            return Err(PackError::Invalid(format!(
                "{} already exists; Fittle never overwrites",
                out.display()
            )));
        }
        Err(_) if !out.exists() => std::fs::rename(&tmp, out)?,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(e.into());
        }
    }
    Ok(())
}

/// Default output name: `x.fits` → `x.fits.fz`.
pub fn packed_name(src: &Path) -> PathBuf {
    let mut s = src.as_os_str().to_owned();
    s.push(".fz");
    PathBuf::from(s)
}

/// Default output name: `x.fits.fz` → `x.fits`; `x.fz` → `x.fits`.
pub fn unpacked_name(src: &Path) -> PathBuf {
    let s = src.to_string_lossy();
    match s.strip_suffix(".fz") {
        Some(stem)
            if stem
                .rsplit(['/', '\\'])
                .next()
                .is_some_and(|n| n.contains('.')) =>
        {
            PathBuf::from(stem)
        }
        Some(stem) => PathBuf::from(format!("{stem}.fits")),
        None => PathBuf::from(format!("{s}.fits")),
    }
}

/// Tile-compress every image HDU of `src` into `out`; other HDUs are copied.
pub fn fpack(src: &Path, out: &Path, opts: &PackOptions) -> Result<PackReport, PackError> {
    let bytes = std::fs::read(src)?;
    let fits = Fits::from_bytes(&bytes)?;
    if !fits.hdus.iter().any(is_image) {
        return Err(PackError::Invalid(
            if fits.hdus.iter().any(|h| h.kind == HduKind::CompressedImage) {
                "already compressed".into()
            } else {
                "no image to compress".into()
            },
        ));
    }
    let mut outb = Vec::new();
    let mut hdus = Vec::new();
    let mut originals = Vec::new();
    for hdu in &fits.hdus {
        let before = hdu.data_padded() + hdu.header_blocks * BLOCK as u64;
        if !is_image(hdu) {
            let raw = raw_hdu(&bytes, hdu);
            hdus.push(HduReport {
                index: hdu.index,
                action: "copied",
                method: None,
                bytes_before: before,
                bytes_after: raw.len() as u64,
            });
            outb.extend(raw);
            continue;
        }
        if hdu.kind == HduKind::Primary {
            // fpack convention: an empty primary, the image in extension 1.
            outb.extend(hdu_bytes(
                vec![
                    logical("SIMPLE", true, "file does conform to FITS standard"),
                    int("BITPIX", 8, "number of bits per data pixel"),
                    int("NAXIS", 0, "number of data axes"),
                    logical("EXTEND", true, "FITS dataset may contain extensions"),
                ],
                Vec::new(),
                false,
            ));
        }
        let data = data_unit(&bytes, hdu)?;
        let (b, method) = compress_hdu(hdu.header(), hdu, data, opts)?;
        originals.push(data.to_vec());
        hdus.push(HduReport {
            index: hdu.index,
            action: "compressed",
            method: Some(method.keyword().into()),
            bytes_before: before,
            bytes_after: b.len() as u64,
        });
        outb.extend(b);
    }
    commit(out, &outb, |p| {
        // Every compressed HDU must decode to the original bytes.
        let f = Fits::open(p)?;
        let written = std::fs::read(p)?;
        let mut want = originals.iter();
        for h in f.hdus.iter().filter(|h| h.kind == HduKind::CompressedImage) {
            let img = tiles::decode(h, data_unit(&written, h)?)?;
            if Some(&img.to_be_bytes()) != want.next() {
                return Err(PackError::Verify(format!(
                    "HDU {} did not decode to the original pixels",
                    h.index
                )));
            }
        }
        Ok(())
    })?;
    Ok(PackReport {
        source: src.to_string_lossy().into(),
        path: out.to_string_lossy().into(),
        bytes_in: bytes.len() as u64,
        bytes_out: outb.len() as u64,
        hdus,
        verified: true,
    })
}

/// Expand every tile-compressed HDU of `src` into a plain image in `out`.
pub fn funpack(src: &Path, out: &Path) -> Result<PackReport, PackError> {
    let bytes = std::fs::read(src)?;
    let fits = Fits::from_bytes(&bytes)?;
    if !fits.hdus.iter().any(|h| h.kind == HduKind::CompressedImage) {
        return Err(PackError::Invalid(
            "no compressed image in this file".into(),
        ));
    }
    // An empty primary followed by a compressed image becomes that image.
    let first = &fits.hdus[0];
    let empty_primary = first.data_bytes == 0
        && first
            .cards
            .cards
            .iter()
            .all(|c| image_structural(&c.keyword) || matches!(c.value, Value::Commentary(_)));
    let promote = empty_primary
        && fits
            .hdus
            .get(1)
            .is_some_and(|h| h.kind == HduKind::CompressedImage);
    let mut outb = Vec::new();
    let mut hdus = Vec::new();
    let mut expected = Vec::new();
    for hdu in &fits.hdus {
        let before = hdu.data_padded() + hdu.header_blocks * BLOCK as u64;
        if promote && hdu.index == 0 {
            hdus.push(HduReport {
                index: 0,
                action: "merged",
                method: None,
                bytes_before: before,
                bytes_after: 0,
            });
            continue;
        }
        if hdu.kind != HduKind::CompressedImage {
            let raw = raw_hdu(&bytes, hdu);
            hdus.push(HduReport {
                index: hdu.index,
                action: "copied",
                method: None,
                bytes_before: before,
                bytes_after: raw.len() as u64,
            });
            outb.extend(raw);
            continue;
        }
        let img = tiles::decode(hdu, data_unit(&bytes, hdu)?)?;
        let primary = promote && hdu.index == 1;
        let b = decompress_hdu(hdu.header(), &img, primary, fits.hdus.len() > 2);
        // Index of this HDU in the output file.
        let at = hdu.index - usize::from(promote);
        expected.push((at, img.to_be_bytes()));
        hdus.push(HduReport {
            index: hdu.index,
            action: "decompressed",
            method: hdu
                .header()
                .string("ZCMPTYPE")
                .map(|s| s.trim().to_string()),
            bytes_before: before,
            bytes_after: b.len() as u64,
        });
        outb.extend(b);
    }
    commit(out, &outb, |p| {
        let f = Fits::open(p)?;
        let written = std::fs::read(p)?;
        for (at, want) in &expected {
            let h = f
                .hdus
                .get(*at)
                .ok_or_else(|| PackError::Verify(format!("HDU {at} missing from the output")))?;
            if data_unit(&written, h)? != want.as_slice() {
                return Err(PackError::Verify(format!("HDU {at} data does not match")));
            }
        }
        Ok(())
    })?;
    Ok(PackReport {
        source: src.to_string_lossy().into(),
        path: out.to_string_lossy().into(),
        bytes_in: bytes.len() as u64,
        bytes_out: outb.len() as u64,
        hdus,
        verified: true,
    })
}
