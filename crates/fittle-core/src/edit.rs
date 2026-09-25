//! Header edits: operations, validation, card formatting and the staged plan.
//!
//! A plan is computed from the file as it is on disk and never touches it;
//! `write::apply` performs a plan. Structural keywords are locked, keywords
//! and values are validated against the FITS standard, and every change is
//! reported as a before/after pair with its plain-language consequences.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::card::{CARD_LEN, Card, Value};
use crate::dict;
use crate::fits::{BLOCK_LEN, Fits, HduKind};

/// A value to write. `Auto` is text from a user (`KEY=VALUE`) whose type is
/// inferred: T/F, integer, float, else string.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum NewValue {
    Logical(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Auto(String),
}

impl NewValue {
    /// Resolve `Auto` to a concrete type.
    pub fn resolve(self) -> NewValue {
        let NewValue::Auto(t) = self else { return self };
        let s = t.trim();
        if let Some(inner) = s.strip_prefix('\'').and_then(|x| x.strip_suffix('\'')) {
            return NewValue::String(inner.to_string());
        }
        match s {
            "T" => return NewValue::Logical(true),
            "F" => return NewValue::Logical(false),
            _ => {}
        }
        if let Ok(i) = s.parse::<i64>() {
            return NewValue::Integer(i);
        }
        let numeric = s
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'+' | b'e' | b'E'));
        if numeric {
            if let Ok(f) = s.parse::<f64>() {
                if f.is_finite() {
                    return NewValue::Float(f);
                }
            }
        }
        NewValue::String(t)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    /// Set a keyword's value (adding it if absent). `comment: None` keeps the
    /// existing comment.
    Set {
        key: String,
        value: NewValue,
        comment: Option<String>,
    },
    /// Remove every value card with this keyword.
    Unset { key: String },
    /// Rename a keyword, keeping its value and comment.
    Rename { from: String, to: String },
    /// Append a HISTORY record (wrapped to 72 characters).
    History { text: String },
}

#[derive(Debug, thiserror::Error)]
pub enum EditError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Fits(#[from] crate::fits::Error),
    /// The edit breaks a rule; nothing was written.
    #[error("{0}")]
    Invalid(String),
    /// A write happened but verification failed; the original was restored.
    #[error("{0}")]
    Unsafe(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Modified,
    Removed,
    Renamed,
    History,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Change {
    pub kind: ChangeKind,
    pub key: String,
    /// Value text as it reads in the header (`250.0`, `'LP'`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

/// What an edit will do to one file, before anything is written.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Plan {
    pub path: String,
    pub hdu: usize,
    pub changes: Vec<Change>,
    pub header_blocks_before: u64,
    pub header_blocks_after: u64,
    /// Same block count: the header is rewritten in place.
    pub in_place: bool,
    /// Plain-language consequences ("disables altitude and moon…").
    pub consequences: Vec<String>,
    pub warnings: Vec<String>,
    /// The new header records (80 chars each, no END), in order.
    #[serde(skip)]
    pub records: Vec<String>,
}

/// Options for applying a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Options {
    /// Copy the original to `<file>.bak` if no backup exists yet.
    pub backup: bool,
    /// Append a `HISTORY Fittle …` line describing the change.
    pub history: bool,
    /// Recompute CHECKSUM / DATASUM when the HDU carries them.
    pub checksum: bool,
    /// HDU to edit (default: the image HDU, else the primary).
    pub hdu: Option<usize>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            backup: true,
            history: true,
            checksum: true,
            hdu: None,
        }
    }
}

// ---- validation -----------------------------------------------------------

