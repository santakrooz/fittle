# 0003 — Free web edition with opt-in header contributions (proposed)

Date: 2026-09-25 · Status: **proposed** — build the local utility first; design the web
edition once the desktop app is solid. Recorded now so early choices keep it possible
(pure-Rust core → WASM; UI talks to the core through a `Backend` interface).

## Context

The corpus of real capture-app files is the project's most valuable asset, and we lack
files from Dwarf, Vaonis, Unistellar and many desktop capture apps. A free web edition
can grow it while also marketing the desktop app.

## Proposal

- **Web edition** of the same React UI, with `fittle-core` / `fittle-image` compiled to
  WebAssembly. Files are parsed **in the browser**; nothing is uploaded by default.
- **Opt-in contributions only.** When the verdict is weak or the capture app is unknown,
  offer "Contribute this header". The privacy scrub runs first and the exact payload is
  shown as a diff. Header-only by default (a few KB); pixel data is a separate explicit
  opt-in with a licence choice (e.g. CC BY 4.0) and optional credit.
- Contributions land in a review queue and graduate to `testdata/real/`.
- **Hosting: Railway** (candidate). One small Rust (axum) service serves the prerendered
  marketing/SEO front page, the static WASM app, and `POST /api/contributions` into
  object storage. Front page and docs pages are prerendered HTML (not a client-only
  SPA) so they are indexable.

## Deviations from PLAN.md

PLAN.md says "no cloud account or telemetry; network only for optional catalog or
plate-solve lookups". This stays true for the desktop app and the CLI/MCP. The web
edition adds exactly one network action, user-initiated contribution, and no
analytics beyond privacy-respecting aggregate page counts (if any).
