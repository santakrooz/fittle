//! "What is this file, and how was it made?" — the `fittle info` document.

use std::path::Path;

use serde::Serialize;

use crate::canonical::{self, Canonical, Note};
use crate::catalog::{self, Target};
use crate::classify::{self, Verdict};
use crate::derive::{self, Derived};
use crate::filename::{self, FileNameFacts};
use crate::fits::{Error, Fits, HduKind, Issue};
use crate::header::Header;
use crate::vendor::{self, Matched, Role};

/// Schema id for `fittle info --json`. Bump on breaking changes.
pub const INFO_SCHEMA: &str = "fittle.info/1";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Software {
    #[serde(flatten)]
    pub matched: Matched,
    pub role: Role,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Origin {
    /// Smart-scope profile (hardware) if recognised.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<Matched>,
    /// Capture and processing software, capture first.
    pub software: Vec<Software>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImageSummary {
    pub hdu: usize,
    pub width: u64,
    pub height: u64,
    pub planes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitpix: Option<i64>,
    pub compressed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Info {
    pub schema: &'static str,
    pub path: String,
    pub verdict: Verdict,
    pub origin: Origin,
    /// Catalogue object the file is of, by OBJECT name or position.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
    pub fields: Canonical,
    pub derived: Derived,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<ImageSummary>,
    pub file_name: FileNameFacts,
    /// How the header was read: defaults used, keys ignored, mismatches.
    pub notes: Vec<Note>,
    /// Structural problems from parsing.
    pub health: Vec<Issue>,
}

pub fn info(path: impl AsRef<Path>) -> Result<Info, Error> {
    let path = path.as_ref();
    let fits = Fits::open(path)?;
    Ok(info_from(&fits, &path.to_string_lossy()))
}

/// Build the document from an already-parsed file.
pub fn info_from(fits: &Fits, path: &str) -> Info {
    let image_hdu = fits.hdus.iter().find(|h| !h.shape.is_empty());
    // Image HDU keys first, then primary keys (e.g. .fz files, extensions).
    let mut merged = Header::default();
    if let Some(h) = image_hdu {
        merged.cards.extend(h.header().cards.iter().cloned());
    }
    if let Some(p) = fits
        .hdus
        .first()
        .filter(|p| image_hdu.is_none_or(|h| h.index != p.index))
    {
        merged.cards.extend(p.header().cards.iter().cloned());
    }

    let file = filename::parse(path);
    let scope = vendor::match_scope(&merged);
    let apps = vendor::match_apps(&merged);
    let app_ids: Vec<&str> = apps.iter().map(|(a, _)| a.id.as_str()).collect();
    let quirks = vendor::quirks_for(scope.as_ref().map(|(p, _)| p.id.as_str()), &app_ids);

    let (width, height, planes) = image_hdu.map_or((0, 0, 0), |h| {
        (
            h.shape[0],
            h.shape.get(1).copied().unwrap_or(1),
            h.shape.iter().skip(2).product::<u64>().max(1),
        )
    });
    let ctx = canonical::Context {
        header: &merged,
        planes,
        profile: scope.as_ref().map(|(p, _)| *p),
        quirks: &quirks,
        file: &file,
    };
    let (fields, mut notes) = canonical::read(&ctx);
    let verdict = classify::classify(&classify::Input {
        header: &merged,
        fields: &fields,
        file: &file,
        bitpix: image_hdu.and_then(|h| h.bitpix),
        planes,
    });
    let derived = derive::derive(&fields, &merged, width, height, &mut notes);

    let target = fields
        .object
        .as_ref()
        .and_then(|o| catalog::lookup(&o.value))
        .or_else(|| match (&fields.ra, &fields.dec) {
            (Some(r), Some(d)) => catalog::nearest(r.value, d.value, 0.5).map(|(t, _)| t),
            _ => None,
        });

    let mut software: Vec<Software> = apps
        .into_iter()
        .map(|(a, m)| Software {
            matched: m,
            role: a.role,
        })
        .collect();
    software.sort_by_key(|s| s.role != Role::Capture);

    Info {
        schema: INFO_SCHEMA,
        path: path.to_string(),
        verdict,
        origin: Origin {
            scope: scope.map(|(_, m)| m),
            software,
        },
        target,
        fields,
        derived,
        image: image_hdu.map(|h| ImageSummary {
            hdu: h.index,
            width,
            height,
            planes,
            bitpix: h.bitpix,
            compressed: h.kind == HduKind::CompressedImage,
        }),
        file_name: file,
        notes,
        health: fits.issues.clone(),
    }
}
