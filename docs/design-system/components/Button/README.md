# Button

Pill button for commands. Use `primary` (amber fill, `accent-on` text) for the one action a region exists for ("Export with this stretch", "Apply 3 changes"); `secondary` for the rest; `ghost` inside dense toolbars and dialogs' cancel.

- The consumer supplies the label (a verb, sentence case) and `onClick`; `icon` is optional (15px line icon, leading).
- Never two primary buttons side by side. Destructive actions are `secondary` with the word ("Delete 3 keys"), confirmed in place.
- Height 32px, `radius-pill`, `control` type.
