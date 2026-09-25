//! Safe header writes (CLAUDE.md rule 1).
//!
//! - Data-unit bytes never change: every byte before the edited header and
//!   every byte after it are hashed before and after the write.
//! - Same header block count: the header blocks are rewritten in place.
//!   Otherwise a temp file is written next to the original, verified, and
//!   atomically renamed over it.
//! - A `.bak` copy is kept on the first edit (optional), CHECKSUM/DATASUM are
//!   resealed when present, and the result is re-read to confirm the edits.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::checksum;
use crate::edit::{ChangeKind, EditError, Op, Options, Plan, plan_for};
use crate::fits::{BLOCK_LEN, Fits};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    /// Nothing to change.
    None,
    InPlace,
    Rewrite,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WriteReport {
    #[serde(flatten)]
    pub plan: Plan,
    pub method: Method,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup: Option<String>,
    /// SHA-256 of every byte outside the edited header, identical before and after.
    pub untouched_sha256: String,
    /// New CHECKSUM value, when the HDU carries one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
}

const BUF: usize = 1 << 20;

/// Hash a byte range of a file.
fn hash_range(f: &mut File, start: u64, end: u64, h: &mut Sha256) -> io::Result<()> {
    f.seek(SeekFrom::Start(start))?;
    let mut left = end.saturating_sub(start);
    let mut buf = vec![0u8; BUF];
    while left > 0 {
        let n = (left as usize).min(BUF);
        f.read_exact(&mut buf[..n])?;
        h.update(&buf[..n]);
        left -= n as u64;
    }
    Ok(())
}

/// SHA-256 of everything except the header blocks `[h0, h1)`.
fn outside_header(path: &Path, h0: u64, h1: u64) -> io::Result<String> {
    let mut f = File::open(path)?;
    let len = f.metadata()?.len();
    let mut h = Sha256::new();
    hash_range(&mut f, 0, h0, &mut h)?;
    h.update(b"|header|");
    hash_range(&mut f, h1, len, &mut h)?;
    Ok(hex(&h.finalize()))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn copy_range(src: &mut File, start: u64, end: u64, dst: &mut impl Write) -> io::Result<()> {
    src.seek(SeekFrom::Start(start))?;
    io::copy(&mut src.take(end - start), dst)?;
    Ok(())
}

/// Ones'-complement sum of a data unit (including its padding).
fn data_sum(path: &Path, start: u64, len: u64) -> io::Result<u32> {
    let mut f = File::open(path)?;
    f.seek(SeekFrom::Start(start))?;
    let mut sum = 0u32;
    let mut left = len;
    let mut buf = vec![0u8; BUF];
    while left > 0 {
        let n = (left as usize).min(BUF);
        f.read_exact(&mut buf[..n])?;
        sum = checksum::sum32(&buf[..n], sum);
        left -= n as u64;
    }
    Ok(sum)
}

fn card(text: &str) -> String {
    format!("{text:<80}")
}

/// Header bytes (whole blocks, END included), with CHECKSUM/DATASUM resealed
/// when the plan's HDU carries them and `opts.checksum` is set.
fn header_bytes(plan: &Plan, sum: Option<u32>) -> (Vec<u8>, Option<String>) {
    let mut recs: Vec<String> = plan.records.clone();
    if let Some(ds) = sum {
        recs.retain(|r| !r.starts_with("CHECKSUM=") && !r.starts_with("DATASUM ="));
        // Place them where readers expect: right after the structural block.
        let at = recs
            .iter()
            .position(|r| !is_structural_record(r))
            .unwrap_or(recs.len());
        recs.insert(
            at,
            card(&format!("DATASUM = '{ds:<10}' / data unit checksum")),
        );
        recs.insert(at, card("CHECKSUM= '0000000000000000' / HDU checksum"));
    }
    recs.push(card("END"));
    let mut bytes: Vec<u8> = recs.concat().into_bytes();
    bytes.resize(
        (bytes.len() as u64).div_ceil(BLOCK_LEN) as usize * BLOCK_LEN as usize,
        b' ',
    );
    let enc = sum.and_then(|ds| checksum::seal(&mut bytes, ds));
    (bytes, enc)
}

fn is_structural_record(r: &str) -> bool {
    let k = r[..8].trim_end();
    k == "SIMPLE"
        || k == "XTENSION"
        || k == "BITPIX"
        || k.starts_with("NAXIS")
        || k == "PCOUNT"
        || k == "GCOUNT"
        || k == "EXTEND"
}

fn backup_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".bak");
    PathBuf::from(s)
}

