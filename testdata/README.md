# Fittle FITS test corpus

Real FITS files from smart telescopes, capture apps and processing tools, used to test Fittle's parsing, classification and display. Seestar is covered best. The biggest gaps are the other smart telescopes (see Gaps below).

- `corpus.tsv`: the manifest. One row per file with its source URL, rig, software, frame type, license, and whether it can be redistributed.
- `fetch-corpus.sh`: downloads everything in the manifest into `corpus/`. It skips files already present. Pass a group prefix to fetch part of it, e.g. `./fetch-corpus.sh seestar`.
- `catalog.py`: reads every downloaded file's headers (plain, `.gz` or tile-compressed) and writes `CATALOG.md`. Uses only the Python standard library.
- `corpus/`: the files themselves (about 550 MB). This folder is git-ignored. Run `fetch-corpus.sh` to recreate it on another machine.

## Licensing

The `redistribute` column in `corpus.tsv` controls what may be committed or shipped:

- **yes**: MIT, BSD, Apache, CC-BY or Unlicense. Fine to commit with attribution.
- **copyleft**: the file comes from a GPL/AGPL repo. Only commit it if Fittle's license is compatible.
- **no**: no license stated. Use for local testing only.

## Coverage (109 files)

| Group | Files | What it covers |
|---|---|---|
| seestar-s50/raw | 9 | Raw 10 s subs; firmware 3.31 to 8.46; LP and IRCUT; mosaic and EQ mode |
| seestar-s50/stacked | 7 | On-device stacks: 1080x1920, 2x "enhanced" 3840x2160, mosaic 2304x1296; STACKCNT 169–588 |
| seestar-s30 | 8 | Seestar S30 raw 10 s subs and on-device stacks incl. mosaic (STACKCNT 180–820), CC-BY-4.0 |
| seestar-s30pro | 3 | Seestar S30 Pro raw subs (4K sensor 2160x3840, fw 8.46 and 9.16, 2 s and 20 s, OBJECT "Unknown"), CC-BY-4.0 |
| seestar-s50/processed | 2 | Siril 1.4 stack and a third-party float stack of Seestar data |
| asiair | 9 | ASIAIR Mini + ASI533MM Pro bias, dark, flat and light; ASIAIR Plus header-only frames (ASI183MC, Canon 1500D, mosaic panel) |
| stellina | 3 | Vaonis Stellina raw 10 s subs; EXPOSURE is in ms; BAYERPAT differs between rotated (`r`) and plain frames. CC BY-NC-ND, local only |
| dwarf-3 | 1 | DWARF 3 on-device stack (DWARFLAB-specific keys) |
| unistellar-evscope | 2 | eVscope v1 median stacks (device keys carried over) |
| nina | 3 | N.I.N.A. 3.1 and 3.2 with ASI533MC Pro, Svbony SV605CC and QHY268M |
| sharpcap | 3 | SharpCap 4.0 with ASI533MC Pro and ASI183MM (uses FRAMETYP instead of IMAGETYP) |
| ekos-indi | 4 | KStars/Ekos with ASI224MC and ASI1600MM Pro |
| phd2 | 1 | PHD2 guide frame, ASI120MC |
| maxim-dl | 10 | MaxIm DL 4.10 to 6.18 and Essentials 2.04; Apogee Alta and Aspen, Orion SSDSI; bias, dark, flat, light, RGB cube |
| pixinsight / siril / astro-pixel-processor | 6 | Re-saved frames, a WBPP master light (QHY600M), a Siril 0.9 master dark, APP integrations |
| sbig / celestron-nightscape / itelescope / dslr-panoptes | 5 | SBIG CCDOPS, Celestron AstroFX, iTelescope remote frame with WCS, Canon DSLR via POCS |
| mike/* | 26 | Mike's own data, local only. Seestar S50 raw subs for firmware 4.43, 4.70, 5.34, 5.50, 5.97, 8.46, 9.16 and 9.31; 4K LiveStack and new `DSO_Stacked_*` outputs; S30 Pro stacks; ASI2600MC Air bias/dark/flat/light; the same NGC 7380 data through Stardog/sidekick, wizardstack and GraXpert | 
| compressed-fz | 7 | Tile compression: RICE_1, GZIP_1, HCOMPRESS_1, PLIO_1 |

## Test cases this corpus already exposes

- **Seestar stacks say `IMAGETYP = 'Light'`.** Only `STACKCNT` shows they're stacks. The classifier must weigh STACKCNT above IMAGETYP.
- **An APP integration keeps N.I.N.A.'s `SWCREATE` and `IMAGETYP = LIGHT`** but has EXPTIME 9000 and BITPIX -32. Frame type, software and exposure keys can all be misleading after processing.
- **`maxim-dl/flattest.fit` is labelled `IMAGETYP = BIAS`.** Headers can lie, so show evidence and confidence.
- **Sanitised Seestar headers** (`TELESCOP = S50_00000001`, `DATE-OBS = 2000-01-01`) test the handling of placeholder values.
- **ASIAIR header-only files** have NAXIS=0 but keep the original dimensions. They test header-only reads.
- **Several software/frame keys are in use:** `SWCREATE`, `CREATOR`, `PROGRAM`, `SOFTWARE` and `ORIGIN`, plus `IMAGETYP`, `FRAMETYP` and `FRAME`.
- **Data layouts:** 3-plane RGB cubes, BITPIX 8/16/-32/-64, a PixInsight thumbnail extension, and gzip and tile-compressed files.

### From Mike's own files

- **Processing strips identity.** Siril exports such as `__state_export.fit` and `pal_b.fit` have no TELESCOP/INSTRUME/CREATOR at all, and "wizardstack" writes STACKCNT but no software key. Fittle should fall back to "unknown stack" with the evidence it has.
- **STACKCNT is not trustworthy after processing.** Siril's OIII extract doubles it (1468 from a 734-frame stack).
- **Siril `pp_` subs keep `CREATOR = ZWO Seestar S50`.** Only the filename prefix and HISTORY show they were calibrated.
- **Renamed files.** The `ELEPHANT ic 20s pau seestar 50 (1).fit` names carry nothing, so the header must do all the work.
- **Privacy.** TELESCOP embeds unit serials (`S50_xxxxxxxx`) and SITELAT/SITELONG hold real coordinates. Scrub these before any `mike/*` file is published.

## Gaps and leads

These are listed so the gaps can be filled later.

| Device | Status | Best lead |
|---|---|---|
| IKI Observatory (SGP + SX694) | in manifest, blocked here | Run `./fetch-corpus.sh sgp` from your own terminal |
| DWARF II, DWARF mini | none public | Ask r/DWARFLAB owners for 2–5 subs + stack under CC0/CC-BY |
| Vespera / II / Pro | none public | Ask r/vaonis; note the known ROWORDER=BOTTOM-UP quirk |
| Celestron Origin | none public (Drive folder removed) | Ask the Origin Facebook group / Cloudy Nights |
| Unistellar raw subs | only processed stacks | Owners can export with unistellar-data-downloader |
| APT, SGP subs, TheSkyX, FireCapture, ASIStudio, Voyager | none found | Ask on Cloudy Nights; Dropbox ASI294MC lead in CN thread 732010 |
| Siril tutorial set (ASI2600MC, 15 each L/D/F/B) | not downloaded (528–650 MB zips) | free-astro.org/download/colmic/ |
| Feder Observatory full calibration set | not downloaded (483 MB) | Zenodo 3245296, CC-BY-4.0 |

The quickest way to fill these gaps is to ask the community for files: a short post asking owners of each scope for 2–5 raw subs and one stack under CC0 or CC-BY.
