# Adopting the look in AstroSideKick

AstroSideKick shares Fittle's stack (Tauri 2, React 19) and should share its look. Swap its
GitHub-dark palette for these tokens, then move its components to the patterns in this system.
Do it token-first: most screens follow once the variables change.

## Token mapping

| AstroSideKick today | Value | Use this token | Notes |
| --- | --- | --- | --- |
| `--bg` | `#0f1216` | `bg` | Canvas. |
| `--panel` | `#171b21` | `surface` (cards) · `panel` (side panes) | Split the one variable by role. |
| `--panel-2` | `#1e232b` | `surface-2` | Pills, chips, selected rows. |
| `--border` | `#2a313b` | `line` (hairlines) · `line-strong` (control borders) | |
| `--text` | `#e6e9ee` | `text` | |
| `--muted` | `#8b95a5` | `text-muted` | Also for uppercase labels. |
| `--accent` | `#58a6ff` (blue) | `accent` (amber) | Blue leaves the palette. Informational blue → `nebula-teal`. |
| `--ok` | `#3fb950` | `ok` / `ok-fg` | |
| `--warn` | `#d29922` | `warn` | |
| `--bad` | `#f85149` | `bad` | |
| `system-ui` | | `ui` (Inter) | Bundle with `@fontsource-variable/inter`. |
| `ui-monospace, Menlo` | | `mono` (JetBrains Mono) | Keywords, values, logs, Siril commands. |
| — | | `display` (Bricolage Grotesque) | New: page titles, object names, big numbers. |

## Pattern changes

- **Buttons and toggles become pills** (`Button`, `Chip`, `Segmented`). One primary amber button
  per view; the rest `secondary` or `ghost`.
- **Cards** get `radius-card`, a `line` border and no shadow. Card titles in `body-strong`, their
  fields as `Field` (uppercase `label` over a value).
- **Section headings** use `display-title`; counts sit beside them as muted `caption`.
- **Tables** follow the Data display rules: uppercase `label` headers, `line` row rules at 60%,
  `mono` for keywords and numbers, tabular figures.
- **Stats strips** become `StatTile` rows (big `display-number`, `label` under it).
- **Links** are `accent-fg`, not blue; external links keep the ↗ glyph.
- **Status** keeps its semantic colours but always with a word ("Kept", "Rejected", "Blocks
  calibration").
- **Images** sit on the stage glow (`stage-center` → `stage-edge`), not on a panel.

## Shared package

Generate CSS variables for both apps from this system's `tokens.json` into one package (working
name `@astrodog/tokens`), so neither app hard-codes a hex value.
