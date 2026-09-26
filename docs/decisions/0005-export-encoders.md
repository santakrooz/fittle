# 0005 — Export encoders: focused pure-Rust crates, lossless WebP only, AVIF, share cards

Date: 2026-09-25 · Milestone: M4

## Context

PLAN.md lists the `image` crate for PNG/JPEG/WebP/TIFF, `tiff` for 32-bit float and
`ravif` for AVIF. Exports also need XMP embedded in each format, 16-bit PNG/TIFF,
float TIFF and FITS output with an updated header, all without C dependencies
(CLAUDE.md rule 7).

## Decision

- Use the encoders directly instead of the `image` umbrella crate: `png` (iTXt XMP),
  `jpeg-encoder` (APP1 XMP), `image-webp` (XMP chunk) and `tiff` (tag 700 XMP; 8/16-bit
  and 32-bit float). They are what `image` uses underneath, each gives direct access to
  the metadata slot we need, and we skip `image`'s decoders and format zoo.
- **WebP is lossless only.** The only pure-Rust WebP encoder (`image-webp`) writes
  lossless VP8L; lossy VP8 needs `libwebp` (C). Users who want small lossy files pick
  JPEG (or AVIF, below).
- FITS output is written by Fittle's own encoder (32-bit float, one HDU), with the
  source header carried over minus structural/compression keys, a rewritten TAN WCS
  when geometry changes (SIP dropped after resampling or rotation, with a HISTORY
  note), Bayer keys dropped after debayering, and the privacy scrub applied.
- **AVIF** uses `ravif` (rav1e) **without its `asm` feature**, which would need nasm; the
  pure-Rust encoder is slower but needs no toolchain (1600 px in ~0.8 s). ravif has no XMP
  slot, so AVIF carries the one-line acquisition summary as EXIF `ImageDescription`.
- **Share cards** draw text with `swash` (pure Rust, supports variable fonts) using the
  design-system fonts, embedded as OFL variable TTFs (~1.5 MB). Fontsource's WOFF2 files
  can't be read by pure-Rust rasterizers, so the TTFs come from google/fonts.

## Consequences

- No C toolchain for export; binaries stay small.
- Lossy WebP can come later if a maintained pure-Rust VP8 encoder appears; the
  `Format::Webp` variant would gain a `quality` field.
