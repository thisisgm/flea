## How the list renders

Language marks default on through the persisted `langMarks` boolean in `src/uischema.rs`; saved false stays off.
The file and background menus offer Hide code icons directly after Show hidden files, and Settings > View
has the same switch. `ui/ViewState.qml` shares it across panes and windows. Off, `ui/Row.qml`,
`ui/GridTile.qml`, `ui/ColumnRow.qml` and `ui/QuickLookBar.qml` keep the generic glyph slot and create
neither a `LangMarkLoader` nor a `LangMark`; toggling changes the slot in place without a re-list.

`ui/Pane.qml`'s `ListView` sets `clip: true` because the top row of a wheel-scrolled viewport is
only partly inside the list, and the rest of it draws over the column header: the cursor's accent
band, its edge bar and the row's filename all landed on top of the header labels and the header
rule. `tests/ui.sh cursor` counts the accent pixels in the header band to hold that closed.

**It also sets `reuseItems: true`, and that is safe here only because nothing in the delegate
depends on setup that runs once.** Every property the delegate draws, save `cursor`, is a
binding that reads `index`, so a row leaving the small `cacheBuffer` is re-bound rather than destroyed and rebuilt.
`Component.onCompleted` does NOT re-run on reuse, so imperative setup added there has to be
repeated from a handler that fires on reuse (`onListingIndexChanged`, `ListView.onReused`) or move
to a binding, or this line has to go. The one value set imperatively is `cursor`: `ui/CursorMark.qml` writes it on the delegate
the cursor leaves and the one it reaches, the delegate's `Component.onCompleted` files a new
delegate, and `onListingIndexChanged` refiles a reused one, whose `listingIndex` moves on reuse. Measured in Plan 5 Task 5a on the
100,000-file fixture, nine interleaved pairs each driving 2400 wheel detents then 400 single-row
`j` steps: the fling cost 1.60 s of process CPU without reuse and 1.46 s with it, reuse winning
every one of the nine pairs, and the row-by-row steps cost 0.94 s against 0.72 s, again nine pairs
out of nine, about a quarter less on the churn case reuse exists for. Input to rows for one `G`
jump, stamped inside the UI, went from a median 39 ms to 37, reuse winning eight pairs and tying
one. It is not free, and the direction reverses here: peak PSS after that fling rose from
76,146 KiB to 77,299 KiB, reuse the WORSE arm in all nine pairs, about 1.1 MiB of pooled
delegates, `pss_kb` over 1024. **That cost does not reach either
field column**, because the benchmark samples PSS from launch to settle and never scrolls:
first-paint PSS moved by a median 0.16 MiB on the scale fixture over five interleaved pairs, with
per-pair deltas of 16 to 626 KiB against a 455 KiB spread inside the base arm, and by 0.06 MiB on
the media fixture over nine, where reuse is the higher arm in 6 of 9 and the lower in 3, and the
57 KiB median-to-median sits well inside the base arm's own 727 KiB spread. Call it a fifth of a
megabyte at worst where the fling costs 1.1.

The correctness hunt was the point rather than the timing. A delegate that failed to re-bind shows
the wrong icon on a row, so the check pairs delegate state against model state through the
read-only seam: `rowIcon(i)` is what the row draws and `thumbFile(i)` is what the pane holds for
that index, and they must agree for every instantiated row. A purpose-built fixture of 100 jpegs
and 100 text files interleaved by name puts both kinds on every screen. Zero violations across
eight dumps, with and without reuse, after a hard fling and after scrolling out past the buffer and
back to the top. The dumper and the comparison both live at `~/bench/t5a-rows.sh` and
`~/bench/t5a-checkrows.py`, outside the repo, so the next session re-runs them rather than
rebuilding them.

**The comparison was proved able to fail twice, and the second control is the one to copy.** A
mutation of a saved dump, swapping two rows' drawn icons and relabelling a thumbnailed row as
`.txt`, reddens it with three violations. Stronger, and run live on the box: an arm whose delegate
takes `thumb` in `Component.onCompleted` instead of binding it, which is exactly what a real reuse
break looks like, produced **21 violations across the 21 thumbnailed rows of the screen while the
qs log still reported `LOGCLEAN`**. **The log check alone would never have caught a live reuse
break**, so anything that touches delegate reuse runs this comparison and not a clean log.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
