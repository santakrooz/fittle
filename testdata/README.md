# Test corpus

## `synthetic/` (committed)

Small (≤ 40 KB) FITS files written by **astropy** via `generate.py`, so the parser
is tested against an independent writer. Headers imitate each capture app's
style (Seestar, ASIAIR, N.I.N.A., Siril, PixInsight, calibration frames) but are
not real captures. `edge/` covers CONTINUE, HIERARCH, multi-HDU and RICE_1
`.fz` (each `.fz` has an `-original` uncompressed twin). `malformed/` covers
truncation, duplicate keys, unquoted strings, lowercase keywords and non-ASCII.

Regenerate (deterministic output):

    uv run testdata/generate.py

Golden snapshots of `fittle header --json` for every file live in
`crates/fittle-cli/tests/snapshots/`. Review changes with `cargo insta review`.

## Real files (local only, never committed)

Real captures are large and often contain site coordinates. Keep them outside
git (the repo ignores `/sample fits/`) and run the opt-in check:

    FITTLE_SAMPLES="$PWD/sample fits" cargo test -p fittle-core --release --test local_corpus -- --nocapture

Small, privacy-scrubbed real files can later go under `testdata/real/` via git-lfs.
