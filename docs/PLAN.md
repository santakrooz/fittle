# Fittle — FITS Utility Plan

Sep 25, 2026 · Mike

## Vision and principles

Fittle answers one question in under a second: **"What is this FITS file, and how was it made?"** It is the astrophotographer's Quick Look, a header surgeon, and a one-click exporter, sharing one Rust core across a CLI, a light desktop GUI, and an MCP server.

**Principles**

- **Instant.** Header-only reads by default; pixels decoded lazily. A 60 MP 32-bit stack opens and auto-stretches in under 1 s.
- **Explain, don't just dump.** Every keyword has a plain-English label, a unit, and a "why it matters" tooltip. Derived facts (pixel scale, FOV, total integration) sit beside raw cards.
- **Show the evidence.** When Fittle says "stacked, 212 subs", it lists which keywords and HISTORY lines led it there, with a confidence score.
- **Never corrupt data.** Header edits never touch the data unit bytes; writes are atomic, backed up, and checksum-aware.
- **One core, three faces.** Every GUI action has a CLI equivalent and an MCP tool. JSON output everywhere.
- **Quiet beauty.** Deep-space dark UI, amber accent, zero chrome; the image is the hero.

**Non-goals (v1)**

- No stacking, registration, calibration, gradient removal, sharpening, denoise, deconvolution, or colour calibration. That is Siril / PixInsight / Stardog territory.
- No non-linear processing baked into exports beyond a display stretch (clearly labelled as such).
- No cloud account or telemetry. Fully offline; network only for optional catalog or plate-solve lookups.

## Core feature: "Understand this file"

The Overview card is Fittle's reason to exist: a one-screen verdict on frame type, rig, conditions, and integration, each backed by evidence.

### 1. Frame classification (with confidence + evidence)

| Verdict | Primary signals | Secondary signals |
| --- | --- | --- |
| Light sub | `IMAGETYP=Light Frame`, single `EXPTIME`, no combine keys | Integer BITPIX 16, Bayer pattern present, capture-app filename (`Light_NGC 6995_20.0s_LP_…`) |
| Stacked / integrated | `STACKCNT`, `NCOMBINE`, `LIVETIME`, `IMAGETYP=Master Light` | BITPIX −32, HISTORY lines (Siril `stack`, PixInsight `ImageIntegration`, DSS), `PROGRAM`, filename prefixes (`r_pp_`, `result_`, `Stacked_`) |
| Calibration: dark / flat / bias / dark-flat | `IMAGETYP` values, `EXPTIME` ≈ 0 (bias) | Very low median (dark), mid-range median + vignetting (flat) |
| Master calibration | Calibration type + combine keys | Float data, `master` in name |
| Processed / intermediate | Siril `pp_`, `r_`, `bkg_` prefixes, HISTORY entries | Non-linear histogram (median far above noise floor) |
| Unknown | None of the above | Show raw header and ask user to tag |

Each verdict ships with a score (0–100) and an evidence list, e.g. *"Stacked (94): STACKCNT=212, LIVETIME=4240 s, HISTORY 'Siril stack…', BITPIX −32"*.

### 2. Capture-app fingerprinting

Identify the source (`SWCREATE`, `CREATOR`, `PROGRAM`, `SOFTWARE`, filename style) and apply per-app keyword quirks: **Seestar S50/S30, ASIAIR, N.I.N.A., SharpCap, KStars/Ekos, APT, SGP, MaxIm DL, Dwarf II/3, Vaonis, Unistellar, Siril, PixInsight, DSS, ASTAP**. A quirks table in `fittle-core` maps vendor keys to canonical fields.

### 3. Canonical fields shown on Overview

