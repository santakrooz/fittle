# 0001 — FITS I/O: own reader and RICE_1 decoder, `fitsrs` as test oracle only

Date: 2026-09-25 · Milestone: M0

## Context

PLAN.md proposed "own header reader/writer + `fitsrs` (pure Rust) for data", with a
spike to decide between `fitsrs` tile-compression support and a small Rice codec.

## What the spike found

- **Headers.** Our `fittle-core` reader parses every file in the synthetic corpus
  and all 2,682 real files in the local sample set (Seestar S50/S30 Pro subs,
  live and DSO stacks, Siril 1.4.4 stacks, ASIAIR lights/flats/darks/biases)
  with zero issues. Total 287 ms, slowest file 0.44 ms (budget: < 5 ms).
  `fitsrs` 0.4.1 lacks HIERARCH and would not give us the byte offsets,
  record indices and raw cards that safe editing needs.
- **Uncompressed data.** Reading big-endian pixels at the offsets core already
  records is ~60 lines. It matches `fitsrs` exactly on every uncompressed
  corpus image (`decoder_matches_fitsrs`).
- **`.fz` / RICE_1.** `fitsrs` 0.4.1 panics with integer overflow in debug
  builds and returns wrong pixels in release on 16-bit RICE_1 tiles written by
  astropy (the same layout fpack produces for Seestar/ASIAIR archives).
  Captured by the ignored test `fitsrs_decodes_rice_fz_losslessly`.
- Our own RICE_1 decoder (`fittle-image::rice`, ~150 lines, safe Rust,
  bounds-checked) round-trips losslessly on 8/16/32-bit, row and 16×16 tiles,
  flat (low-entropy) and full-range noise (high-entropy) images.

## Decision

1. Headers: our own parser in `fittle-core` (already required for safe writes).
2. Pixel data: our own decoder in `fittle-image`, including RICE_1 tiles.
3. `fitsrs` stays as a **dev-dependency only**, used as an independent oracle in
   tests. No C dependencies (`cfitsio`) are needed.
4. Deferred: GZIP_1/GZIP_2 tiles (pure-Rust `flate2`/`miniz_oxide`), quantized
   float tiles (ZSCALE/ZZERO + dithering), HCOMPRESS. Add when a corpus file needs
   them. Report the RICE_1 bug upstream to cds-astro/fitsrs.

## Consequences

- We own the tile decoder, so it gets `cargo-fuzz` coverage in M3 alongside the
  header parser (as PLAN.md already requires).
- Re-run the ignored test when upgrading `fitsrs`; if it passes we can reconsider.
