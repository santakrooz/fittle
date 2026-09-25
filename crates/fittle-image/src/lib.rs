//! Pixel decode, stats, stretch, debayer, stars/HFR, resize, export.
//!
//! M0 scope: decode an image HDU (plain or RICE_1 tile-compressed) into
//! physical `f32` values.

pub mod decode;
pub mod rice;
mod tiles;

pub use decode::{DecodeError, Image, decode_hdu};