| Group | Fields (canonical → common FITS keys) |
| --- | --- |
| Target | Object (`OBJECT`), RA/Dec (`OBJCTRA`/`OBJCTDEC`, `RA`/`DEC`, `CRVAL1/2`), constellation (computed) |
| Optics | Telescope (`TELESCOP`), focal length (`FOCALLEN`), aperture (`APTDIA`, `APERTURE`), focal ratio (`FOCRATIO` or computed), reducer notes |
| Camera | Instrument (`INSTRUME`), pixel size (`XPIXSZ`/`YPIXSZ`), binning (`XBINNING`), gain / offset (`GAIN`, `OFFSET`, `EGAIN`), sensor temp (`CCD-TEMP`, `SET-TEMP`), Bayer (`BAYERPAT`, `XBAYROFF`), readout mode |
| Filter | `FILTER`, filter wheel position; Seestar LP flag from filename |
| Exposure | Sub length (`EXPTIME`/`EXPOSURE`), sub count, total integration, stack method if known |
| Time | `DATE-OBS` (UTC), `DATE-LOC`, `MJD-OBS`, session night |
| Site | `SITELAT`, `SITELONG`, `SITEELEV`, observatory name; reverse-geocoded place label (offline gazetteer) |
| Mount / guiding | `MOUNT`, `PIERSIDE`, guide RMS if written, focuser position/temp (`FOCPOS`, `FOCTEMP`) |
| Astrometry | WCS present? (`CTYPE`, `CD`/`CDELT`), plate-solved rotation, pixel scale from WCS |

### 4. Derived facts (computed, labelled "derived")

- **Pixel scale** = 206.265 × pixel µm ÷ focal mm (arcsec/px); cross-check against WCS scale and flag mismatches (wrong focal length is common).
- **Field of view** (arcmin), **sampling** vs typical seeing (under / well / over-sampled).
- **Total integration** (subs × sub length or `LIVETIME`), formatted `4 h 12 m`.
- **Target altitude and airmass** at `DATE-OBS` from site + RA/Dec; **moon phase, illumination and separation**; **sun altitude** (twilight check).
- **Image stats**: dimensions, bit depth, min/max/median/MAD, saturation %, estimated background, star count, median HFR/FWHM (arcsec).
- **Linear vs stretched** guess from histogram shape.
- **Header health**: missing essentials, non-standard keys, duplicate keywords, bad CHECKSUM.

## Metadata viewer and editor

Header editing is safe by construction: edits are staged, diffed, validated, then written atomically without touching pixel bytes.

**Viewing**

- Three views of the same header: **Grouped** (Target, Optics, Camera… with friendly labels), **Raw cards** (exact 80-char records, monospace, in file order), and **JSON**.
- Built-in keyword dictionary (FITS standard + common astro conventions + vendor quirks) powering hover help and autocomplete.
- Multi-HDU support: HDU switcher for extensions (e.g. Siril/PixInsight extra tables, `.fz` compressed images).
- Search/filter keys, copy a card, copy all as text/JSON.

**Editing**

- Inline edit value and comment; add, delete, reorder keywords; typed inputs (bool, int, float, string, date).
- Validation: 8-char uppercase names, value types, 68-char string limits, reserved/structural keys (`SIMPLE`, `BITPIX`, `NAXISn`, `END`) locked.
- **Staged changes panel** with a diff and one-click revert; undo/redo stack.
- **Batch edit** across many files: set, rename, delete, find/replace in values (e.g. fix `FOCALLEN` on 3,785 Seestar subs, set `OBJECT` for a folder).
- **Templates / rig profiles**: save "Seestar S50 + LP" or "RedCat 51 + 533MC" and apply the key set in one click.
- **Privacy scrub** preset: remove `SITELAT`, `SITELONG`, observer names, serial numbers before sharing.
- **Header diff** between two files, side by side, highlighting differences (great for "why won't these calibrate together?").

**Safe writes**

- In-place rewrite only when the header's 2,880-byte block count is unchanged; otherwise stream to a temp file and atomic-rename.
- Optional `.bak` sidecar (on by default for the first edit per file).
- Recompute `CHECKSUM`/`DATASUM` if present; add a `HISTORY Fittle vX: set FOCALLEN 250→300` audit line (toggleable).
- Round-trip test: data unit bytes must hash identically before and after every write.

## Image viewer and stretch

The viewer auto-stretches by default so a linear sub never looks "black", while the file itself stays untouched.

**Stretch modes** (display only unless exported)

- **Auto STF** (default): PixInsight/Siril-style midtones transfer. Shadows clip = median − 2.8 × MAD, target background 0.25; linked or unlinked RGB.
- Linear min/max, percentile (0.1–99.9 %), asinh, log, and manual black/mid/white sliders over a live histogram.
- Clipping overlay (blue shadows, red highlights).

**Viewing**

