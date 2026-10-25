## The list model

Task 8's registered decision rule, written before any measurement was taken through the product
path: ship `ScriptModel` plus Qt's documented `ItemSelectionModel` only if BOTH an interleaved
fling on the scale fixture drops no more frames than the integer model does within the
run-to-run spread AND the scale `Pss` regression sits inside a 1 MiB budget; otherwise keep
`model: root.total` (rule 2 above) and hand-roll selection as an index set. A windowless probe
had already measured `ScriptModel` holding 100,000 integers at about 3.7 MB more `Pss` than the
same process holding nothing, so the prediction, registered in advance, was decline on memory.
This section is that prediction checked against the real application rather than a probe.

**Two scratch UI trees, one shared binary, because the model lives only in QML.** Arm I is the
shipped `ui/` unmodified, `ui/List.qml`'s `model: pane.total`. Arm S patches `ui/List.qml` to
`model: rowModel`, a `ScriptModel` filled through one `setRowCount(n)` function that assigns `[]`
before the new array, because a single disjoint replace of 100,000 values diffs quadratically
(about 294 s in the windowless probe; clear-then-fill there cost 17 ms). Both arms launched
`flea --gui` against the same `target/release/flea`, so only `FLEA_UI` differed. Nine interleaved
pairs on the scale fixture (`/home/flea-sandbox/flea-bench-btrfs`, 100,000 files), `Pss` read from
`smaps_rollup` in KiB after settling to a 256 KiB band held for three 300 ms samples, order
alternated pair to pair:

| | Arm I median (KiB) | Arm S median (KiB) | delta |
|---|---|---|---|
| Pss | 76,308 | 78,064 | **+1,756 KiB, 1.71 MiB** |
| Pss_Anon | 37,400 | 39,052 | +1,652 KiB |
| USS (Private_Clean + Private_Dirty) | 54,988 | 56,704 | +1,716 KiB |

Arm S cost more `Pss` in **9 of 9 pairs**, fully disjoint from Arm I in every one (per-pair delta
567 to 1,925 KiB, no overlap). This box permanently holds a resident `quickshell`, which is the
sharing condition `Pss` divides against.

**Ruling: `ScriptModel` is declined on memory**, per the registered rule: 1.71 MiB median against
a 1 MiB budget, over budget in 9 of 9 pairs. The fling half of the gate was not run. The ship rule
needs both halves to pass and this half already fails on its own with no overlap across nine
pairs, so a second box-time measurement on the scale fixture cannot change an outcome the first
one already settled; this project already used the same reasoning to stop the row-mark gate once
its own decisive axis (the toolchain) made the rest of that measurement moot. `ui/List.qml` keeps
`model: pane.total`, unchanged. Task 9's selection is a hand-rolled index set in
`ui/js/Selection.js`, not `ItemSelectionModel`.

**A count change keeps the drawn delegates.** Assigning a new integer to a view's `model` goes
through `QQuickItemView::setModel`, which clears the view: every item is released as not reusable,
the position goes back to the start, and `refill()` builds every visible delegate again, so
`reuseItems` never applies. Every filter key and every live search answer paid that rebuild (11 of
12 drawn rows on a narrowing in `tests/listnarrow.sh`, before the change). `ui/StableModel.qml` is a
`DelegateModel` over a fixed integer of 100,000,000 rows with a `shown` group holding the first
`total` of them, and the view filters on that group. A new count adds or removes rows at the
group's tail, so the delegates that stay drawn are kept and re-bound, and the model still holds no
value per row. It keeps what the reset did: the view goes back to its start and to row 0, and the
Grid, whose current row follows the cursor, is placed on that row after the change. A move asked
for from inside the change's own layout is dropped, which is why the Grid places itself on the
model's `resized` signal. `tests/listnarrow.sh`, `tests/gridnarrow.sh` and `tests/columnnarrow.sh` gate it: a narrowing
and a widening build no delegate, every kept delegate draws its new row, and no pooled one lights
a cursor, rename or drop mark.

