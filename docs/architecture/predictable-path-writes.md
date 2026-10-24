## Predictable path writes

`dest` sits in a shared runtime directory, so its path is guessable and could be
pre-planted as a symlink. `write_prewarm` writes to `<dest>.<pid>.tmp`: it unlinks any
leftover of its own first, then opens with `O_CREAT | O_EXCL` so a planted path cannot
be followed, and the pid in the temp name keeps two concurrent launchers off each
other's file. The file is created at mode `0o600` in the `open()` call itself, never
chmod'd afterward, so there is no window where it is readable by anyone but the owner.
`fs::rename` runs last, so a reader only ever sees a complete file or none at all. On
any failure, only the temp file is removed; `dest` is left exactly as it was, because
unlinking a caller-supplied path is not this function's job even when that path came
from a broken caller. The exit status is the contract: a caller must check it before
trusting whatever `dest` currently holds.

The gvfs outcome file beside it is the same shape with a claim on top. `flea --launch-warm`'s
gio half writes either gio's listing bytes (hidden included, so either scan can adopt) or the one-line
failure marker `flea-gvfs-fail`, both through its own exclusive `0600` temp plus rename, so a
reader never sees a partial. The first backend claims with `rename(dest, dest.claimed)` and reads
the claim, which it keeps; a reader finding the claim, or finding neither dest nor claim more than
2 s after the launch start, returns None at once and the scan takes today's gio path. Staleness is
judged against the launch: mtime older than `FLEA_GVFS_START` is refused, and freshness is age under
10 s against a clock read after the wait, never before it. The sweep reaps dest and claim leftovers
by the same 60 s age rule. The helper forks once for both jobs, so the launcher waits only
for the fork and qs inherits no zombie; `gui.rs` exports `FLEA_GVFS_*` only when that spawn succeeded.
The terminal interface takes the same head start: `tui/warm.rs` runs `prepare` and the helper with "-" for the
page-cache list, and `tui/wire.rs` passes `FLEA_GVFS_*` to its backend only when that spawn succeeded.
A local or USB launch pays only the `is_gvfs` prefix check in `prepare` and arms nothing.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