- GPU tiles (WebGL2 / wgpu) with smooth zoom, Fit, 1:1, 2:1; pan with space-drag; minimap on large images.
- **Debayer preview** for OSC subs (bilinear, fast; honours `BAYERPAT` + offsets); toggle raw CFA view.
- Channel isolate (R/G/B/L), mono false-colour, invert.
- **Pixel inspector**: x/y, raw ADU, normalized value, RA/Dec under cursor when WCS exists.
- Histogram per channel (linear/log axis) with stats.
- **Star overlay**: detected stars with HFR circles; median HFR/FWHM and eccentricity heatmap (quick tilt/collimation hint).
- **WCS grid + object labels** (optional, from bundled lightweight catalog: Messier, NGC/IC, Sharpless, bright stars).
- **Blink** across a folder of subs (arrow keys / auto-play at 2–10 fps) with a locked stretch so clouds, satellites, and drift jump out.
- **Compare** two files split-screen or wipe slider (e.g. sub vs stack, before/after another tool).

## Quick tasks

Every task works on one file or a whole selection/folder, previews output first, and never overwrites the source.

| Task | Options | Notes |
| --- | --- | --- |
| Export image | TIFF (8/16/32-bit float), PNG (8/16), JPEG, WebP, AVIF, FITS | Stretch: none / current view / auto; embed acquisition summary into EXIF/XMP |
| Resize / bin | Scale %, max edge px, 2×2/3×3 software bin (average/sum) | Lanczos3 for downscale; WCS updated when scaling |
| Crop / rotate / flip | Rectangle, 90° steps, mirror H/V | WCS and `BAYERPAT` offsets adjusted correctly |
| Debayer to RGB FITS | Bilinear / VNG | Outputs 3-plane FITS or TIFF |
| Split / merge channels | RGB ↔ R, G, B mono | Useful before handing to other tools |
| Bit-depth convert | 16-bit int ↔ 32-bit float, normalize 0–1 |  |
| Compress | fpack/funpack Rice `.fz` (lossless) | Often 40–60 % smaller for subs |
| Rename by template | `{object}_{filter}_{exptime}s_{gain}_{date}_{seq}` | Dry-run table first |
| Sort into folders | By object / filter / night / frame type | Great for Seestar dumps |
| Thumbnail / contact sheet | Grid PNG of a folder with captions | For forums and quick reviews |
| Header export | CSV / JSON of chosen keys across many files | Spreadsheet-friendly |
| Share card | Stretched image + caption strip (target, rig, integration, site) | Social / AstroBin-style |

## Ideas you may be missing

The biggest gap is folder-level intelligence: your Nebulis screenshot shows 3,785 twenty-second Seestar subs, and nobody wants to open those one at a time.

1. **Session Report (folder scan).** Point Fittle at a folder and get: nights, targets, total integration per filter, frame-type counts, temp/gain consistency, and a timeline of subs across the night. Export as Markdown/HTML/JSON.
2. **Sub grader (lightweight, not processing).** Per-sub star count, median HFR/FWHM, background level, eccentricity, satellite-trail flag. Sort and tag "reject"; move rejects to a `_rejected/` folder. Siril does this in-pipeline; Fittle does it in 10 seconds before you start.
3. **Calibration matcher.** Given lights, list which darks/flats/biases in a library match (gain, offset, temp ±2 °C, exposure, binning, filter, rotation) and warn on mismatches.
4. **Integration planner hint.** "You have 4 h 12 m on NGC 6995 in LP; Mar–Jan visibility" and a hand-off link to Nebulis Planner.
5. **OS integration.** macOS Quick Look extension + Finder thumbnails, Windows Explorer thumbnail/preview handler, Linux thumbnailer (`.thumbnailer` file). File association and "Open with Fittle". This alone would make it beloved.
6. **Drag-out.** Drag a stretched preview straight into Discord/Slack/forums as PNG.
7. **Watch folder / live mode.** Tail a capture folder during a session; auto-show the newest sub with a locked stretch and running stats (HFR trend = focus drift, background trend = dawn or clouds).
8. **Plate-solve on demand.** Optional local ASTAP or astrometry.net integration to write WCS into a header, then annotate.
9. **AstroBin acquisition CSV.** Generate the per-filter/date acquisition CSV AstroBin accepts from a folder of subs.
10. **Hand-offs.** "Open in Siril", "Send to Stardog", "Add to Nebulis library" buttons, via deep links or a shared manifest.
11. **Duplicate and corruption detector.** Truncated files (size not multiple of 2,880), duplicate subs by hash, zero-length or all-zero frames.
12. **XISF read (later).** PixInsight's format is common in the same workflows; read-only XISF + export to FITS would widen reach.
13. **Accessibility.** Keyboard-first operation, command palette, screen-reader labels, colour-blind-safe overlays.
14. **Portable mode.** A single binary that runs from a USB stick at a dark site with no install.

