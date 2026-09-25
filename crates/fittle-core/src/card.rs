//! Single 80-character header records and their values.

use serde::Serialize;

/// Length of one header record in bytes.
pub const CARD_LEN: usize = 80;

/// A parsed keyword value. Commentary records (`HISTORY`, `COMMENT`, blank or
/// any keyword without a value indicator) carry their text as `Commentary`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Value {
    Logical(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Complex([f64; 2]),
    /// Value field is present but empty.
    Undefined,
    /// Value field could not be parsed (e.g. an unquoted string). Holds the raw text.
    Unparsed(String),
    Commentary(String),
}

impl Value {
    pub fn as_f64(&self) -> Option<f64> {
        match *self {
            Value::Integer(i) => Some(i as f64),
            Value::Float(f) => Some(f),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match *self {
            Value::Integer(i) => Some(i),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match *self {
            Value::Logical(b) => Some(b),
            _ => None,
        }
    }

    pub fn is_commentary(&self) -> bool {
        matches!(self, Value::Commentary(_))
    }
}

/// One logical keyword record. Usually one 80-char line; long strings using the
/// `CONTINUE` convention span several, all kept in `raw` in file order.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Card {
    pub keyword: String,
    #[serde(flatten)]
    pub value: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Keyword was written with the ESO `HIERARCH` convention.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub hierarch: bool,
    /// Zero-based index of the first raw record within the header.
    pub record: usize,
    /// Exact 80-char records as they appear in the file.
    pub raw: Vec<String>,
    /// Value text as written (numbers keep their original formatting).
    #[serde(skip)]
    pub value_text: String,
}

/// A problem noticed while parsing one record. Collected into file issues.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CardProblem {
    pub code: &'static str,
    pub message: String,
}

pub(crate) enum Parsed {
    Card(Card, Vec<CardProblem>),
    Continue {
        value: Option<String>,
        comment: Option<String>,
        raw: String,
    },
    End,
}

const COMMENTARY: [&str; 3] = ["COMMENT", "HISTORY", ""];

/// Parse one 80-byte record.
pub(crate) fn parse_record(bytes: &[u8], record: usize) -> Parsed {
    debug_assert_eq!(bytes.len(), CARD_LEN);
    let mut problems = Vec::new();
    let raw: String = if bytes.iter().all(|b| (0x20..=0x7E).contains(b)) {
        // All printable ASCII, so this is valid UTF-8.
        String::from_utf8_lossy(bytes).into_owned()
    } else {
        problems.push(CardProblem {
            code: "non_ascii",
            message: "record contains bytes outside printable ASCII".into(),
        });
        bytes
            .iter()
            .map(|&b| {
                if (0x20..=0x7E).contains(&b) {
                    b as char
                } else {
                    '?'
                }
            })
            .collect()
    };

    let name = raw[..8].trim_end();
    if name == "END" && raw[8..].trim().is_empty() {
        return Parsed::End;
    }

    if name == "CONTINUE" && &raw[8..10] != "= " {
        let (value, comment) = match parse_value_field(&raw[10..]) {
            (Value::String(s), c, _) => (Some(s), c),
            _ => (None, None),
        };
        return Parsed::Continue {
            value,
            comment,
            raw,
        };
    }

    if name == "HIERARCH" {
        if let Some(eq) = raw[8..].find('=') {
            let key = raw[8..8 + eq].trim().to_string();
            let (value, comment, text) = parse_value_field(&raw[8 + eq + 1..]);
            if let Value::Unparsed(ref s) = value {
                problems.push(bad_value(&key, s));
            }
            let card = Card {
                keyword: key,
                value,
                comment,
                hierarch: true,
                record,
                raw: vec![raw],
                value_text: text,
            };
            return Parsed::Card(card, problems);
        }
    }

    if !name
        .bytes()
        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        || raw[..8].trim_end().contains(' ')
    {
        problems.push(CardProblem {
            code: "invalid_keyword",
            message: format!("keyword {name:?} has characters outside A-Z 0-9 - _"),
        });
    }

    let keyword = name.to_string();
    let card = if &raw[8..10] == "= " && !COMMENTARY.contains(&name) {
        let (value, comment, text) = parse_value_field(&raw[10..]);
        if let Value::Unparsed(ref s) = value {
            problems.push(bad_value(&keyword, s));
        }
        Card {
            keyword,
            value,
            comment,
            hierarch: false,
            record,
            raw: vec![raw],
            value_text: text,
        }
    } else {
        let text = raw[8..].trim_end().to_string();
        Card {
            keyword,
            value: Value::Commentary(text.clone()),
            comment: None,
            hierarch: false,
            record,
            raw: vec![raw],
            value_text: text,
        }
    };
    Parsed::Card(card, problems)
}

fn bad_value(key: &str, s: &str) -> CardProblem {
    CardProblem {
        code: "invalid_value",
        message: format!("{key}: value {s:?} is not a valid FITS value"),
    }
}

/// Parse the value/comment field (columns 11-80). Returns the value, the
/// comment, and the value text as written.
pub(crate) fn parse_value_field(field: &str) -> (Value, Option<String>, String) {
    let s = field.trim_start();
    if let Some(rest) = s.strip_prefix('\'') {
        let mut out = String::new();
        let mut chars = rest.char_indices().peekable();
        let mut end = None;
        while let Some((i, c)) = chars.next() {
            if c == '\'' {
                if matches!(chars.peek(), Some((_, '\''))) {
                    chars.next();
                    out.push('\'');
                } else {
                    end = Some(i + 1);
                    break;
                }
            } else {
                out.push(c);
            }
        }
        let Some(end) = end else {
            return (
                Value::Unparsed(s.trim_end().to_string()),
                None,
                s.trim_end().to_string(),
            );
        };
        let text = format!("'{}", &rest[..end]);
        let after = &rest[end..];
        // Trailing spaces inside a string are not significant; leading ones are.
        let value = Value::String(out.trim_end().to_string());
        return (value, comment_of(after), text);
    }

    let (token, comment) = match s.find('/') {
        Some(i) => (s[..i].trim(), comment_of(&s[i..])),
        None => (s.trim(), None),
    };
    (parse_scalar(token), comment, token.to_string())
}

