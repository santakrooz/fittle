//! Pixel decode, stats, stretch, debayer, stars/HFR, resize, export.
//!
//! M0 scope: decode an image HDU (plain or RICE_1 tile-compressed) into
//! physical `f32` values, normalize, and compute auto-STF display parameters.

pub mod decode;
pub mod preview;
pub mod rice;
pub mod stretch;
mod tiles;

pub use decode::{DecodeError, Image, decode_hdu};
pub use preview::{Preview, PreviewInfo, preview};
