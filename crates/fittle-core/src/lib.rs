//! Fittle core: FITS header model, parsing, keyword dictionary.
//!
//! Reads are header-only. Writes (`write`) change header blocks only and
//! verify that every other byte is unchanged.

pub mod canonical;
pub mod card;
pub mod catalog;
pub mod checksum;
pub mod classify;
pub mod derive;
pub mod dict;
pub mod diff;
pub mod doc;
pub mod edit;
pub mod filename;
pub mod fits;
pub mod header;
pub mod info;
pub mod privacy;
pub mod vendor;
pub mod write;

pub use card::{Card, Value};
pub use doc::{HEADER_SCHEMA, HeaderDoc};
pub use fits::{BLOCK_LEN, Error, Fits, Hdu, HduKind, Issue, Severity};
pub use header::Header;
pub use info::{INFO_SCHEMA, Info, info, info_from};
