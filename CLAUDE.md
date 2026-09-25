# Fittle

Fittle is an open-source, cross-platform (macOS, Windows, Linux) utility for astrophotography FITS files. It has three front ends over one Rust core: a CLI, a light desktop GUI (Tauri 2), and an MCP server. It explains a FITS file (is it a sub or a stack, what rig took it, how much integration, where and when), views and edits header keywords safely, shows the image with a display stretch, and does quick conversions.

## Read first

- `docs/PLAN.md`: the full product and technical plan. Treat it as the spec.
- `docs/mocks/screens/*.png`: one image per GUI screen. Match these when building UI.
- `docs/mocks/fittle-mocks.html`: all mocks in one page (open in a browser; tokens and CSS live here too).

When a task names a milestone (M0–M6, v1.0), read that row of the Roadmap in `docs/PLAN.md` and its screens in the Mocks table.

## Non-goals (do not build)

No stacking, registration, calibration, gradient removal, sharpening, denoise, deconvolution, or colour calibration. No non-linear processing in exports beyond a display stretch that is labelled as such. No accounts, no telemetry. Network use only for optional catalog or plate-solve lookups.

## Hard rules

1. **Never change data-unit bytes during a header edit.** Rewrite in place only if the header block count (2,880-byte blocks) is unchanged; otherwise write a temp file and atomically rename. Every write path has a test that hashes the data unit before and after.
2. **Never overwrite a source file during export or transform.** Outputs are always new files.
3. **One core, three faces.** Every feature is a function in a `fittle-*` crate, then a CLI command, an MCP tool, and a GUI action. No logic in the GUI or CLI layers.
4. **`--json` on every read command**, with a stable schema. Exit codes: 0 ok, 1 error, 2 validation.
5. **MCP mutations are dry-run by default** and return a diff. Respect the configured root-folder allowlist.
6. **Show evidence.** Classification results carry a 0–100 confidence and the list of keywords or HISTORY lines that produced them. Derived values are marked as derived.
7. **Pure Rust by default.** Don't add C dependencies (e.g. cfitsio) without a note in `docs/decisions/`.

## Workspace layout

```
crates/fittle-core    header model, parse/write, keyword dictionary, vendor quirks, classify, derive
crates/fittle-image   decode, stats, stretch, debayer, stars/HFR, resize, export
crates/fittle-astro   alt/az, airmass, moon/sun, WCS math, constellation lookup
crates/fittle-scan    folder scan, session report, sub grader, calibration matcher
crates/fittle-cli     clap; binary `fittle`
crates/fittle-mcp     rmcp, stdio; also reachable as `fittle mcp`
app/                  Tauri 2 shell (src-tauri) + web UI (ui)
integrations/         Quick Look (Swift), Windows thumbnail handler, Linux thumbnailer
testdata/             FITS corpus by capture app (git-lfs)
docs/                 PLAN.md, mocks/, decisions/
```

## Design tokens (dark first)

| Token | Dark | Light |
| --- | --- | --- |
| bg | #0A0E16 | #F6F4EF |
| surface | #111827 | #FFFFFF |
| surface-2 | #172033 | #EEF0F4 |
| line | #1F2A3D | #DDE1E8 |
| text | #E8ECF3 | #141A26 |
| muted | #8391A7 | #5B6678 |
| accent | #F5B83D | #C98A0C |
| nebula-orange / rose / teal | #E9894A / #E0567A / #3FC1C9 | same |
| ok | #3DD68C | #1A9E5E |

Type: Bricolage Grotesque (display, big numbers), Inter (UI), JetBrains Mono (keywords, values). Radius 14px for cards, 999px for pills. Uppercase 10–11px labels with letter-spacing. Three-pane layout: file rail 232px, image stage (fluid), inspector 356px, filmstrip for folders. Keep these tokens in one shared package so they can be reused by Stardog.

## Testing and performance budgets

- Golden snapshots (`insta`) of `fittle info --json` for every corpus file.
- Property tests (`proptest`) for header edits; `cargo-fuzz` for the header parser and tile decompression.
- Budgets: header open < 5 ms; 60 MP float stats < 150 ms; scan 5,000 headers < 3 s; 60 MP open + auto-stretch in the GUI < 1 s.
- CI matrix: macOS, Windows, Ubuntu on every PR.

## Working style

- Work one milestone per branch. Finish with tests green on all three OSes.
- Before adding a crate or changing the architecture, check `docs/PLAN.md`. If you deviate from it, record why in `docs/decisions/NNNN-title.md`.
- Keep `docs/PLAN.md` current when decisions land (license, frontend framework, etc.).

## Open decisions (ask before assuming)

License (MIT/Apache-2.0 vs GPL-3.0), frontend framework (match Stardog), shared token package name, whether Quick Look and thumbnails ship in v1.0.
