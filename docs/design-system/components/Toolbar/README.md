# Toolbar

The floating pill toolbar over the image stage: stretch modes, zoom, overlays. It is the only control surface that sits on the image.

- The consumer supplies `Toolbar.Button` children (`on`, `icon`, `label` for icon-only buttons) and `Toolbar.Separator` between groups: stretch · zoom · overlays.
- Active buttons use `accent-soft`/`accent-fg`. Icon-only buttons need `label` (it becomes the tooltip and accessible name).
- Place it top-centre, `space-14` from the stage edge.
