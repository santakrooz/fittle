//! File encoders for exports: PNG (8/16), JPEG, WebP (lossless), TIFF
//! (8/16/32-bit float) and FITS. All pure Rust. XMP is embedded where the
//! format has a standard place for it (PNG iTXt, JPEG APP1, TIFF tag 700,
//! WebP XMP chunk).

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

/// Interleaved pixels ready to encode (1 = grey, 3 = RGB channels).
#[derive(Debug, Clone, PartialEq)]
pub enum Samples {
    U8(Vec<u8>),
    U16(Vec<u16>),
    F32(Vec<f32>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Raster {
    pub width: usize,
    pub height: usize,
    pub channels: usize,
    pub samples: Samples,
}

fn other(e: impl std::fmt::Display) -> io::Error {
    io::Error::other(e.to_string())
}

pub fn png(path: &Path, r: &Raster, xmp: Option<&str>) -> io::Result<()> {
    let w = BufWriter::new(File::create(path)?);
    let mut enc = png::Encoder::new(w, r.width as u32, r.height as u32);
    enc.set_color(if r.channels == 3 {
        png::ColorType::Rgb
    } else {
        png::ColorType::Grayscale
    });
    let bytes: Vec<u8> = match &r.samples {
        Samples::U8(d) => {
            enc.set_depth(png::BitDepth::Eight);
            d.clone()
        }
        Samples::U16(d) => {
            enc.set_depth(png::BitDepth::Sixteen);
            d.iter().flat_map(|v| v.to_be_bytes()).collect()
        }
        Samples::F32(_) => return Err(other("PNG has no float samples; use 8 or 16 bits")),
    };
    if let Some(x) = xmp {
        enc.add_itxt_chunk("XML:com.adobe.xmp".into(), x.into())
            .map_err(other)?;
    }
    let mut wr = enc.write_header().map_err(other)?;
    wr.write_image_data(&bytes).map_err(other)?;
    wr.finish().map_err(other)
}

pub fn jpeg(path: &Path, r: &Raster, quality: u8, xmp: Option<&str>) -> io::Result<()> {
    let Samples::U8(d) = &r.samples else {
        return Err(other("JPEG needs 8-bit samples"));
    };
    if r.width > 65_535 || r.height > 65_535 {
        return Err(other("JPEG is limited to 65,535 px per side"));
    }
    let mut enc = jpeg_encoder::Encoder::new_file(path, quality.clamp(1, 100)).map_err(other)?;
    if let Some(x) = xmp {
        let mut seg = b"http://ns.adobe.com/xap/1.0/\0".to_vec();
        seg.extend_from_slice(x.as_bytes());
        if seg.len() < 65_533 {
            enc.add_app_segment(1, seg).map_err(other)?;
        }
    }
    let ct = if r.channels == 3 {
        jpeg_encoder::ColorType::Rgb
    } else {
        jpeg_encoder::ColorType::Luma
    };
    enc.encode(d, r.width as u16, r.height as u16, ct)
        .map_err(other)
}

pub fn webp(path: &Path, r: &Raster, xmp: Option<&str>) -> io::Result<()> {
    let Samples::U8(d) = &r.samples else {
        return Err(other("WebP needs 8-bit samples"));
    };
    let w = BufWriter::new(File::create(path)?);
    let mut enc = image_webp::WebPEncoder::new(w);
    if let Some(x) = xmp {
        enc.set_xmp_metadata(x.as_bytes().to_vec());
    }
    let ct = if r.channels == 3 {
        image_webp::ColorType::Rgb8
    } else {
        image_webp::ColorType::L8
    };
    enc.encode(d, r.width as u32, r.height as u32, ct)
        .map_err(other)
}

pub fn tiff(path: &Path, r: &Raster, xmp: Option<&str>) -> io::Result<()> {
    use tiff::encoder::{TiffEncoder, colortype};
    use tiff::tags::Tag;
    let w = BufWriter::new(File::create(path)?);
    let mut enc = TiffEncoder::new(w).map_err(other)?;
    let (width, height) = (r.width as u32, r.height as u32);
    macro_rules! write {
        ($ct:ty, $data:expr) => {{
            let mut img = enc.new_image::<$ct>(width, height).map_err(other)?;
            if let Some(x) = xmp {
                img.encoder()
                    .write_tag(Tag::Unknown(700), x.as_bytes())
                    .map_err(other)?;
            }
            img.write_data($data).map_err(other)
        }};
    }
    match (&r.samples, r.channels) {
        (Samples::U8(d), 3) => write!(colortype::RGB8, d),
        (Samples::U8(d), _) => write!(colortype::Gray8, d),
        (Samples::U16(d), 3) => write!(colortype::RGB16, d),
        (Samples::U16(d), _) => write!(colortype::Gray16, d),
        (Samples::F32(d), 3) => write!(colortype::RGB32Float, d),
        (Samples::F32(d), _) => write!(colortype::Gray32Float, d),
    }
}

/// Write a single-HDU FITS file: `records` are 80-char header cards (without
/// END); `data` is the big-endian data unit (padded here).
pub fn fits(path: &Path, records: &[String], data: &[u8]) -> io::Result<()> {
    let mut w = BufWriter::new(File::create(path)?);
    let mut head: Vec<u8> = records
        .iter()
        .flat_map(|r| format!("{r:<80}").into_bytes())
        .collect();
    head.extend(format!("{:<80}", "END").bytes());
    head.resize(head.len().div_ceil(2880) * 2880, b' ');
    w.write_all(&head)?;
    w.write_all(data)?;
    let pad = (2880 - data.len() % 2880) % 2880;
    w.write_all(&vec![0u8; pad])?;
    w.flush()
}

/// Minimal big-endian EXIF (TIFF) block: ImageDescription and Software.
pub fn exif(description: &str, software: &str) -> Vec<u8> {
    let ascii = |s: &str| -> Vec<u8> {
        let mut v: Vec<u8> = s
            .chars()
            .map(|c| {
                if c.is_ascii() && !c.is_ascii_control() {
                    c as u8
                } else {
                    b'?'
                }
            })
            .collect();
        v.push(0);
        v
    };
    let entries = [
        (0x010Eu16, ascii(description)),
        (0x0131u16, ascii(software)),
    ];
    let ifd_len = 2 + entries.len() * 12 + 4;
    let mut out = b"MM\0\x2a".to_vec();
    out.extend(8u32.to_be_bytes());
    out.extend((entries.len() as u16).to_be_bytes());
    let mut data_at = 8 + ifd_len;
    let mut values = Vec::new();
    for (tag, v) in &entries {
        out.extend(tag.to_be_bytes());
        out.extend(2u16.to_be_bytes()); // ASCII
        out.extend((v.len() as u32).to_be_bytes());
        if v.len() <= 4 {
            let mut inline = v.clone();
            inline.resize(4, 0);
            out.extend(inline);
        } else {
            out.extend((data_at as u32).to_be_bytes());
            data_at += v.len();
            values.extend_from_slice(v);
        }
    }
    out.extend(0u32.to_be_bytes()); // no next IFD
    out.extend(values);
    out
}

/// AVIF (AV1, 8-bit, pure-Rust rav1e). `quality` 1–100. Metadata goes in as
/// EXIF (`exif`), since the AVIF writer has no XMP slot.
pub fn avif(path: &Path, r: &Raster, quality: u8, exif: Option<Vec<u8>>) -> io::Result<()> {
    let Samples::U8(d) = &r.samples else {
        return Err(other("AVIF needs 8-bit samples"));
    };
    let rgb: Vec<ravif::RGB8> = if r.channels == 3 {
        d.chunks_exact(3)
            .map(|c| ravif::RGB8::new(c[0], c[1], c[2]))
            .collect()
    } else {
        d.iter().map(|&v| ravif::RGB8::new(v, v, v)).collect()
    };
    let mut enc = ravif::Encoder::new()
        .with_quality(quality.clamp(1, 100) as f32)
        .with_speed(6);
    if let Some(e) = exif {
        enc = enc.with_exif(e);
    }
    let out = enc
        .encode_rgb(ravif::Img::new(&rgb[..], r.width, r.height))
        .map_err(other)?;
    std::fs::write(path, out.avif_file)
}
