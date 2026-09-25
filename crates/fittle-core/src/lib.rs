//! Fittle core: FITS header model, parsing, keyword dictionary.
//!
//! Reads are header-only. Nothing in this crate touches data-unit bytes.

pub mod canonical;
pub mod card;
pub mod catalog;
pub mod classify;
pub mod derive;
pub mod dict;
pub mod diff;
pub mod doc;
pub mod filename;
pub mod fits;
pub mod header;
pub mod info;
pub mod vendor;

pub use card::{Card, Value};
pub use doc::{HEADER_SCHEMA, HeaderDoc};
pub use fits::{BLOCK_LEN, Error, Fits, Hdu, HduKind, Issue, Severity};
pub use header::Header;
pub use info::{INFO_SCHEMA, Info, info, info_from};
