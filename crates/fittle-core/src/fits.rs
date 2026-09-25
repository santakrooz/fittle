//! File layout: walk HDUs reading headers only, and record where each data
//! unit lives. Pixels are never read here.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use serde::Serialize;

use crate::card::CARD_LEN;
use crate::header::{Header, HeaderBuilder};

/// FITS logical block size in bytes.
pub const BLOCK_LEN: u64 = 2880;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("not a FITS file: first record is not SIMPLE")]
    NotFits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// Something wrong or notable about the file, surfaced as header health.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Issue {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hdu: Option<usize>,
    /// Zero-based record index within that HDU's header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record: Option<usize>,
}

impl Issue {
    fn new(severity: Severity, code: &str, message: impl Into<String>, hdu: Option<usize>) -> Self {
        Issue {
            severity,
            code: code.into(),
            message: message.into(),
            hdu,
            record: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HduKind {
    Primary,
    Image,
    Table,
    BinTable,
    /// Tile-compressed image stored in a BINTABLE (`ZIMAGE = T`, e.g. `.fz`).
    CompressedImage,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hdu {
    pub index: usize,
    pub kind: HduKind,
    /// Byte offset of the first header block.
    pub header_offset: u64,
    /// Number of 2,880-byte header blocks.
    pub header_blocks: u64,
    /// Byte offset of the data unit.
    pub data_offset: u64,
    /// Data unit length without padding.
    pub data_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitpix: Option<i64>,
    /// `NAXIS1..NAXISn` (for compressed images, `ZNAXIS1..n`).
    pub shape: Vec<u64>,
    pub cards: Header,
}

impl Hdu {
    pub fn header(&self) -> &Header {
        &self.cards
    }

    /// Data unit length including padding to a whole block.
    pub fn data_padded(&self) -> u64 {
        self.data_bytes.div_ceil(BLOCK_LEN) * BLOCK_LEN
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Fits {
    pub file_bytes: u64,
    pub hdus: Vec<Hdu>,
    pub issues: Vec<Issue>,
}

impl Fits {
    /// Read every header in the file. Data units are skipped, not read.
    pub fn open(path: impl AsRef<Path>) -> Result<Fits, Error> {
        let mut f = File::open(path)?;
        let len = f.metadata()?.len();
        Self::read(&mut f, len)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Fits, Error> {
        Self::read(&mut io::Cursor::new(bytes), bytes.len() as u64)
    }

    pub fn read<R: Read + Seek>(r: &mut R, len: u64) -> Result<Fits, Error> {
        let mut first = [0u8; 10];
        if len < 10 || r.read_exact(&mut first).is_err() || &first[..] != b"SIMPLE  = " {
            return Err(Error::NotFits);
        }
        r.seek(SeekFrom::Start(0))?;

        let mut hdus = Vec::new();
        let mut issues = Vec::new();
        let mut offset = 0u64;
        let mut block = [0u8; BLOCK_LEN as usize];

        while offset < len {
            let index = hdus.len();
            if index > 0 && len - offset < BLOCK_LEN {
                issues.push(Issue::new(
                    Severity::Warning,
                    "trailing_bytes",
                    format!("{} bytes after the last HDU", len - offset),
                    None,
                ));
                break;
            }

            let mut builder = HeaderBuilder::default();
            let mut blocks = 0u64;
            let mut ended = false;
            r.seek(SeekFrom::Start(offset))?;
            while !ended {
                if read_block(r, &mut block)? < BLOCK_LEN as usize {
                    break;
                }
                if blocks == 0 && index > 0 && !block.starts_with(b"XTENSION= ") {
                    break;
                }
                blocks += 1;
                for card in block.chunks_exact(CARD_LEN) {
                    if builder.push(card) {
                        ended = true;
                        break;
                    }
                }
            }

            if index > 0 && blocks == 0 {
                issues.push(Issue::new(
                    Severity::Warning,
                    "trailing_bytes",
                    format!(
                        "{} bytes after the last HDU are not an extension",
                        len - offset
                    ),
                    None,
                ));
                break;
            }

            let (cards, problems) = builder.finish();
            for (record, p) in problems {
                let mut issue = Issue::new(Severity::Warning, p.code, p.message, Some(index));
                issue.record = Some(record);
                issues.push(issue);
            }
            if !ended {
                issues.push(Issue::new(
                    Severity::Error,
                    "missing_end",
                    "header has no END record (file truncated?)",
                    Some(index),
                ));
            }

            let layout = Layout::of(&cards, index, &mut issues);
            let data_offset = offset + blocks * BLOCK_LEN;
            if ended && data_offset + layout.data_bytes > len {
                issues.push(Issue::new(
                    Severity::Error,
                    "truncated_data",
                    format!(
                        "data unit needs {} bytes but only {} remain",
                        layout.data_bytes,
                        len.saturating_sub(data_offset)
                    ),
                    Some(index),
                ));
            }
            check_duplicates(&cards, index, &mut issues);

            let hdu = Hdu {
                index,
                kind: layout.kind,
                header_offset: offset,
                header_blocks: blocks,
                data_offset,
                data_bytes: layout.data_bytes,
                bitpix: layout.bitpix,
                shape: layout.shape,
                cards,
            };
            offset = data_offset + hdu.data_padded();
            hdus.push(hdu);
            if !ended {
                break;
            }
        }

        if len % BLOCK_LEN != 0 {
            issues.push(Issue::new(
                Severity::Warning,
                "size_not_block_multiple",
                format!("file size {len} is not a multiple of 2880"),
                None,
            ));
        }

        Ok(Fits {
            file_bytes: len,
            hdus,
            issues,
        })
    }
}

/// Fill `buf` as far as the reader allows; returns bytes read.
fn read_block<R: Read>(r: &mut R, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(n)
}

struct Layout {
    kind: HduKind,
    bitpix: Option<i64>,
    shape: Vec<u64>,
    data_bytes: u64,
}

impl Layout {
    fn of(h: &Header, index: usize, issues: &mut Vec<Issue>) -> Layout {
        let kind = if index == 0 {
            HduKind::Primary
        } else {
            match h.string("XTENSION").map(str::trim) {
                Some("IMAGE") => HduKind::Image,
                Some("TABLE") => HduKind::Table,
                Some("BINTABLE") if h.logical("ZIMAGE") == Some(true) => HduKind::CompressedImage,
                Some("BINTABLE") => HduKind::BinTable,
                Some(other) => HduKind::Other(other.to_string()),
                None => HduKind::Other(String::new()),
            }
        };

        let mut missing = |key: &str| {
            issues.push(Issue::new(
                Severity::Error,
                "missing_required",
                format!("required keyword {key} is missing or not an integer"),
                Some(index),
            ));
        };
        let bitpix = h.int("BITPIX");
        let naxis = h.int("NAXIS");
        if bitpix.is_none() {
            missing("BITPIX");
        }
        if naxis.is_none() {
            missing("NAXIS");
        }
        let naxis = naxis.unwrap_or(0).max(0) as usize;
        let mut axes = Vec::with_capacity(naxis);
        for n in 1..=naxis {
            let key = format!("NAXIS{n}");
            match h.int(&key) {
                Some(v) if v >= 0 => axes.push(v as u64),
                _ => {
                    missing(&key);
                    axes.push(0);
                }
            }
        }

        let (pcount, gcount) = (h.int("PCOUNT").unwrap_or(0), h.int("GCOUNT").unwrap_or(1));
        let random_groups =
            index == 0 && h.logical("GROUPS") == Some(true) && axes.first() == Some(&0);
        let elems: u64 = if axes.is_empty() {
            0
        } else if random_groups {
            axes[1..].iter().product()
        } else {
            axes.iter().product()
        };
        let bytes_per = bitpix.map_or(0, |b| b.unsigned_abs() / 8);
        let data_bytes = if axes.is_empty() {
            0
        } else {
            bytes_per * gcount.max(0) as u64 * (pcount.max(0) as u64 + elems)
        };

        let shape = if kind == HduKind::CompressedImage {
            let zn = h.int("ZNAXIS").unwrap_or(0).max(0);
            (1..=zn)
                .map(|n| h.int(&format!("ZNAXIS{n}")).unwrap_or(0).max(0) as u64)
                .collect()
        } else {
            axes
        };
        let bitpix = if kind == HduKind::CompressedImage {
            h.int("ZBITPIX").or(bitpix)
        } else {
            bitpix
        };

        Layout {
            kind,
            bitpix,
            shape,
            data_bytes,
        }
    }
}

fn check_duplicates(h: &Header, index: usize, issues: &mut Vec<Issue>) {
    let mut seen = std::collections::HashMap::new();
    for c in h.cards.iter().filter(|c| !c.value.is_commentary()) {
        if let Some(first) = seen.insert(c.keyword.as_str(), c.record) {
            let mut issue = Issue::new(
                Severity::Warning,
                "duplicate_keyword",
                format!(
                    "{} appears more than once (first at record {first})",
                    c.keyword
                ),
                Some(index),
            );
            issue.record = Some(c.record);
            issues.push(issue);
            seen.insert(c.keyword.as_str(), first);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a header block list from card strings, padding with spaces.
    fn header(cards: &[&str]) -> Vec<u8> {
        let mut out: Vec<u8> = cards
            .iter()
            .flat_map(|c| format!("{c:<80}").into_bytes())
            .collect();
        out.extend(format!("{:<80}", "END").bytes());
        out.resize(out.len().div_ceil(2880) * 2880, b' ');
        out
    }

    fn data(n: usize) -> Vec<u8> {
        let mut d = vec![7u8; n];
        d.resize(n.div_ceil(2880) * 2880, 0);
        d
    }

    #[test]
    fn primary_plus_extension() {
        let mut f = header(&[
            "SIMPLE  = T",
            "BITPIX  = 16",
            "NAXIS   = 2",
            "NAXIS1  = 10",
            "NAXIS2  = 5",
        ]);
        f.extend(data(100));
        f.extend(header(&[
            "XTENSION= 'BINTABLE'",
            "BITPIX  = 8",
            "NAXIS   = 2",
            "NAXIS1  = 4",
            "NAXIS2  = 3",
            "PCOUNT  = 0",
            "GCOUNT  = 1",
        ]));
        f.extend(data(12));
        let fits = Fits::from_bytes(&f).unwrap();
        assert!(fits.issues.is_empty(), "{:?}", fits.issues);
        assert_eq!(fits.hdus.len(), 2);
        assert_eq!(fits.hdus[0].data_offset, 2880);
        assert_eq!(fits.hdus[0].data_bytes, 100);
        assert_eq!(fits.hdus[0].shape, [10, 5]);
        assert_eq!(fits.hdus[1].kind, HduKind::BinTable);
        assert_eq!(fits.hdus[1].header_offset, 5760);
        assert_eq!(fits.hdus[1].data_bytes, 12);
    }

    #[test]
    fn not_fits() {
        assert!(matches!(Fits::from_bytes(b"hello"), Err(Error::NotFits)));
    }

    #[test]
    fn truncated_data_and_duplicates() {
        let mut f = header(&[
            "SIMPLE  = T",
            "BITPIX  = 16",
            "NAXIS   = 1",
            "NAXIS1  = 4000",
            "NAXIS1  = 4000",
        ]);
        f.extend(vec![0u8; 100]);
        let fits = Fits::from_bytes(&f).unwrap();
        let codes: Vec<_> = fits.issues.iter().map(|i| i.code.as_str()).collect();
        assert_eq!(
            codes,
            [
                "truncated_data",
                "duplicate_keyword",
                "size_not_block_multiple"
            ]
        );
    }

    #[test]
    fn missing_end() {
        let mut f: Vec<u8> = ["SIMPLE  = T", "BITPIX  = 8", "NAXIS   = 0"]
            .iter()
            .flat_map(|c| format!("{c:<80}").into_bytes())
            .collect();
        f.resize(2880, b' ');
        let fits = Fits::from_bytes(&f).unwrap();
        assert_eq!(fits.hdus.len(), 1);
        assert!(fits.issues.iter().any(|i| i.code == "missing_end"));
    }

    #[test]
    fn header_spanning_blocks() {
        let many: Vec<String> = (0..40).map(|i| format!("KEY{i:<5}= {i}")).collect();
        let mut cards = vec![
            "SIMPLE  = T".to_string(),
            "BITPIX  = 8".into(),
            "NAXIS   = 0".into(),
        ];
        cards.extend(many);
        let refs: Vec<&str> = cards.iter().map(String::as_str).collect();
        let fits = Fits::from_bytes(&header(&refs)).unwrap();
        assert_eq!(fits.hdus[0].header_blocks, 2);
        assert_eq!(fits.hdus[0].cards.cards.len(), 43);
        assert!(fits.issues.is_empty());
    }
}