## Architecture and tech stack

Recommendation: a Rust workspace with one core library and three thin front ends, with the GUI in Tauri 2 so the UI is web tech that can share tokens and components with Stardog.

```
fittle/
├─ crates/
│  ├─ fittle-core     # header model, parse/write, keyword dictionary, vendor quirks, classify, derive
│  ├─ fittle-image    # decode, stats, stretch, debayer, stars/HFR, resize, export (image crate)
│  ├─ fittle-astro    # alt/az, airmass, moon/sun ephemeris, WCS math, constellation lookup
│  ├─ fittle-scan     # folder walker, session report, sub grader, calibration matcher
│  ├─ fittle-cli      # clap; binary `fittle`
│  └─ fittle-mcp      # rmcp (official Rust MCP SDK), stdio; also `fittle mcp`
├─ app/              # Tauri 2 shell + React 19 frontend (matches AstroSideKick)
│  ├─ src-tauri/      # commands call fittle-core directly
│  └─ ui/             # components, design tokens, WebGL2 viewer
├─ integrations/     # quicklook (Swift), win-thumbnail (Rust COM), linux thumbnailer
├─ testdata/         # FITS corpus by capture app (git-lfs)
└─ docs/
```

| Layer | Choice | Why |
| --- | --- | --- |
| FITS I/O | **Decided (M0, [0001](decisions/0001-fits-io.md)):** own header reader/writer and own pixel decoder; `fitsrs` kept only as a test oracle | Pure Rust = painless Windows/macOS/Linux builds, no C toolchain |
| Tile compression | **Decided (M0):** own RICE_1 decoder (`fitsrs` 0.4.1 mis-decodes 16-bit Rice); GZIP / quantized float later | `.fz` files from Seestar/ASIAIR archives |
| Pixels | `ndarray` + `rayon`; SIMD stats | 60 MP median/MAD in <150 ms |
| Export | `image` crate (PNG/JPEG/WebP/TIFF), `tiff` for 32-bit float, `ravif` for AVIF |  |
| Ephemeris | Small built-in VSOP87/ELP-lite or `astro` crate | Offline moon/sun positions |
| CLI | `clap` v4, `comfy-table`, `owo-colors`, `--json` everywhere |  |
| MCP | `rmcp`, stdio transport; optional streamable HTTP later | Works with Claude Desktop/Code |
| GUI framework | **Decided: React 19 + TypeScript + Vite** ([0002](decisions/0002-frontend-react.md)), matching AstroSideKick | Shared tokens/components |
| Web edition | Same UI + core compiled to WASM, hosted on Railway ([0003](decisions/0003-web-edition.md)) | Free tool, marketing, opt-in corpus growth |
| GUI shell | Tauri 2 | \~10 MB installers vs 100+ MB Electron; native menus, file associations |
| GUI render | WebGL2 tiled texture viewer (float textures, stretch in fragment shader) | Instant re-stretch while dragging sliders |

**Key flows**

- **Open**: read header blocks only → classify + derive → Overview renders (<50 ms). Pixels load in background → stats → auto-stretch LUT → GPU.
- **Edit**: UI stages ops → `fittle-core::HeaderEdit` validates → diff preview → atomic write → re-read and verify data hash.
- **Scan**: parallel header-only reads (thousands of files/second), optional pixel pass for grading with progress + cancel.

## CLI and MCP surface

One verb set, three front ends: every CLI command maps 1:1 to an MCP tool and a GUI action.

### CLI

