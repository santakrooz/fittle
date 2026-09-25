Deep-space dark, one amber light, and the image as the hero. Fittle's look is quiet
furniture around astrophotographs: near-black navy panes, hairline borders, pill controls,
uppercase micro-labels, and a single warm accent that always means "on" or "do this".
It is dark-first; light exists for daytime editing and a red night-vision theme keeps
eyes dark-adapted at the telescope.

## Principles

- **The image is the hero.** The stage runs edge to edge on `stage-center` → `stage-edge`
  (a radial glow), never on `surface`. Chrome floats over it (toolbar, HUD) or sits beside it
  (rail, inspector). No decoration competes with pixels.
- **One amber light.** `accent` marks the active state, the primary action and focus. At most
  one filled amber control per region; active pills use `accent-soft` with `accent-fg` text.
  If two things are amber, one of them is wrong.
- **Explain, and show the evidence.** Every claim sits beside the facts that produced it
  (evidence chips in `mono-xs`), every computed value carries a teal `DERIVED` tag, and every
  number has its unit.
- **Quiet by default.** Borders, not shadows, separate panes and cards (`line`). Shadows are
  for things that float: the palette, modals, the fitted image.
- **Keyboard-first.** Every action is in the ⌘K palette, and each palette row shows its CLI
  equivalent in `mono`.

## Content

- Plain English, sentence case: "Open a stack", "Reset to Auto STF", "Export with this stretch".
  Uppercase only in micro-labels (`label`): `SUB LENGTH`, `FILTER`, `FOLDER · 3,785`.
- Units always, with a thin space where it reads better: `20 s`, `250 mm`, `2.39″/px`,
  `28.4 °C`, `18 h 57 m`. Use real primes (″ ′) and `×`, `·`, `−`, `→`, `Δ`.
- Say where a value came from. Header facts appear as `KEY=value`; defaults say whose
  ("used Seestar S50 default (250 mm)"); derived values say "derived".
- Warnings name the fix: "FOCALLEN=0 in header; used Seestar S50 default (250 mm)".
  No apologies, no exclamation marks, no emoji.
- Numbers use grouping (`3,785`) and tabular figures wherever they line up.

## Colour

- Grounds, back to front: `bg` (canvas) → `panel` (rail, inspector) → `surface` (cards) →
  `surface-2` (pills, chips, selected rows) → `surface-3` (hover, nested wells).
  `bg-raised` is for chrome and input wells.
- Text: `text` for values and primary copy, `text-muted` for captions, labels and inactive
  tabs. `text-dim` is never text a person must read (3.0–3.6:1); use it for disabled icons and
  decorative separators only.
- Status: `ok`/`ok-fg` (good, match), `warn` (attention, Δ over tolerance), `bad`
  (blocks calibration, rejected, error). Always pair status colour with a word or an icon;
  the night theme has no red/green difference.
- Nebula hues carry meaning from the sky, not decoration: `nebula-orange` for supernova
  remnants and broadband, `nebula-rose` for Hα/emission and clipped highlights, `nebula-teal`
  for OIII, info and DERIVED. Use the tone for fills and chart marks, the `*-fg` token for text.
- Tinted badges: the tone at 14% alpha as ground, its `*-fg` as text.
- Light theme uses deeper `*-fg` values (amber `#c98a0c` is 2.9:1 as text; `accent-fg`
  `#8a5c06` is 5.8:1).

## Type

- Three faces, three jobs. **Bricolage Grotesque** (`display`) for titles, the verdict and
  big numbers only. **Inter** (`ui`) for everything you read in the chrome. **JetBrains Mono**
  (`mono`) for FITS keywords, values, the pixel HUD, CLI text and evidence.
- Default text is `body` (14/1.5). Controls use `control` (12.5px, 500). Sub-lines use
  `caption`. Labels use `label` (10.5px, 600, 0.14em tracking, uppercase, `text-muted`).
- Big numbers use `display-number` with `font-variant-numeric: tabular-nums`; a smaller
  unit follows in `text-muted` ("18 h 57 m" with h/m muted is acceptable).
- Desktop apps bundle the fonts (Fittle uses `@fontsource-variable/*`); never load fonts
  from the network at runtime.

## Shape, space and layout

- Controls are pills (`radius-pill`): buttons, chips, tabs, segmented controls, the floating
  toolbar. Cards are `radius-card` (14px). Inputs and list rows `radius-sm`. Evidence chips
  and kbd `radius-xs`.
- The main window is three panes: rail `layout-rail` (232px) · stage (fluid) · inspector
  `layout-inspector` (356px), under a `layout-titlebar` (42px) title bar with a breadcrumb and a
  ⌘K search pill. Folders add a `layout-filmstrip` (88px) strip across the bottom.
- Pane padding `space-14`; card padding `space-12` × `space-14`; gaps between cards `space-12`;
  label/value grids `space-10`.
- The toolbar floats top-centre on the stage, a translucent `surface` pill with a blur and a
  `line-strong` border; groups are split by 1px `line-strong` separators. The HUD sits
  bottom-left in `mono-sm` on translucent `bg` boxes.

## States and motion

- Hover: border to `line-strong` or fill to `surface-3`. Active/selected: `accent-soft` +
  `accent-fg`, or `surface-2` + `text` for neutral selection (tabs, list rows with an inset
  1px `line-strong` ring).
- Focus: a solid 2px `accent` outline with 2px offset on every focusable control (≥9:1 on
  dark surfaces). Never remove it.
- Editing a value: `accent` 1px border plus `shadow-focus-halo`.
- Header edits: modified rows get `accent-soft` with a 3px inset `accent` bar on the first cell;
  added rows `ok`; deleted rows `bad` with strike-through.
- Motion: `duration-base` (150ms) ease-out for hover, press and tab changes; nothing slower in
  the chrome, no bounce. Respect `prefers-reduced-motion`. The only ambient motion allowed is a
  subtle starfield on empty states.

## Data display

- Header tables: keywords in `accent-fg` `mono`, values in `text` `mono`, comments in `ui`
  `text-muted`, group rows as `label` on `bg-raised`, structural keys with a lock.
- Confidence: a `mono-sm` pill in `ok-fg` on a 10% `ok` tint with a 30% `ok` border.
- Evidence: `mono-xs` chips on `surface-2` with a `line` border, key muted, value `text`.
- Derived values: a teal `DERIVED` micro-tag after the label, 9.5px, 0.08em tracking.
- Charts: histograms per channel in `nebula-rose`/`ok`/`nebula-teal` at ~35% fill with a solid
  line; the STF curve dashed `accent`; clipping overlay blue for shadows, `nebula-rose` for
  highlights.

## Iconography

- Line icons, 1.7px stroke, round caps and joins, `currentColor`, drawn at 15–19px (Lucide
  style). No filled icons, no emoji, no logos of other products inside the chrome.
- The verdict icon sits in a 38px `radius-md` tile tinted with its meaning (teal for a sub,
  `accent-soft` for a stack).

## Themes

- **Dark** (default, designed first). **Light** for daytime: same structure, deeper text-safe
  tones. **Night vision**: every token maps to deep reds on near-black; status relies on words
  and icons. Switch with `data-theme` on the root.
