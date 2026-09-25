//! A parsed header: logical cards in file order.

use serde::Serialize;

use crate::card::{Card, CardProblem, Parsed, Value, parse_record};

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Header {
    pub cards: Vec<Card>,
}

impl Header {
    /// First value card with this keyword (commentary is ignored).
    pub fn get(&self, keyword: &str) -> Option<&Card> {
        self.cards
            .iter()
            .find(|c| c.keyword == keyword && !c.value.is_commentary())
    }

    pub fn value(&self, keyword: &str) -> Option<&Value> {
        self.get(keyword).map(|c| &c.value)
    }

    pub fn int(&self, keyword: &str) -> Option<i64> {
        self.value(keyword)?.as_i64()
    }

    pub fn float(&self, keyword: &str) -> Option<f64> {
        self.value(keyword)?.as_f64()
    }

    pub fn string(&self, keyword: &str) -> Option<&str> {
        self.value(keyword)?.as_str()
    }

    pub fn logical(&self, keyword: &str) -> Option<bool> {
        self.value(keyword)?.as_bool()
    }

    /// Text of every `HISTORY` record, in order.
    pub fn history(&self) -> impl Iterator<Item = &str> {
        self.commentary("HISTORY")
    }

    pub fn commentary<'a>(&'a self, keyword: &'a str) -> impl Iterator<Item = &'a str> {
        self.cards
            .iter()
            .filter(move |c| c.keyword == keyword)
            .filter_map(|c| match &c.value {
                Value::Commentary(t) => Some(t.as_str()),
                _ => None,
            })
    }
}

/// Incrementally builds a header from 80-char records, merging `CONTINUE`
/// long strings into the card they extend.
#[derive(Default)]
pub(crate) struct HeaderBuilder {
    header: Header,
    records: usize,
    pub problems: Vec<(usize, CardProblem)>,
}

impl HeaderBuilder {
    /// Feed one record. Returns true when `END` is reached.
    pub fn push(&mut self, bytes: &[u8]) -> bool {
        let record = self.records;
        self.records += 1;
        match parse_record(bytes, record) {
            Parsed::End => return true,
            Parsed::Card(card, problems) => {
                self.problems
                    .extend(problems.into_iter().map(|p| (record, p)));
                self.header.cards.push(card);
            }
            Parsed::Continue {
                value,
                comment,
                raw,
            } => {
                let target = self
                    .header
                    .cards
                    .last_mut()
                    .filter(|c| matches!(&c.value, Value::String(s) if s.ends_with('&')));
                match (target, value) {
                    (Some(card), Some(more)) => {
                        let Value::String(s) = &mut card.value else {
                            unreachable!()
                        };
                        s.pop();
                        s.push_str(&more);
                        // Trailing spaces are not significant in a FITS string,
                        // including across CONTINUE records.
                        let keep = s.trim_end().len();
                        s.truncate(keep);
                        card.comment = match (card.comment.take(), comment) {
                            (Some(a), Some(b)) => Some(format!("{a} {b}")),
                            (a, b) => a.or(b),
                        };
                        card.value_text = format!("'{}'", s.replace('\'', "''"));
                        card.raw.push(raw);
                    }
                    (_, value) => {
                        self.problems.push((
                            record,
                            CardProblem {
                                code: "orphan_continue",
                                message: "CONTINUE record does not follow a string ending in '&'"
                                    .into(),
                            },
                        ));
                        let text = raw[8..].trim_end().to_string();
                        self.header.cards.push(Card {
                            keyword: "CONTINUE".into(),
                            value: value.map_or(Value::Commentary(text.clone()), Value::String),
                            comment,
                            hierarch: false,
                            record,
                            raw: vec![raw],
                            value_text: text,
                        });
                    }
                }
            }
        }
        false
    }

    pub fn finish(self) -> (Header, Vec<(usize, CardProblem)>) {
        (self.header, self.problems)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(lines: &[&str]) -> (Header, Vec<(usize, CardProblem)>) {
        let mut b = HeaderBuilder::default();
        for l in lines {
            if b.push(format!("{l:<80}").as_bytes()) {
                break;
            }
        }
        b.finish()
    }

    #[test]
    fn continue_long_string() {
        let (h, p) = build(&[
            "NOTES   = 'The quick brown fox &'  / first",
            "CONTINUE  'jumps over the &'",
            "CONTINUE  'lazy dog'         / last",
            "END",
        ]);
        assert!(p.is_empty());
        assert_eq!(h.cards.len(), 1);
        assert_eq!(
            h.string("NOTES"),
            Some("The quick brown fox jumps over the lazy dog")
        );
        assert_eq!(h.cards[0].raw.len(), 3);
        assert_eq!(h.cards[0].comment.as_deref(), Some("first last"));
    }

    #[test]
    fn orphan_continue() {
        let (h, p) = build(&["OBJECT  = 'M31'", "CONTINUE  'x'", "END"]);
        assert_eq!(h.cards.len(), 2);
        assert_eq!(p[0].1.code, "orphan_continue");
    }

    #[test]
    fn accessors() {
        let (h, _) = build(&[
            "EXPTIME =                   20",
            "HISTORY a",
            "HISTORY b",
            "END",
        ]);
        assert_eq!(h.float("EXPTIME"), Some(20.0));
        assert_eq!(h.history().collect::<Vec<_>>(), ["a", "b"]);
    }
}