/// Keywords that are never edited: layout (dict structural), scaling that
/// changes what the pixel bytes mean, and keys Fittle manages itself.
pub fn is_locked(key: &str, kind: &HduKind) -> Option<&'static str> {
    const MANAGED: [(&str, &str); 7] = [
        ("BZERO", "changes what every stored pixel value means"),
        ("BSCALE", "changes what every stored pixel value means"),
        ("BLANK", "changes which pixels count as undefined"),
        ("CHECKSUM", "is maintained by Fittle on write"),
        ("DATASUM", "is maintained by Fittle on write"),
        ("CONTINUE", "is part of a long-string value"),
        ("END", "ends the header"),
    ];
    if let Some((_, why)) = MANAGED.iter().find(|(k, _)| *k == key) {
        return Some(why);
    }
    if dict::is_structural(key) {
        return Some("describes the data layout");
    }
    let base = key.trim_end_matches(|c: char| c.is_ascii_digit());
    if matches!(
        base,
        "TFORM" | "TTYPE" | "TBCOL" | "TDIM" | "TFIELDS" | "THEAP" | "TZERO" | "TSCAL" | "TNULL"
    ) {
        return Some("describes a table column");
    }
    if *kind == HduKind::CompressedImage && key.starts_with('Z') {
        return Some("is part of the tile-compression description");
    }
    None
}

/// 8 characters or fewer, A–Z 0–9 `-` `_`.
pub fn check_keyword(key: &str) -> Result<(), EditError> {
    let ok = !key.is_empty()
        && key.len() <= 8
        && key
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
    if ok {
        Ok(())
    } else {
        Err(EditError::Invalid(format!(
            "{key:?} is not a valid FITS keyword (1–8 characters, A–Z 0–9 - _)"
        )))
    }
}

fn printable(s: &str) -> bool {
    s.bytes().all(|b| (0x20..=0x7E).contains(&b))
}

// ---- formatting -----------------------------------------------------------

/// Longest string content in one card (80 − 10 for `KEY     = ` − 2 quotes).
pub const MAX_STRING_IN_CARD: usize = 68;

fn float_text(f: f64) -> String {
    let d = format!("{f:?}");
    let t = if d.contains('e') {
        d.replace('e', "E")
    } else {
        d
    };
    if t.len() <= 20 {
        t
    } else {
        format!("{f:.12E}")
    }
}

/// Value text as written in columns 11–30 (right-justified numbers) or a
/// quoted string. Strings over one card return several pieces for CONTINUE.
fn value_text(v: &NewValue) -> Vec<String> {
    match v {
        NewValue::Logical(b) => vec![format!("{:>20}", if *b { "T" } else { "F" })],
        NewValue::Integer(i) => vec![format!("{i:>20}")],
        NewValue::Float(f) => vec![format!("{:>20}", float_text(*f))],
        NewValue::String(s) => {
            let esc = s.replace('\'', "''");
            if esc.len() <= MAX_STRING_IN_CARD {
                return vec![format!("'{esc:<8}'")];
            }
            // Long-string convention: pieces end with '&', continued by
            // CONTINUE. Split the original text (never inside an escaped
            // quote pair), then escape each piece.
            let mut pieces = Vec::new();
            let mut cur = String::new();
            for ch in s.chars() {
                let w = if ch == '\'' { 2 } else { 1 };
                if cur.len() + w > MAX_STRING_IN_CARD - 1 {
                    pieces.push(format!("'{cur}&'"));
                    cur.clear();
                }
                if ch == '\'' {
                    cur.push_str("''");
                } else {
                    cur.push(ch);
                }
            }
            pieces.push(format!("'{cur}'"));
            pieces
        }
        NewValue::Auto(_) => value_text(&v.clone().resolve()),
    }
}

/// Format a keyword record (plus CONTINUE records for long strings).
pub fn format_card(key: &str, value: &NewValue, comment: Option<&str>) -> Vec<String> {
    let pieces = value_text(value);
    let n = pieces.len();
    pieces
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            let head = if i == 0 {
                format!("{key:<8}= {text}")
            } else {
                format!("CONTINUE  {text}")
            };
            let line = match comment.filter(|c| !c.is_empty() && i == n - 1) {
                Some(c) if head.len() + 3 < CARD_LEN => {
                    let room = CARD_LEN - head.len() - 3;
                    format!("{head} / {}", &c[..c.len().min(room)])
                }
                _ => head,
            };
            format!("{line:<80}")
        })
        .collect()
}