```
fittle info    <files…>              # Overview: verdict, rig, exposure, site, derived facts
fittle header  <file> [--raw|--json] [--hdu N] [--grep KEY]
fittle set     <files…> KEY=VALUE [KEY=VALUE…] [--comment …] [--dry-run] [--no-backup]
fittle unset   <files…> KEY…
fittle rename-key <files…> OLD NEW
fittle diff    <a> <b>
fittle scrub   <files…> --privacy
fittle export  <files…> --to png|jpg|webp|tiff|avif [--stretch auto|none|asinh] [--bits 16] [--max-edge 2048] [-o dir]
fittle resize  <files…> --scale 50% | --bin 2
fittle crop|rotate|flip|debayer|split <files…> …
fittle fpack|funpack <files…>
fittle scan    <folder> [--grade] [--report md|html|json]
fittle grade   <folder> [--reject hfr>3.5,stars<50] [--move-rejects]
fittle match-cal <lights…> --library <cal-folder>
fittle organize <folder> --by object/filter/night [--dry-run]
fittle rename  <files…> --template "{object}_{filter}_{exptime}s_{seq}"
fittle view    <file|folder>           # launch GUI
fittle mcp                             # run MCP server on stdio
```

Conventions: `--json` on every read command, exit codes (0 ok, 1 error, 2 validation), globbing on Windows handled internally, no writes without an explicit write command.

### MCP tools

| Tool | Purpose | Writes? |
| --- | --- | --- |
| `fits_inspect` | Verdict, canonical fields, derived facts, evidence | No |
| `fits_header` | Raw or grouped header for an HDU | No |
| `fits_preview` | Returns an auto-stretched PNG/WebP (image content) at a requested size | No |
| `fits_stats` | Pixel stats, stars, HFR/FWHM | No |
| `fits_diff` | Header diff between two files | No |
| `fits_scan_folder` | Session report for a folder | No |
| `fits_grade_subs` | Per-sub quality table + suggested rejects | No |
| `fits_match_calibration` | Match lights to cal frames | No |
| `fits_set_keywords` | Set/unset/rename keys; `dry_run` defaults to true, returns diff | Yes |
| `fits_export` | Convert/resize/crop to an output path | Yes (new files only) |
| `fits_organize` | Plan (and optionally apply) moves/renames | Yes, plan-first |

MCP safety: a configurable allowlist of root folders, dry-run-first for any mutation, and outputs always to new files. Resources expose `fits://keywords` (dictionary) and `fits://quirks` so an agent can explain headers.

## GUI design language

Fittle borrows Nebulis's deep-navy canvas, warm amber accent, pill controls and uppercase micro-labels, and ships them as a shared token package so Stardog and Fittle stay in lockstep.

**Design tokens (proposed `@astrodog/tokens`, shared with Stardog)**

| Token | Dark | Light | Use |
| --- | --- | --- | --- |
| `bg` | `#0A0E16` | `#F6F4EF` | App canvas |
| `surface` | `#111827` | `#FFFFFF` | Cards, panels |
| `surface-2` | `#172033` | `#EEF0F4` | Inputs, pills |
| `line` | `#1F2A3D` | `#DDE1E8` | 1 px borders |
| `text` | `#E8ECF3` | `#141A26` | Primary text |
| `muted` | `#8391A7` | `#5B6678` | Labels, captions |
| `accent` | `#F5B83D` | `#C98A0C` | Primary action, focus, selection (Nebulis gold) |
| `nebula-orange` | `#E9894A` | same | Supernova remnant / warning chips |
| `nebula-rose` | `#E0567A` | same | Emission nebula chips, clipping |
| `nebula-teal` | `#3FC1C9` | same | OIII / info, "online" |
| `ok` | `#3DD68C` | `#1A9E5E` | Success, visible |

**Type:** a characterful geometric display face for titles and big numbers (Nebulis-style; candidates: Bricolage Grotesque, Space Grotesk), Inter for UI, JetBrains Mono for keyword cards. Uppercase 11 px tracked labels (`OBJECTS`, `SUB-FRAMES`).

