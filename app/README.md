# Fittle desktop app (Tauri 2 + React 19)

The UI is a thin face over the Rust crates: it reaches them only through the
`Backend` interface (`ui/backend/types.ts`). Styling comes from the Fittle design
system (`docs/design-system/`); `ui/styles/tokens.css` is generated from its
`tokens.json` by `node scripts/tokens.mjs`.

## Run

    pnpm install
    pnpm dev                      # Vite on :1430
    cargo run -p fittle-app -- "/path/to/file.fit"   # in another terminal

`FITTLE_TRACE=1` prints timings for opening a file (Rust) and time to first frame (UI).

## Demo mode (browser, no Tauri)

Generate fixtures from local FITS folders (they may carry site coordinates, so they
stay out of git), then open `http://localhost:1430/?demo`:

    FITTLE_DEMO_MAX=5 cargo run --release -p fittle-image --example ui_fixtures -- "<folder>" "<folder>"…

Fixtures land in `app/demo-fixtures/` and are served only by the dev server.

## Layout

- `ui/state/`: the store (`app`, `hover`), actions, stretch maths.
- `ui/viewer/`: WebGL2 renderer (half-float textures, stretch in the fragment shader),
  view transform, the stage with zoom, pan, full-resolution detail on zoom and pixel readout.
- `ui/panes/`: title bar, file rail, stage chrome, inspector tabs, command palette.
- `ui/ds/`: design-system components (typed ports of the style guide's bundle).

## Keys

⌘K / Ctrl K palette · ↑↓ previous/next file · F fit · 1 actual pixels · + − zoom ·
A auto · L linear · H asinh · C clipping · D debayer · double-click toggles fit / actual.