/// A readable value for diffs (`250.0`, `'LP'`, `T`).
fn show(v: &Value, text: &str) -> String {
    match v {
        Value::String(s) => format!("'{s}'"),
        Value::Logical(b) => if *b { "T" } else { "F" }.into(),
        _ => text.trim().to_string(),
    }
}

fn show_new(v: &NewValue) -> String {
    match v {
        NewValue::String(s) => format!("'{s}'"),
        NewValue::Logical(b) => if *b { "T" } else { "F" }.into(),
        NewValue::Integer(i) => i.to_string(),
        NewValue::Float(f) => float_text(*f),
        NewValue::Auto(_) => show_new(&v.clone().resolve()),
    }
}

/// Wrap text into HISTORY records.
pub fn history_cards(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().filter(|c| (' '..='~').contains(c)).collect();
    chars
        .chunks(72)
        .map(|c| {
            format!(
                "{:<80}",
                format!("HISTORY {}", c.iter().collect::<String>())
            )
        })
        .collect()
}

// ---- planning -------------------------------------------------------------

/// Keys whose change has a consequence worth saying before the write.
fn consequence(key: &str, removed: bool) -> Option<&'static str> {
    Some(match key {
        "SITELAT" | "SITELONG" | "OBSGEO-B" | "OBSGEO-L" | "LATITUDE" | "LONGITUD" if removed => {
            "Removing site coordinates disables altitude, airmass and moon calculations for this file."
        }
        "DATE-OBS" if removed => {
            "Removing DATE-OBS disables altitude, moon and session-night calculations."
        }
        "FOCALLEN" | "XPIXSZ" | "YPIXSZ" | "XBINNING" => {
            "Changes the pixel scale and field of view Fittle derives."
        }
        "GAIN" | "OFFSET" | "EXPTIME" | "EXPOSURE" | "CCD-TEMP" | "SET-TEMP" | "FILTER"
        | "INSTRUME" | "READOUTM" => "Changes which calibration frames this file matches.",
        "IMAGETYP" | "FRAME" | "STACKCNT" | "NCOMBINE" | "LIVETIME" => {
            "Changes how Fittle classifies this file."
        }
        "OBJECT" => "Changes the target name other tools use to group this file.",
        "INSTRUME" | "CREATOR" | "TELESCOP" | "SWCREATE" => {
            "Changes which scope or app Fittle identifies. Smart scopes repeat the model in several keys \
             (Seestar: INSTRUME, CREATOR and the TELESCOP serial); change them together or Fittle reports a conflict."
        }
        "BAYERPAT" | "XBAYROFF" | "YBAYROFF" | "ROWORDER" => {
            "Changes how colour is reconstructed (debayering) and the image orientation."
        }
        _ => return None,
    })
}

/// Default HDU to edit: the image HDU, else the primary.
pub fn default_hdu(fits: &Fits) -> usize {
    fits.hdus
        .iter()
        .find(|h| !h.shape.is_empty())
        .map_or(0, |h| h.index)
}

/// Compute what `ops` would do to `path`. Reads headers only.
pub fn plan(path: impl AsRef<Path>, ops: &[Op], opts: &Options) -> Result<Plan, EditError> {
    let path = path.as_ref();
    let fits = Fits::open(path)?;
    plan_for(&fits, &path.to_string_lossy(), ops, opts)
}