**Layout:** three panes: collapsible **file rail** (left, thumbnails + filters), **image stage** (centre, edge-to-edge, floating pill toolbar), **inspector** (right, tabs: Overview · Header · Histogram · Stars). A bottom **filmstrip** appears for folders (like Nebulis's lightbox, but virtualised for 10k+ items).

**Motion and detail:** 150 ms ease-out transitions, radius 14 px cards / 999 px pills, soft inner glow on the stage, subtle starfield only on empty states. **Command palette** (⌘K / Ctrl K) for every action. Native window chrome on macOS (traffic lights in the title area), custom on Windows/Linux.

**Light mode** exists for daytime editing, but dark is the default and the one we design first; a **red night-vision mode** (all tokens mapped to deep red) is a nice field touch.

## Mocks

Fifteen high-fidelity screens live in the [Fittle Mocks](https://claude.ai/artifact/PtGxHuV34SRZR9rRFmQcxj) page, drawn with the tokens above on sample Seestar NGC 6995 data. Local copies: `docs/mocks/fittle-mocks.html` (open in a browser) and one PNG per screen in `docs/mocks/screens/`.

| # | Screen | What it proves | Milestone |
| --- | --- | --- | --- |
| 1 | Sub overview | Three-pane layout, auto-STF, verdict + evidence, canonical Target / Equipment / Conditions cards, pixel HUD, filmstrip | M2 |
| 2 | Stacked file | Integration breakdown (3,412 × 20 s = 18 h 57 m), processing-state checklist from HISTORY, hand-off buttons to Siril / Stardog / Nebulis | M2 |
| 3 | Header editor | Grouped keyword table with meanings, locked structural keys, change bars, staged diff, safe-write options and impact warnings | M3 |
| 4 | Histogram and stretch | RGB histogram, black/mid/white handles, clipping overlay, per-channel stats | M2 |
| 5 | Export modal | Format tiles, stretch choice, long edge, filename template, XMP + privacy toggles, live preview | M4 |
| 6 | Batch edit | 3,785-file selection, per-key value distribution, rig profile, queued ops, dry run, "Export plan as CLI" | M3 |
| 7 | Session report | Tiles, HFR-per-sub scatter across six nights with reject threshold, worst subs, consistency checks | M6 |
| 8 | Blink | Locked-stretch playback, satellite-trail flag, Keep / Reject with keyboard | M6 |
| 9 | Calibration match | Light groups vs a calibration library with plain-language mismatch reasons | M6 |
| 10 | Header diff | Side-by-side keys with impact column ("blocks calibration") | M1 / M3 |
| 11 | Command palette | Fuzzy actions + keywords, each showing its CLI equivalent | M2 |
| 12 | Themes | Dark (default), light, red night-vision | v1.0 |
| 13 | CLI | `fittle info` and `fittle scan --grade` terminal output | M1 / M6 |
| 14 | MCP in Claude | `fits_scan_folder`, `fits_preview` returning an image, dry-run `fits_set_keywords` | M5 |
| 15 | Quick Look | Finder preview with stretched image and verdict panel | v1.0 |

**Layout wireframe (main window, 1440 × 900)**

```
┌─ title bar ── folder › file.fit ────────────────────────────── [⌕ Search ⌘K] ─┐
│ FILE RAIL 232px │          IMAGE STAGE (fluid)         │ INSPECTOR 356px │
│ Folder · 3,785  │   ( Auto STF | Linear | Asinh │ Fit ) │ Overview Header  │
│ [All][Lights]…  │                                      │ Histogram Stars  │
│ ▣ sub-0001  ●   │                                      │ ┌ Verdict  97 ┐ │
│ ▣ sub-0002  ●   │          stretched image             │ │ evidence…  │ │
│ ▣ sub-0003  ●   │                                      │ ├ Target      ┤ │
│ ▣ sub-0004  ○   │                                      │ ├ Equipment   ┤ │
│ … virtualised   │ x 612 y 1044 ADU 1,284 RA… Dec…      │ └ Conditions  ┘ │
├─ FILMSTRIP (folders only) ─ ▣ ▣ ▣ ▣ ▣ ▣ ▣ ▣ ▣ ▣ … +3,771 ─────────┤
```

Imagery in the mocks is procedurally generated; replace with real corpus files once M0 lands.

## Cross-platform build, distribution, testing

GitHub Actions builds and signs every platform from one tag; the test corpus of real capture-app files is the project's most valuable asset.

**Distribution**

| Platform | GUI | CLI / MCP |
| --- | --- | --- |
| macOS (arm64 + x64 universal) | Signed + notarized `.dmg`; Quick Look extension bundled | Homebrew tap (`brew install fittle`), bundled in app |
| Windows 10/11 (x64, arm64) | Signed `.msi` + portable `.zip`; thumbnail handler optional in installer | winget, Scoop |
| Linux (x64, arm64) | AppImage, `.deb`, `.rpm`, Flatpak (Flathub) | tarball, `cargo install fittle-cli` |

One-line MCP setup in docs for Claude Desktop / Claude Code: `claude mcp add fittle -- fittle mcp`. Auto-update via Tauri updater with GitHub Releases (opt-in).

**Testing**

- **Corpus**: 1–3 real files per capture app and frame type (Seestar, ASIAIR, NINA, SharpCap, Ekos, APT, SGP, Dwarf, Siril/PixInsight/DSS stacks, calibration masters, `.fz`, multi-HDU, malformed). Ask the community for contributions with a privacy scrub script.
- **Golden tests**: `fittle info --json` snapshot per corpus file (insta crate).
- **Round-trip invariants**: data-unit hash unchanged after any header edit; property tests over random edits (proptest).
- **Fuzzing**: `cargo-fuzz` on the header parser and tile decompressor.
- **Perf budgets in CI**: header open <5 ms, 60 MP float stats <150 ms, scan 5,000 headers <3 s.
- **UI**: Playwright against the Tauri web layer; visual snapshots of key screens in dark and light.
- **Matrix**: CI on macos-14, windows-latest, ubuntu-22.04 for every PR.

## Roadmap

Ship the read-only CLI first, because it hardens the core that every other surface depends on; the GUI follows once `fittle info` is right on the whole corpus.

| Milestone | Scope | Exit criteria |
| --- | --- | --- |
| M0 Spike (1 wk) | Workspace, header parser, `fitsrs` data read, `.fz` check, Tauri hello-world with WebGL float texture | Opens every corpus file; decision on FITS I/O |
| M1 CLI read (2 wk) | `info`, `header`, `diff`, classification + evidence, derived facts, `--json` | Golden snapshots pass for all corpus files |
| W1 Web edition (2 wk) | WASM build of core + `info`/`header` views, prerendered marketing/SEO front page, opt-in scrubbed header contributions, Railway deploy ([0003](decisions/0003-web-edition.md)) | Drop a Seestar/Dwarf/ASIAIR file in the browser and get the Overview with no upload; contribution lands in the review queue |
| M2 Viewer GUI (3 wk) | Three-pane app, auto-STF, histogram, pixel inspector, debayer preview, Overview + Header tabs | 60 MP opens + stretches <1 s on M1 Mac and mid-range Windows laptop |
| M3 Edit (2 wk) | Staged edits, validation, batch set, templates, privacy scrub, safe writes | Round-trip invariant + fuzzing clean |
| M4 Export (2 wk) | All formats, resize/bin/crop/rotate, fpack, share card | Visual diff tests on exports |
| M5 MCP (1 wk) | All read tools, preview images, dry-run writes | Works in Claude Desktop + Claude Code |
| M6 Folders (3 wk) | Scan/session report, sub grader, blink, filmstrip, calibration matcher, organize/rename | 3,785-sub Seestar folder scanned + graded <60 s |
| v1.0 (2 wk) | Installers, signing, Quick Look / thumbnails, docs site, sample corpus | Public GitHub release |
| Later | Watch mode, plate-solve, WCS annotations, XISF read, AstroBin CSV, night-vision theme |  |

## Open decisions and Claude Code kickoff

**Open decisions**

- [ ] License: MIT/Apache-2.0 dual (Rust norm, maximally reusable) vs GPL-3.0 (matches Siril's ecosystem, keeps forks open). Leaning MIT; confirm before first public release.
- [x] Frontend framework: React 19 + TypeScript + Vite, matching AstroSideKick ([0002](decisions/0002-frontend-react.md)).
- [ ] Name check: "fittle" on crates.io, Homebrew, winget, GitHub org, and domain.
- [ ] Shared token package name and repo (`@astrodog/tokens`?) and who owns it.
- [ ] Do Quick Look / Explorer thumbnails ship in v1.0 or v1.1?
- [ ] Telemetry: none (recommended) vs opt-in crash reports.

**Claude Code kickoff**

1. Create the repo with a `CLAUDE.md` holding: the non-goals list, the "never touch data bytes" invariant, crate boundaries, `--json` convention, perf budgets, and "every feature = core fn + CLI + MCP + GUI".
2. Drop this plan into `docs/PLAN.md` and the mocks into `docs/mocks/`.
3. First prompt: *"Scaffold the Cargo workspace per docs/PLAN.md. Implement fittle-core header parsing (80-char cards, CONTINUE long strings, multi-HDU), a keyword dictionary stub, and `fittle header --json`. Add testdata and insta snapshots."*
4. Then one milestone per branch, each ending with the corpus golden tests green on all three OSes.
