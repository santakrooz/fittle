# Fittle MCP server

`fittle mcp` runs an [MCP](https://modelcontextprotocol.io) server on stdio, so Claude
(or any MCP client) can inspect, preview and carefully edit your FITS files.

## Set up

Claude Code:

```bash
claude mcp add fittle -- fittle mcp --root ~/Astro
```

Claude Desktop (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "fittle": { "command": "/path/to/fittle", "args": ["mcp", "--root", "/Users/you/Astro"] }
  }
}
```

`--root` (repeatable) lists the folders the server may touch. Without it, Fittle uses
`FITTLE_MCP_ROOTS` (a path list), else your home folder. Every path, including glob
matches and output folders, must be inside an allowed root.

## Tools

| Tool | What it does | Writes? |
| --- | --- | --- |
| `fits_inspect` | Verdict (sub or stack, with confidence and evidence), rig, target, exposure, site, derived facts. Schema `fittle.info/1` | No |
| `fits_header` | Header records per HDU, optionally one HDU or a keyword filter | No |
| `fits_preview` | A stretched JPEG/PNG preview (image content), debayered if colour | No |
| `fits_stats` | Per-channel statistics, auto-stretch parameters, stars and HFR | No |
| `fits_diff` | Header diff between two files, with calibration impact | No |
| `fits_scan_folder` | Session report: frame counts, nights, integration per target and filter, consistency checks; `grade: true` adds per-sub grading | No |
| `fits_grade_subs` | Stars, HFR, background, trails per sub; suggested rejects with reasons | Moving rejects: dry run by default |
| `fits_match_calibration` | Light groups vs a calibration library: best darks, flats, bias/dark-flats, mismatch reasons, what to shoot | No |
| `fits_keyword_spread` | How each keyword varies across files: same, mixed (files per value), range, unique, missing | No |
| `fits_set_keywords` | Set, remove or rename keywords (paths and/or glob), or apply a rig profile (`rig`) | Header only; dry run by default |
| `fits_organize` | Sort into `{object}/{filter}/{night}` folders and/or rename from a template; companions follow; undo manifest | Renames only; plan first (dry run by default) |
| `fits_scrub` | Remove site coordinates, observer names and serials | Header only; dry run by default |
| `fits_export` | PNG/JPEG/WebP/AVIF/TIFF/FITS with stretch, crop, rotate, bin, resize, share card | New files only; dry run by default |
| `fits_fpack` | Lossless compress or expand (`unpack: true`) | New files only; dry run by default |

Every tool that writes returns a plan first (`dry_run` defaults to `true`). The agent is
told to show you the plan and only call again with `dry_run: false` once you agree.
Header edits never change pixel data and keep `.bak` backups by default; exports and
compression never replace an existing file.

Resources: `fits://keywords` (keyword dictionary), `fits://quirks` (software fingerprints
and vendor quirks), `fits://scopes` (smart-scope registry), `fits://rigs` (rig profiles).

