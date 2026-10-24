## Prewarm

`flea --prewarm <path> <first> <dest>` (`launcher/prewarm.rs`) runs the same scan, sort
and stat-range as the backend's `list` command, then writes exactly the two lines
`--backend` would have printed to stdout, a `listed` line and a `rows` line, to `dest`
instead; the `rows` line names `"listing":1`, the numbering of a backend's first listing, which is
the one the file stands in for. This producer and its secure file contract remain available for
measurement, but production ignores `FLEA_PREWARM`: the rejected reader was slower and nothing in
the file proves its directory unchanged since it was written (rule 4 above).

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
