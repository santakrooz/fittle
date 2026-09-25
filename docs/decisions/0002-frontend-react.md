# 0002 — Frontend: React 19 + TypeScript + Vite

Date: 2026-09-25 · Milestone: M0

## Decision

The GUI uses React 19 with TypeScript, Vite and `@vitejs/plugin-react`, matching the
sibling app AstroSideKick (Tauri v2 + React 19, pnpm workspace with `@sidekick/*`
packages). No Svelte.

## Why

- Sharing tokens and components with the sibling app is the reason PLAN.md chose a
  web UI; Svelte and React components cannot be shared.
- One toolchain (pnpm, Vitest + Testing Library, eslint-plugin-react-hooks, Tauri 2).
- Fittle's hot paths do not depend on the framework: stretch runs in a WebGL2 shader,
  decode and stats run in Rust.

## Rules that follow

- High-frequency updates (stretch sliders, zoom, pixel HUD on mouse move) bypass React
  state: refs → shader uniforms, or a tiny external store read via
  `useSyncExternalStore`. The three-pane tree must not re-render at 60 Hz.
- Large lists (filmstrip, file rail, batch edit) use TanStack Virtual.
- The UI reaches the core only through a small TypeScript `Backend` interface, with a
  Tauri implementation now and a WASM implementation for the web edition (0003).
