# FileRow

A row in the file rail: thumbnail, file name and a status line with a coloured dot.

- The consumer supplies `name`, `detail` ("Light · 20 s · HFR 2.1"), `status` (`ok`, `warn`, `rejected`) and `selected`.
- Long names truncate from the end with an ellipsis; the rail may show a leading "…" for shared prefixes. Rejected files are struck through and muted, and keep their dot.
