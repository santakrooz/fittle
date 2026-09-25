# VerdictCard

The Overview's first card: what the file is, how sure we are, and the header facts that say so.

- The consumer supplies `kind` (`sub` or `stack`), `label` ("Light sub", "Stacked · 3,412 subs"), `detail` (one line: exposure and source), `confidence` 0–100 and `evidence` as `KEY=value` strings (at most six).
- Confidence is a `mono-sm` pill in `ok-fg`. Evidence chips are `mono-xs` on `surface-2`; the key is muted, the value is `text`.
