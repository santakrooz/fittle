//! Fittle core: FITS header model, parsing, keyword dictionary.
//!
//! Reads are header-only. Nothing in this crate touches data-unit bytes.

pub mod card;
pub mod dict;
pub mod doc;
pub mod fits;
pub mod header;

pub use card::{Card, Value};
pub use doc::{HEADER_SCHEMA, HeaderDoc};
pub use fits::{BLOCK_LEN, Error, Fits, Hdu, HduKind, Issue, Severity};
pub use header::Header;