/// Plan and apply `ops` to one file. Validation errors write nothing.
pub fn apply(path: impl AsRef<Path>, ops: &[Op], opts: &Options) -> Result<WriteReport, EditError> {
    let path = path.as_ref();
    let fits = Fits::open(path)?;
    let plan = plan_for(&fits, &path.to_string_lossy(), ops, opts)?;
    let hdu = &fits.hdus[plan.hdu];
    let (h0, h1) = (hdu.header_offset, hdu.data_offset);
    let before = outside_header(path, h0, h1)?;
    if plan.changes.is_empty() {
        return Ok(WriteReport {
            plan,
            method: Method::None,
            backup: None,
            untouched_sha256: before,
            checksum: None,
        });
    }

    let has_sum = hdu.header().get("CHECKSUM").is_some() || hdu.header().get("DATASUM").is_some();
    let sum = if opts.checksum && has_sum {
        Some(data_sum(path, h1, hdu.data_padded())?)
    } else {
        None
    };
    let (bytes, enc) = header_bytes(&plan, sum);
    let new_blocks = bytes.len() as u64 / BLOCK_LEN;

    let backup = if opts.backup {
        let b = backup_path(path);
        if !b.exists() {
            fs::copy(path, &b)?;
        }
        Some(b.to_string_lossy().to_string())
    } else {
        None
    };

    let method = if new_blocks == hdu.header_blocks {
        let mut old = vec![0u8; bytes.len()];
        let mut f = OpenOptions::new().read(true).write(true).open(path)?;
        f.seek(SeekFrom::Start(h0))?;
        f.read_exact(&mut old)?;
        f.seek(SeekFrom::Start(h0))?;
        f.write_all(&bytes)?;
        f.sync_all()?;
        drop(f);
        if outside_header(path, h0, h1)? != before {
            // Cannot happen short of a storage fault; put the old header back.
            let mut f = OpenOptions::new().write(true).open(path)?;
            f.seek(SeekFrom::Start(h0))?;
            f.write_all(&old)?;
            f.sync_all()?;
            return Err(EditError::Unsafe(format!(
                "{}: bytes outside the header changed; original header restored",
                path.display()
            )));
        }
        Method::InPlace
    } else {
        let dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let name = path
            .file_name()
            .map_or_else(|| "file".into(), |n| n.to_string_lossy().to_string());
        let tmp = dir.join(format!(".{name}.fittle-{}.tmp", std::process::id()));
        let result = (|| -> Result<(), EditError> {
            let mut src = File::open(path)?;
            let len = src.metadata()?.len();
            let mut out = File::create(&tmp)?;
            copy_range(&mut src, 0, h0, &mut out)?;
            out.write_all(&bytes)?;
            copy_range(&mut src, h1, len, &mut out)?;
            out.sync_all()?;
            drop(out);
            fs::set_permissions(&tmp, src.metadata()?.permissions())?;
            // Verify the new file before it replaces the original.
            if outside_header(&tmp, h0, h0 + bytes.len() as u64)? != before {
                return Err(EditError::Unsafe(format!(
                    "{}: bytes outside the header would change; nothing written",
                    path.display()
                )));
            }
            fs::rename(&tmp, path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result?;
        Method::Rewrite
    };

    verify(path, &plan, &before)?;
    Ok(WriteReport {
        plan,
        method,
        backup,
        untouched_sha256: before,
        checksum: enc,
    })
}

/// Re-read the written file: structure intact, bytes outside the header
/// unchanged, and each change present.
fn verify(path: &Path, plan: &Plan, before: &str) -> Result<(), EditError> {
    let fits = Fits::open(path)?;
    if fits
        .issues
        .iter()
        .any(|i| i.severity == crate::Severity::Error)
    {
        return Err(EditError::Unsafe(format!(
            "{}: written file has structural errors",
            path.display()
        )));
    }
    let hdu = fits
        .hdus
        .get(plan.hdu)
        .ok_or_else(|| EditError::Unsafe("edited HDU missing after write".into()))?;
    if outside_header(path, hdu.header_offset, hdu.data_offset)? != before {
        return Err(EditError::Unsafe(format!(
            "{}: bytes outside the header changed",
            path.display()
        )));
    }
    // The net effect per keyword, in order (a key set then removed ends absent).
    let mut expect: Vec<(String, bool)> = Vec::new();
    let mut mark = |k: &str, present: bool| match expect.iter_mut().find(|(x, _)| x == k) {
        Some(e) => e.1 = present,
        None => expect.push((k.to_string(), present)),
    };
    for c in &plan.changes {
        match c.kind {
            ChangeKind::Removed => mark(&c.key, false),
            ChangeKind::Added | ChangeKind::Modified => mark(&c.key, true),
            ChangeKind::Renamed => {
                if let Some((from, to)) = c.key.split_once(" → ") {
                    mark(from, false);
                    mark(to, true);
                }
            }
            ChangeKind::History => {}
        }
    }
    let h = hdu.header();
    for (key, present) in expect {
        if h.get(&key).is_some() != present {
            return Err(EditError::Unsafe(format!(
                "{}: {key} should be {} after the write",
                path.display(),
                if present { "present" } else { "absent" }
            )));
        }
    }
    Ok(())
}

/// Verify an HDU's CHECKSUM: `Some(true)` if the HDU sums to -0, `None` if
/// it carries no CHECKSUM.
pub fn verify_checksum(path: impl AsRef<Path>, hdu: usize) -> Result<Option<bool>, EditError> {
    let path = path.as_ref();
    let fits = Fits::open(path)?;
    let h = fits
        .hdus
        .get(hdu)
        .ok_or_else(|| EditError::Invalid(format!("no HDU {hdu}")))?;
    if h.header().get("CHECKSUM").is_none() {
        return Ok(None);
    }
    let head = data_sum(path, h.header_offset, h.data_offset - h.header_offset)?;
    let data = data_sum(path, h.data_offset, h.data_padded())?;
    Ok(Some(checksum::add(head, data) == 0xFFFF_FFFF))
}
