//! Pixel decode, stats, stretch, debayer, stars/HFR, resize, export.
//!
//! M0 scope: decode an image HDU (plain or RICE_1 tile-compressed) into
//! physical `f32` values, normalize, and compute auto-STF display parameters.

pub mod debayer;
pub mod decode;
pub mod preview;
pub mod rice;
pub mod session;
pub mod stats;
pub mod stretch;
pub mod thumb;
pub mod tiles;
pub mod view;

pub use decode::{DecodeError, Image, decode_hdu};
pub use preview::{Preview, PreviewInfo, preview};
pub use session::{Readout, ViewSession};
pub use thumb::thumbnail;
pub use view::{Mode, Opened};
