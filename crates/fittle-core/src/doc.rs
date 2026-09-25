//! Stable JSON documents shared by the CLI (`--json`) and MCP tools.

use serde::Serialize;

use crate::fits::{Fits, Issue};

/// Schema id for `fittle header --json`. Bump on breaking changes.
pub const HEADER_SCHEMA: &str = "fittle.header/1";

#[derive(Debug, Serialize)]
pub struct HeaderDoc<'a> {
    pub schema: &'static str,
    pub path: &'a str,
    #[serde(flatten)]
    pub fits: &'a Fits,
}

impl Fits {
    /// A copy keeping only HDU `hdu` (if given) and cards whose keyword
    /// contains `grep` (case-insensitive, if given). File-level issues and
    /// issues for kept HDUs are retained.
    pub fn filtered(&self, hdu: Option<usize>, grep: Option<&str>) -> Fits {
        let needle = grep.map(str::to_ascii_uppercase);
        let hdus = self
            .hdus
            .iter()
            .filter(|h| hdu.is_none_or(|i| h.index == i))
            .map(|h| {
                let mut h = h.clone();
                if let Some(n) = &needle {
                    h.cards
                        .cards
                        .retain(|c| c.keyword.to_ascii_uppercase().contains(n.as_str()));
                }
                h
            })
            .collect();
        let issues: Vec<Issue> = self
            .issues
            .iter()
            .filter(|i| hdu.is_none() || i.hdu.is_none() || i.hdu == hdu)
            .cloned()
            .collect();
        Fits {
            file_bytes: self.file_bytes,
            hdus,
            issues,
        }
    }
}