pub fn plan_for(fits: &Fits, path: &str, ops: &[Op], opts: &Options) -> Result<Plan, EditError> {
    if fits
        .issues
        .iter()
        .any(|i| i.severity == crate::Severity::Error)
    {
        return Err(EditError::Invalid(format!(
            "{path} has structural errors ({}); refusing to edit",
            fits.issues
                .iter()
                .filter(|i| i.severity == crate::Severity::Error)
                .map(|i| i.code.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    let index = opts.hdu.unwrap_or_else(|| default_hdu(fits));
    let hdu = fits
        .hdus
        .get(index)
        .ok_or_else(|| EditError::Invalid(format!("no HDU {index} in {path}")))?;

    // Working copy: logical cards with their records.
    let mut cards: Vec<(Card, Vec<String>)> = hdu
        .header()
        .cards
        .iter()
        .map(|c| (c.clone(), c.raw.clone()))
        .collect();
    let mut changes = Vec::new();
    let mut warnings = Vec::new();
    let mut consequences: Vec<String> = Vec::new();
    let note = |key: &str, removed: bool, list: &mut Vec<String>| {
        if let Some(c) = consequence(key, removed) {
            if !list.iter().any(|x| x == c) {
                list.push(c.to_string());
            }
        }
    };
    let is_value = |c: &Card| !c.value.is_commentary();

    for op in ops {
        match op {
            Op::Set {
                key,
                value,
                comment,
            } => {
                let key = key.trim().to_ascii_uppercase();
                check_keyword(&key)?;
                if let Some(why) = is_locked(&key, &hdu.kind) {
                    return Err(EditError::Invalid(format!("{key} is locked: it {why}")));
                }
                let value = value.clone().resolve();
                if let NewValue::String(s) = &value {
                    if !printable(s) {
                        return Err(EditError::Invalid(format!(
                            "{key}: value must be printable ASCII"
                        )));
                    }
                }
                if let Some(c) = comment {
                    if !printable(c) {
                        return Err(EditError::Invalid(format!(
                            "{key}: comment must be printable ASCII"
                        )));
                    }
                }
                let matches: Vec<usize> = cards
                    .iter()
                    .enumerate()
                    .filter(|(_, (c, _))| is_value(c) && c.keyword == key)
                    .map(|(i, _)| i)
                    .collect();
                if matches.len() > 1 {
                    warnings.push(format!(
                        "{key} appears {} times; updating the first",
                        matches.len()
                    ));
                }
                match matches.first() {
                    Some(&i) => {
                        let old = &cards[i].0;
                        let keep_comment = comment.clone().or_else(|| old.comment.clone());
                        let before = show(&old.value, &old.value_text);
                        let after = show_new(&value);
                        if before == after && comment.is_none() {
                            continue;
                        }
                        cards[i].1 = format_card(&key, &value, keep_comment.as_deref());
                        changes.push(Change {
                            kind: ChangeKind::Modified,
                            key: key.clone(),
                            before: Some(before),
                            after: Some(after),
                        });
                    }
                    None => {
                        // Insert after the last value card, before HISTORY/COMMENT blocks.
                        let at = cards
                            .iter()
                            .rposition(|(c, _)| is_value(c))
                            .map_or(0, |i| i + 1);
                        let recs = format_card(&key, &value, comment.as_deref());
                        let card = Card {
                            keyword: key.clone(),
                            value: Value::Undefined,
                            comment: None,
                            hierarch: false,
                            record: 0,
                            raw: recs.clone(),
                            value_text: String::new(),
                        };
                        cards.insert(at, (card, recs));
                        changes.push(Change {
                            kind: ChangeKind::Added,
                            key: key.clone(),
                            before: None,
                            after: Some(show_new(&value)),
                        });
                    }
                }
                note(&key, false, &mut consequences);
            }
            Op::Unset { key } => {
                let key = key.trim().to_ascii_uppercase();
                if let Some(why) = is_locked(&key, &hdu.kind) {
                    return Err(EditError::Invalid(format!("{key} is locked: it {why}")));
                }
                let before: Vec<String> = cards
                    .iter()
                    .filter(|(c, _)| is_value(c) && c.keyword == key)
                    .map(|(c, _)| show(&c.value, &c.value_text))
                    .collect();
                if before.is_empty() {
                    warnings.push(format!("{key} is not in the header"));
                    continue;
                }
                cards.retain(|(c, _)| !(is_value(c) && c.keyword == key));
                for b in before {
                    changes.push(Change {
                        kind: ChangeKind::Removed,
                        key: key.clone(),
                        before: Some(b),
                        after: None,
                    });
                }
                note(&key, true, &mut consequences);
            }
            Op::Rename { from, to } => {
                let (from, to) = (
                    from.trim().to_ascii_uppercase(),
                    to.trim().to_ascii_uppercase(),
                );
                check_keyword(&to)?;
                for k in [&from, &to] {
                    if let Some(why) = is_locked(k, &hdu.kind) {
                        return Err(EditError::Invalid(format!("{k} is locked: it {why}")));
                    }
                }
                if cards.iter().any(|(c, _)| is_value(c) && c.keyword == to) {
                    return Err(EditError::Invalid(format!(
                        "cannot rename {from} to {to}: {to} already exists"
                    )));
                }
                let Some(i) = cards
                    .iter()
                    .position(|(c, _)| is_value(c) && c.keyword == from)
                else {
                    warnings.push(format!("{from} is not in the header"));
                    continue;
                };
                let old = cards[i].0.clone();
                // Swap only the keyword field; the value and comment stay byte for byte.
                if let Some(first) = cards[i].1.first_mut() {
                    *first = format!("{to:<8}{}", &first[8..]);
                }
                cards[i].0.keyword = to.clone();
                changes.push(Change {
                    kind: ChangeKind::Renamed,
                    key: format!("{from} → {to}"),
                    before: Some(format!("{from} = {}", show(&old.value, &old.value_text))),
                    after: Some(format!("{to} = {}", show(&old.value, &old.value_text))),
                });
                note(&from, true, &mut consequences);
            }
            Op::History { text } => {
                let recs = history_cards(text);
                let card = Card {
                    keyword: "HISTORY".into(),
                    value: Value::Commentary(text.clone()),
                    comment: None,
                    hierarch: false,
                    record: 0,
                    raw: recs.clone(),
                    value_text: text.clone(),
                };
                cards.push((card, recs));
                changes.push(Change {
                    kind: ChangeKind::History,
                    key: "HISTORY".into(),
                    before: None,
                    after: Some(text.clone()),
                });
            }
        }
    }

    // Audit line, only when something actually changes.
    if opts.history && changes.iter().any(|c| c.kind != ChangeKind::History) {
        let summary: Vec<String> = changes
            .iter()
            .filter(|c| c.kind != ChangeKind::History)
            .map(|c| match c.kind {
                ChangeKind::Added => format!("add {}={}", c.key, c.after.as_deref().unwrap_or("")),
                ChangeKind::Removed => format!("remove {}", c.key),
                ChangeKind::Renamed => format!("rename {}", c.key.replace(" → ", "->")),
                _ => format!(
                    "set {} {}->{}",
                    c.key,
                    c.before.as_deref().unwrap_or(""),
                    c.after.as_deref().unwrap_or("")
                ),
            })
            .collect();
        let text = format!(
            "Fittle {}: {}",
            env!("CARGO_PKG_VERSION"),
            summary.join("; ")
        );
        for line in history_cards(&text) {
            let card = Card {
                keyword: "HISTORY".into(),
                value: Value::Commentary(String::new()),
                comment: None,
                hierarch: false,
                record: 0,
                raw: vec![line.clone()],
                value_text: String::new(),
            };
            cards.push((card, vec![line]));
        }
    }

    let records: Vec<String> = cards.into_iter().flat_map(|(_, r)| r).collect();
    let blocks_after = ((records.len() + 1) * CARD_LEN).div_ceil(BLOCK_LEN as usize) as u64;
    Ok(Plan {
        path: path.to_string(),
        hdu: index,
        changes,
        header_blocks_before: hdu.header_blocks,
        header_blocks_after: blocks_after,
        in_place: blocks_after == hdu.header_blocks,
        consequences,
        warnings,
        records,
    })
}

/// Parse `KEY=VALUE` (value type inferred; quote with '…' to force a string).
pub fn parse_assignment(s: &str) -> Option<(String, NewValue)> {
    let (k, v) = s.split_once('=')?;
    let k = k.trim().to_ascii_uppercase();
    check_keyword(&k).ok()?;
    Some((k, NewValue::Auto(v.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::HeaderBuilder;

    fn reparse(recs: &[String]) -> crate::Header {
        let mut b = HeaderBuilder::default();
        for r in recs {
            b.push(r.as_bytes());
        }
        b.finish().0
    }

    #[test]
    fn inference() {
        assert_eq!(
            NewValue::Auto("T".into()).resolve(),
            NewValue::Logical(true)
        );
        assert_eq!(
            NewValue::Auto("250".into()).resolve(),
            NewValue::Integer(250)
        );
        assert_eq!(
            NewValue::Auto("250.0".into()).resolve(),
            NewValue::Float(250.0)
        );
        assert_eq!(
            NewValue::Auto("-1.5e-3".into()).resolve(),
            NewValue::Float(-0.0015)
        );
        assert_eq!(
            NewValue::Auto("NGC 6995".into()).resolve(),
            NewValue::String("NGC 6995".into())
        );
        assert_eq!(
            NewValue::Auto("'42'".into()).resolve(),
            NewValue::String("42".into())
        );
        assert_eq!(
            NewValue::Auto("inf".into()).resolve(),
            NewValue::String("inf".into())
        );
    }

    #[test]
    fn cards_round_trip() {
        for (v, comment) in [
            (NewValue::Float(250.0), Some("[mm] focal length")),
            (NewValue::Float(2.9e-7), None),
            (NewValue::Integer(-32768), None),
            (NewValue::Logical(false), Some("x")),
            (NewValue::String("O'Brien".into()), None),
            (NewValue::String("x".repeat(150)), Some("long")),
            // Fuzz finds: never split inside an escaped '' pair, and trailing
            // spaces at a CONTINUE boundary are not significant.
            (
                NewValue::String("'2.''''''*''''''''''''''''''''>'''''''''".into()),
                Some("q"),
            ),
            (NewValue::String("'".repeat(70)), None),
            (
                NewValue::String(format!("{}'{}", "a".repeat(66), "b".repeat(80))),
                None,
            ),
            (
                NewValue::String(format!("2NN{}2NN{}", " ".repeat(60), " ".repeat(9))),
                None,
            ),
        ] {
            let recs = format_card("TESTKEY", &v, comment);
            assert!(recs.iter().all(|r| r.len() == 80), "{recs:?}");
            let h = reparse(&recs);
            let got = &h.cards[0];
            match (&v, &got.value) {
                (NewValue::Float(a), Value::Float(b)) => assert_eq!(a, b),
                (NewValue::Integer(a), Value::Integer(b)) => assert_eq!(a, b),
                (NewValue::Logical(a), Value::Logical(b)) => assert_eq!(a, b),
                (NewValue::String(a), Value::String(b)) => assert_eq!(a.trim_end(), b),
                other => panic!("{other:?}"),
            }
            assert_eq!(got.comment.as_deref(), comment);
        }
    }

    #[test]
    fn keywords_and_locks() {
        assert!(check_keyword("FOCALLEN").is_ok());
        assert!(check_keyword("CCD-TEMP").is_ok());
        assert!(check_keyword("TOOLONGKEY").is_err());
        assert!(check_keyword("lower").is_err());
        assert!(is_locked("NAXIS1", &HduKind::Primary).is_some());
        assert!(is_locked("BZERO", &HduKind::Primary).is_some());
        assert!(is_locked("ZCMPTYPE", &HduKind::CompressedImage).is_some());
        assert!(is_locked("OBJECT", &HduKind::Primary).is_none());
    }

    #[test]
    fn assignments() {
        assert_eq!(
            parse_assignment("focallen=250"),
            Some(("FOCALLEN".into(), NewValue::Auto("250".into())))
        );
        assert_eq!(
            parse_assignment("OBJECT=M 31").map(|x| x.0),
            Some("OBJECT".into())
        );
        assert!(parse_assignment("not a key=1").is_none());
        assert!(parse_assignment("file.fit").is_none());
    }
}
