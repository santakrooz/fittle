//! Decode an image data unit (plain or tile-compressed). Reads exactly the data-unit bytes
//! whose location `fittle-core` recorded; never writes.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use fittle_core::{Hdu, HduKind};

use crate::tiles;

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("HDU {0} is not an uncompressed image")]
    NotImage(usize),
    #[error("unsupported BITPIX {0}")]
    Bitpix(i64),
    #[error("image has no pixels")]
    Empty,
    #[error("data unit is shorter than its header says")]
    Truncated,
    #[error("unsupported tile compression: {0}")]
    Unsupported(String),
}

/// Physical pixel values (`BZERO + BSCALE × stored`), planes stored one after
/// another: `data[(plane * height + y) * width + x]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub planes: usize,
    pub data: Vec<f32>,
}

impl Image {
    pub fn get(&self, x: usize, y: usize, plane: usize) -> f32 {
        self.data[(plane * self.height + y) * self.width + x]
    }
}

pub fn decode_hdu(path: impl AsRef<Path>, hdu: &Hdu) -> Result<Image, DecodeError> {
    if !matches!(
        hdu.kind,
        HduKind::Primary | HduKind::Image | HduKind::CompressedImage
    ) {
        return Err(DecodeError::NotImage(hdu.index));
    }
    let bitpix = hdu.bitpix.ok_or(DecodeError::Bitpix(0))?;
    let width = *hdu.shape.first().ok_or(DecodeError::Empty)? as usize;
    let height = hdu.shape.get(1).copied().unwrap_or(1) as usize;
    let planes = hdu.shape.iter().skip(2).product::<u64>() as usize;
    let n = width * height * planes;
    if n == 0 {
        return Err(DecodeError::Empty);
    }

    let mut bytes = vec![0u8; hdu.data_bytes as usize];
    let mut f = File::open(path)?;
    f.seek(SeekFrom::Start(hdu.data_offset))?;
    f.read_exact(&mut bytes).map_err(|e| match e.kind() {
        std::io::ErrorKind::UnexpectedEof => DecodeError::Truncated,
        _ => e.into(),
    })?;

    let h = hdu.header();
    let zero = h.float("BZERO").unwrap_or(0.0);
    let scale = h.float("BSCALE").unwrap_or(1.0);
    let phys = |v: f64| (zero + scale * v) as f32;

    if hdu.kind == HduKind::CompressedImage {
        let raw = tiles::decode(hdu, &bytes)?;
        let data = raw.into_iter().map(|v| phys(v as f64)).collect();
        return Ok(Image {
            width,
            height,
            planes,
            data,
        });
    }

    let data: Vec<f32> = match bitpix {
        8 => bytes.iter().take(n).map(|&b| phys(b as f64)).collect(),
        16 => be(&bytes, 2, n, |c| {
            phys(i16::from_be_bytes([c[0], c[1]]) as f64)
        }),
        32 => be(&bytes, 4, n, |c| {
            phys(i32::from_be_bytes(c.try_into().unwrap()) as f64)
        }),
        64 => be(&bytes, 8, n, |c| {
            phys(i64::from_be_bytes(c.try_into().unwrap()) as f64)
        }),
        -32 => be(&bytes, 4, n, |c| {
            phys(f32::from_be_bytes(c.try_into().unwrap()) as f64)
        }),
        -64 => be(&bytes, 8, n, |c| {
            phys(f64::from_be_bytes(c.try_into().unwrap()))
        }),
        b => return Err(DecodeError::Bitpix(b)),
    };
    Ok(Image {
        width,
        height,
        planes,
        data,
    })
}

fn be(bytes: &[u8], width: usize, n: usize, f: impl Fn(&[u8]) -> f32) -> Vec<f32> {
    bytes.chunks_exact(width).take(n).map(f).collect()
}