fn comment_of(after_value: &str) -> Option<String> {
    let t = after_value.trim_start();
    let c = t.strip_prefix('/')?.trim();
    (!c.is_empty()).then(|| c.to_string())
}

fn parse_scalar(token: &str) -> Value {
    match token {
        "" => return Value::Undefined,
        "T" => return Value::Logical(true),
        "F" => return Value::Logical(false),
        _ => {}
    }
    if let Some(inner) = token.strip_prefix('(').and_then(|t| t.strip_suffix(')')) {
        let parts: Vec<_> = inner.split(',').map(str::trim).collect();
        if let [re, im] = parts[..] {
            if let (Some(re), Some(im)) = (parse_float(re), parse_float(im)) {
                return Value::Complex([re, im]);
            }
        }
        return Value::Unparsed(token.to_string());
    }
    if is_integer(token) {
        return match token.parse::<i64>() {
            Ok(i) => Value::Integer(i),
            Err(_) => parse_float(token).map_or(Value::Unparsed(token.to_string()), Value::Float),
        };
    }
    parse_float(token).map_or(Value::Unparsed(token.to_string()), Value::Float)
}

fn is_integer(t: &str) -> bool {
    let d = t.strip_prefix(['+', '-']).unwrap_or(t);
    !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit())
}

/// FITS fixed or exponential float; `D` exponents are accepted.
fn parse_float(t: &str) -> Option<f64> {
    let body = t.strip_prefix(['+', '-']).unwrap_or(t);
    let (mantissa, exp) = match body.find(['E', 'e', 'D', 'd']) {
        Some(i) => (&body[..i], Some(&body[i + 1..])),
        None => (body, None),
    };
    let digits = mantissa.replacen('.', "", 1);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if let Some(e) = exp {
        if !is_integer(e) {
            return None;
        }
    }
    t.replace(['D', 'd'], "E").parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(s: &str) -> Vec<u8> {
        format!("{s:<80}").into_bytes()
    }

    fn card(s: &str) -> Card {
        match parse_record(&rec(s), 0) {
            Parsed::Card(c, _) => c,
            _ => panic!("not a card"),
        }
    }

    #[test]
    fn values() {
        assert_eq!(
            card("SIMPLE  =                    T / conforms").value,
            Value::Logical(true)
        );
        assert_eq!(
            card("BITPIX  =                  -32").value,
            Value::Integer(-32)
        );
        assert_eq!(
            card("EXPTIME =                 20.0 / [s]").value,
            Value::Float(20.0)
        );
        assert_eq!(
            card("EXPTIME =              2.5D+01").value,
            Value::Float(25.0)
        );
        assert_eq!(
            card("CPLX    = (1.5, -2)").value,
            Value::Complex([1.5, -2.0])
        );
        assert_eq!(card("BLANKV  =").value, Value::Undefined);
        assert_eq!(
            card("OBJECT  = M31 / oops").value,
            Value::Unparsed("M31".into())
        );
        assert_eq!(
            card("EXPTIME =                 20.0 / [s] exposure")
                .comment
                .as_deref(),
            Some("[s] exposure")
        );
    }

    #[test]
    fn strings() {
        let c = card("OBJECT  = 'NGC 6995 / Veil'    / target");
        assert_eq!(c.value, Value::String("NGC 6995 / Veil".into()));
        assert_eq!(c.comment.as_deref(), Some("target"));
        assert_eq!(
            card("OBSERVER= 'O''Brien '").value,
            Value::String("O'Brien".into())
        );
        assert_eq!(card("EMPTY   = ''").value, Value::String(String::new()));
        assert_eq!(card("LEAD    = '  x'").value, Value::String("  x".into()));
    }

    #[test]
    fn commentary() {
        let c = card("HISTORY Siril stack: 212 images");
        assert_eq!(c.value, Value::Commentary("Siril stack: 212 images".into()));
        let c = card("COMMENT = not a value");
        assert!(c.value.is_commentary());
        assert!(card("").value.is_commentary());
    }

    #[test]
    fn hierarch() {
        let c = card("HIERARCH ESO DET CHIP NAME = 'CCD-44' / chip");
        assert_eq!(c.keyword, "ESO DET CHIP NAME");
        assert!(c.hierarch);
        assert_eq!(c.value, Value::String("CCD-44".into()));
    }

    #[test]
    fn end_and_continue() {
        assert!(matches!(parse_record(&rec("END"), 0), Parsed::End));
        match parse_record(&rec("CONTINUE  'more text&'"), 0) {
            Parsed::Continue { value, .. } => assert_eq!(value.as_deref(), Some("more text&")),
            _ => panic!(),
        }
    }

    #[test]
    fn problems() {
        let Parsed::Card(_, p) = parse_record(&rec("bad key = 1"), 0) else {
            panic!()
        };
        assert_eq!(p[0].code, "invalid_keyword");
        let mut b = rec("OBJECT  = 'x'");
        b[20] = 0xE9;
        let Parsed::Card(_, p) = parse_record(&b, 0) else {
            panic!()
        };
        assert_eq!(p[0].code, "non_ascii");
    }
}
