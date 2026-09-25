# 0004 — Vendor knowledge is data, shared with AstroSideKick

Date: 2026-09-25 · Milestone: M1

## Context

M1 must explain files from smart scopes (Seestar, Dwarf, Vaonis, Unistellar, Celestron
Origin), capture apps (ASIAIR, N.I.N.A., SharpCap, Ekos, APT, SGP, MaxIm) and stackers
(Siril, PixInsight, DeepSkyStacker, ASTAP). AstroSideKick already keeps a verified scope
registry (`packages/scope-profiles/src/registry.json`: match rules + hardware) and an
OpenNGC-derived target catalogue, under the rule "everything scope-specific is data,
never code".

## Decision

- `fittle-core/data/` holds **byte-for-byte copies** of AstroSideKick's `registry.json`
  (as `scope-profiles.json`) and `targets*.json`, refreshed with
  `scripts/sync-astrosidekick-data.sh`. Corrections go upstream first.
- Fittle's own knowledge lives in `data/apps.json`: software fingerprints (capture,
  processing, observatory) and **quirks** (extra keys, ignored keys, unit scales, value
  aliases, serial-number keys, mount-in-TELESCOP, row order, "EXPTIME is the total",
  frame counts from HISTORY). Each entry records `verified` and a `source`.
- No vendor names appear in Rust code. Standard FITS conventions (OBJECT, EXPTIME,
  LIVETIME …) and generic processing vocabulary stay in code.
- Precedence: standard keywords → vendor keys → scope-profile defaults → file name.
  Every canonical value records its source, and defaults/derivations are labelled.

## Consequences

- One place to fix a vendor mistake, visible to both apps.
- The copies can drift; the sync script plus `SOURCES.md` (commit id) make drift visible.
  When the shared-package decision lands, both apps consume one package instead.
- Licence: the catalogue files are CC BY-SA 4.0 (OpenNGC). See `NOTICE.md`.

## Open items found while building M1 (to fix upstream in AstroSideKick)

- Seestar S30 Pro: focal length is 160 mm (f/5.3), not 150; native frame 2160×3840.
- Seestar matchers should accept `INSTRUME='ZWO Seestar S30 Pro'` (seen in real stacks).
- Dwarf 3 writes `TELESCOP`/`INSTRUME = 'DWARFIII'` (no space); add that pattern.
- Dwarf mini hardware: IMX662, 2.9 µm, 150 mm, 30 mm.
- Candidate profiles with documented hardware: Vespera I/II/Pro/Passenger, Stellina,
  eVscope 1/2, eQuinox 1/2, Odyssey, Celestron Origin (see research notes in
  `data/apps.json` sources).