**A search's first answer builds nothing.** A folder holding fewer rows than the viewport holds
fewer delegates than a search's first answer draws, so that answer built the rest, and every row
built its location layout as it located: on `tests/search-warm.sh`'s fixture, 62 rows and 895
items in the list, 80 and 1,200 in Columns, 160 and 1,200 in the Grid, all in the answer's frame.
One turn after the frame that first draws the search strip, `ui/PaneSwap.qml` sets `searchWarm`.
Every row then builds its location layout hidden (`ui/Row.qml` `locateArmed`, `ui/ColumnRow.qml`
`markArmed`), and each view's `ui/StableModel.qml` `fill` grows its count to the rows a full layout
from the top builds and shrinks it back in the same turn, with the cache buffer added to the
display margin for that pass, so those rows build at once instead of incubating for rows the shrink
removes. The delegates it laid out wait culled in the view's reuse pool, which only a geometry
change or hiding the view drains, and the answer that grows the count takes them. No frame sees the grown count. The
gate checks that the first answer's frame builds no row and no item and destroys none in any view,
and that the warm turn draws nothing the strip's frame did not.

**A new listing reuses the old screenful from the pool.** A shown view keeps its pooled delegates when it
empties, and the next listing's rows take them back instead of building anew (warm Enter built 43, 50 and 163
delegates in List, Columns and Grid; it builds 0). Emptying lays out on its own (`forceLayout`), because Qt pools a
layout's removed rows only after it builds the inserted ones. Only a hidden view drains its pool
(`StableModel.drainHidden`, on the count reaching 0 or on `visibleChanged`). A pooled delegate is parked:
`ListView.onPooled`/`GridView.onPooled` clear its `live` flag, and every binding that follows the listing reads its
own last value while parked: the row, thumbnail, size, clip mark, kind names, search run, filter and Recent flags,
and Columns' `lifted` trail, which follows the pane's path. `listingIndex` is -1, cursor and selection are false and
its `HoverHandler` is off. `renaming` and `dropTarget` test that -1 first, and Columns' `renaming` and `errorGrowth`
the false cursor, so none of them reads the pane or the drag further. What a parked row still reads is view-wide:
List's `paneFocused` and `dualMode`, the pane reference itself, each view's shared widths, column set and name
budgets, and the drag session's copy and link flags. These follow focus, the window, dual mode and Recent, never a
listing's rows, so a plain Enter or Back re-runs none of them on a parked row; entering or leaving Recent re-runs the
column set on parked rows as on live ones. `onReused` sets `live` before the first frame, so a reused row draws its
new row and no stale mark. `tests/poolpark.sh` gates it in all three views: a warm Enter, and the clear of a filter
narrowed far enough to park delegates, build no delegate, and no parked or reused delegate shows a cursor,
selection, rename, hover or thumbnail that is not its own. Each arm is first seen to light on its own row (a rename
on the cursor row, a hover driven by a synthesized pointer, a thumbnail file per row, compared both as the delegate's
`thumb` and as the path its `Image` draws), and a positive control shows the past-read count sees a delegate's read.

**A parked row still arms its marks, and a pooled Grid tile keeps the editor it held.** The match and location
arming (List's `markArmed` and `locateArmed`, the Grid's `markQuery` and `markArmed`, Columns' own `markQuery` and
`markArmed`) stays live on parked rows: it follows the strip, never a listing's rows, so a pooled row builds its mark
with the strip and not on a later keystroke. A Grid tile owns its park flag (`ui/GridTile.qml` `pooled`, which
`ui/GridArea.qml` reads as `live`), so its editor state is set in the same handler as the park. A tile pooled while
it holds the rename editor records the row in `pooledEditorRow` and keeps that editor, hidden, while the pane's edit
stays on that row, so the same-row replacement takes over the draft and selection (`tests/rename-grid-bottom.sh`).
Like every parked delegate its `listingIndex` reads -1 and its `renaming` false, and `editing` covers the held editor
for the tile's own faces. A pooled tile's Loader is active only for the row recorded when it was pooled, and the
record clears when the edit leaves that row or the tile is reused, so a pooled tile never builds an editor.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
