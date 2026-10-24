## The listing swap

Opening a folder drew 3 to 4 frames of an empty list, 50 to 67 ms at 60 fps: a 60 fps
`gpu-screen-recorder` capture on the box showed it on 12 of 12 navigations into and out of a 300-file
folder, and one frame grab had the breadcrumb already on the new folder over an empty list.
`ui/js/Nav.js openWithoutHistory` cleared the pane at the request, and the rows came back a round trip
later. **The round trip is not where the time went.** In the container `flea --backend` answers a
300-file `list` with its `rows` line in 0.9 ms median over 35 requests, 4.4 ms at worst, and V4's
`JSON.parse` of that 30 KB line costs 0.9 ms. The frames were the pane's own: the reset lands inside
the input handler, so the next vsync presents an empty list however fast the reply is; `listed` moved
the breadcrumb and set the count before `rows` filled it, and the two lines can arrive in separate
reads, which is the grab's frame; and building the new rows' delegates holds the GUI thread for a
frame interval or more while the screen keeps the last frame presented, which was the empty one.

**A settled listing now stays drawn until the next one's rows land, and then everything lands in one
turn.** `ui/PaneSwap.qml` owns it and `ui/js/Swap.js` decides. `Nav.openWithoutHistory` asks
`pane.swap.hold(ask)`: a pane showing a settled listing, any `listingState` but `loading` with no query
line or walk on screen, holds, and every other pane runs `Nav.forget` at the request exactly as
before. While it holds, the `listed` line naming the directory asked for is kept rather than applied, and
its `rows` line runs `land()`: `Nav.forget`, the rows, the kept `listed` line (count, path, state,
`opened`), then what a rows reply always did (pending select, anchor, tab restore, settle,
`listInFlight` false), all in one JS turn. **Only the list's own `listed` line is kept.** A sort sent
just before the list answers first, a `listed` line naming the folder being left and then the window
`ui/js/Sort.js` asked for; keeping the first `listed` line after the request swapped the sort's rows in
as the new folder's. So while any listing is out, `Swap.onListed` drops a `listed` line whose path is
not `pane.listingPath`, and the `rows` behind it then arrive with no `listed` line seen and are dropped
as they always were. The path is a reliable key because a `list` answers with the directory exactly as
it was asked for, byte for byte: `run.rs` sets `st.base` from the request's own path string and
`listed_line` prints it, and `pane.listingPath` and the request are the same `newPath` (`Nav.js`).
A search never meets a hold: a live session answers with no listing out and a walk on screen
refuses one, so its opening line always lands unheld.
The path cannot tell a sort answered after a re-list of the same folder from that re-list, so there the
sort's reply lands the hold and the re-list's own lines land unheld over it, as every listing did before
the swap; the order converges, and a row request sent between the two names the sort's numbering and is
refused. The rows go in before the count, so each delegate the count builds is built on its row
rather than built empty and bound again. The count still passes through 0, which keeps the view's
reset to its top even between two folders holding the same number of rows.

**The hold ends four ways.** The swap above. A failed or refused listing: `ui/PaneWire.qml onFailed`
calls `Swap.failListing`, which runs `drop()` before it clears `listInFlight`, so `Nav.forget` runs and
the failure then draws the error or Locked state it always drew; a `stale` refusal ended only its own
request and ends nothing, and so does a `scan` failure naming a folder that is no longer the one asked for. An empty folder: its `rows` line rides right behind its `listed`
line (docs/protocol.md invariant 1), so it swaps into the empty state at once. And a listing still out
after `Swap.CAP_MS`, 66 ms, falls back: the cap runs `Nav.forget`, and `ui/LoadingState.qml` shows
its mark at once through `heldOff`, because the hold already spent that mark's own hold-off, which
`ui/PaneStates.qml` sets to `Swap.CAP_MS` too for a directory listing. 66 ms is 100 ms from the key to
the fallback's first frame less four 120 Hz frames: one for input, two for the QML Timer's 16 ms
resolution and one for the fallback's own draw. A
`listed` line kept before the cap, or arriving after it, waits for its `rows` line and lands with it
through `land()`, so the mark stands until the rows do rather than giving way to a count over no rows.
**Once drawn, the mark stands `Swap.MIN_SHOW_MS`, 100 ms, before the rows replace it**, so it never
flashes in and out: `ui/PaneStates.qml` tells the swap when the mark is drawn (`markShown`), a `listed`
line arriving while it is drawn is kept even when no rows were held, and `rows` lines that come sooner
wait in `waiting`, in arrival order, for the `minShow` timer and then land as they would have. A folder that lists in
under 66 ms never shows the mark; one that lists between 66 and 166 ms draws the mark at 66 ms and the
whole folder at 166 ms; a slower one draws the mark at 66 ms and the folder when it lands. `describe()`
reports how long the last mark stood as `shownMs`. A walk's matches never wait on the mark. The QML
mark holds still for its first second on screen and crawls only after it (`ui/Spinner.qml`), so a load shorter
than that never draws motion. `tests/spinner-still.sh` counts, never times, the frames the window swaps and the
`dashOffset` writes per mark, so a resting mark that asks for frames, or a crawl written faster than the window
draws (a timer in place of the animation), fails on any machine. That first-second hold belongs to the QML front
end only. By a 0.4.0 ruling the GPUI mark (`crates/flea-gui/src/feedback/spinner.rs`) crawls from the first frame
it draws, so it differs from `tests/spinner-still.sh` on purpose, and `tests/gpui-spinner.sh` counts its frames,
offset writes and frame requests instead.

**No row acts while a listing is out, held or not.** After the `list` request the backend numbers
every row for the directory asked for, so an action on a row by its index would land on another file.
That was true before the swap too, and it was reachable: in a loading state the rows are forgotten
but the cursor is 0, and `dd` sent `trash [0]`, which the backend applied to the new directory's first
file. So the rule covers the whole of `listInFlight`, the hold, the fallen-back loading state and a
slow network folder alike, in two layers. `ui/PaneStates.qml` lays a `MouseArea` over the header, the
filter strip and the listing slot while a listing is out, which takes every press and wheel notch and
says "A directory is already loading." on a press, and `ui/js/Focus.js handleKey` swallows every
listing key but `Swap.ANSWERED_WHILE_LISTING` and says the same sentence: the navigations, which take the place of the listing out instead (the next paragraph), escape, which touches no row, reload, which refuses itself the same way and past the wait retries the folder waited on (`ui/js/Reload.js` `begin`), and the view, rail, new-window and preview-column
keys, which change the window rather than a file. The rail, the crumbs and the tab strip are navigations
too, a row drag already could not start, and the list and the grid
now ask for no `window` while one is out, the rule
the columns view's active column already kept: a coalesce left running by a scroll just before the
request would otherwise fetch the new listing's rows at the old scroll offset, and the swapped view,
which starts at its top, would draw them as blank rows until the next fetch. The inert window is one
round trip for a local folder and the cap at most, and as long as a slow folder takes after that.
Swallowing was chosen over acting on what is visible because the backend can no longer resolve an old
index once it has the new listing, and a path-based rewrite of every row action is a larger change
than a window of refused keys; a rename editor already open keeps its keys, because a rename names its
file by path.

**The backend refuses rows read from a listing it has replaced, because the client cannot be the only
guard.** Before this it did not: `list A`, `list B`, `paths [0]` answered B's first file, measured on
this branch's own binary. Every change of what a row index names already runs `forget_rows`, a `list`,
a `listpaths`, a `search`, an accepted `sort` and a walk's ranking, so `forget_rows` now also moves
`State.generation`, and `write_window` stamps each `rows` line with it as `listing`. `ui/Backend.qml`
records the numbering of the rows the pane applied in `heldListing` (`ui/PaneSwap.qml takeRows`), and
`send()` names it on every request that carries `rows` and names none yet (`Swap.named`). **Rows read
before a later send keep the numbering they were read in**, because a listing can land in between: the
collision card stamps its question and the transfer waiting on it at the ask (`Collide.waiting`),
except a menu's, which the backend resolves from the menu's own capture and never by its rows, and a
row drag stamps its rows at the lift (`ui/FileDrag.qml dragListing`), so a watched re-read or a refresh
that lands before the choice or the drop gets that transfer refused rather than resolved in the new
numbering onto another file. The watched re-read also waits while a transfer waits on the card, so the
usual case keeps its rows. `src/backend/rowguard.rs` refuses a `trash`, `transfer`, `link`, `paths`, `collisions`, `settags`
or `menuaction` whose `listing` is not the one in force, before anything resolves:
an `error` line with `where` `stale` for each but `menuaction`, which `ui/PaneWire.qml onFailed` reports
without ending the listing that is out, and a failed `menuaction` reply for a snapshot, the shape the
menu already waits for. A request that names no numbering is resolved as before, which is every
older client, the picker and `tests/protocol.sh`'s own lines; `thumb`, `dirsize`, `meta` and `window`
only read and are not refused. This also closes the re-sort's window: between `s` and the reordered
rows the pane still draws the old order, and a key that reached the backend there is now refused.

**The newest navigation wins.** While a listing is out, the folder it asked for (`pane.listingPath`) is the
pane's current place for every navigation (`ui/js/Nav.js place`): a second open, Back, Forward, Up, a tab
switch, a Recent run or close, a rail row, a crumb, the folder jump and the path bar are each taken as the new
request, pass `supersede` to `openWithoutHistory`, and move `listingPath` (a new tab and a tab received from another window do too; a tab move lists nothing and leaves the listing to land; tearing a tab out still refuses); two quick Backs step back twice,
`open` pushes no place that never landed while a listing is out for another folder; when that listing is a Back or Forward whose entry put the drawn place on the stack `open` then clears (`pushedOnto` is `forwardHistory`), `open` pushes `BackReturn.drawn(pane)` onto history (`ui/js/Nav.js open`), and otherwise the drawn place was already pushed when that listing was asked for. Up climbs from the place asked for. The listing let go answers into nothing: its `listed`
line names another folder and its rows follow it (`Swap.onListed`, `Swap.onRows`). A re-list of the folder
drawn (sort, reload, the Esc relist) carries no `supersede` and keeps the refusal. On a stalled share the
backend does not make the newest list wait: a `list` that arrives while the gate holds a plain remote list scan
(not a search session's) replays the requests that waited, lets the scan go, and starts its own scan on a fresh
thread (`Gate::let_go`, `src/backend/run.rs`); the let-go scan's late answer is discarded and its watch is
released through `Event::AbandonWatch`, which the loop holds back while a newer scan is out and judges against the landed watch (`Gate::park_late`, inotify gives the same descriptor for the same folder). A tab switch's cursor, order and located row ride its landing (`pane.tabs.pending*`); a navigation that lets that listing go clears them (`BackReturn.dropOwed`).

**What the older guarantees rest on now.** The path is still written only from the answer, now in
`PaneSwap.applyListed`, so a refused hop never moves the breadcrumb. `listInFlight` is still set at the
request, so `menuSelectionIdentity` changes then and a menu raised over the old rows refuses its next
row, and the guard for a re-list of the folder drawn still runs first, before any hold. `Nav.forget` still clears the rename,
the filter, the thumbnail and dirsize maps and the selection before the new rows are set; a filter line
that has the caret when a hold starts is committed there, so a key typed over the held rows meets the
key gate rather than a query the swap would then forget. `thumbed` and `dirsized` lines are still
dropped while a listing is out. `listingPath` is still
recorded at the request, so `dropPath` sends a drop taken during a hold to the directory asked for.
Each pane of the dual view has its own `ui/PaneWire.qml` and so its own swap. The columns view's
preview column keeps the file it shows while rows are held: `ui/SelectionPreview.qml` no longer clears
the moment `listInFlight` turns it unreadable, and is cleared instead by the rows, the path and the
selection `Nav.forget` changes, which is at the request when nothing is held and at the swap when rows
are. It still asks for nothing while a listing is out.

**Who does not hold.** A tab switch into a tab with another view, and a hop into a folder that
keeps another view (`ui/js/Nav.js` compares the view before and after `restoreView`), pass `clearAtOnce`,
because these rows were never listed in that view and would be built in it only to be built again. A
pane still loading has nothing to hold. A walk's exit holds the whole search state, strip and rows,
until the landing clears both in one turn: holding result rows with the strip already gone would draw
half of each, and forgetting them at the request drew a blank frame, so neither happens alone
(`Search.leaveWalk` defers the drawn close while the backend closes at the request, and `Swap.begin`
holds after a walk for the exit's ask). A walk that stood on folder rows already, an emptied query,
closes at once with nothing to keep drawn. Refresh after a write, the watched re-read, the settings
re-list and the hidden toggle all hold: their rows are the same directory's, and the reset lands with
the rows that replace them. The watched re-read hands its filter query back as `keptQuery`, and
`Nav.forget` restores it when the hold ends; restoring it at the request, as `ui/js/Anchor.js` did,
would be undone by the swap's own reset.

**How it is proved.** `tests/js/swap.js` drives the decisions, the Nav route with a swap stub that
holds or does not, and the key gate through the real `Focus.handleKey` and `Focus.act` over a backend
stub: at rest `dd`, Delete, F2 and Return reach it, and in a fallen-back loading state and over held
rows none of them does. Removing the gate reddens six checks, `dd` sending `trash 0` among them, and
removing the hold reddens four. It also replays a sort's `listed` and `rows` ahead of the list's, a
search's opening line after its escape, a `listed` line kept across the cap, a failed listing and a
`stale` refusal, and a key typed while a filter line held the caret; `tests/js/collide.js` sends rows
the card read in numbering 4 after a `rows` line in 5 has landed, and they still name 4. It also builds
the real `ui/FileDrag.qml`, `ui/RowDrag.qml` and `ui/CollideHost.qml` from their own text over a stub
pane and backend, lifts a row in numbering 7, moves `heldListing` to 8 while the drag is up as a landed
re-list does, and drops it on a folder row: the question and the Replace transfer name 7, the transfer
is written before the card clears `pending`, and `Anchor.busy` holds the watched re-read until then.
`src/backend/rowguard.rs` carries the guard's unit tests and
`tests/protocol.sh` drives it through the binary: two lists answer numberings 1 and 2, `paths`,
`trash` and a menu snapshot naming 1 are refused and the file that trash names survives, 2 and an
unnamed request resolve, and after a sort 2 is refused too.
`tests/ui.sh noblank` reads `swapState`. `PaneSwap` counts, from the window's own `afterAnimating`,
every frame synchronised while a listing was out and the pane drawn in the loading state: `blankFrames`
before the cap, the defect, and `loadingFrames` after it. The case requires `blankFrames` not to move
across Enter, a double click and BackSpace into and out of a 300-file folder in the list, grid and
columns views; a listing its stub backend holds back 40 ms to be held that long and still swap; and one
held back 2.5 s to fall back at the cap and count loading frames, which is the control that shows the
counter sees frames drawn with no row. Each step is shot as it is issued and again once it has
landed, as `noblank-<step>-issued` and `noblank-<step>-landed`, and the fallback as
`noblank-slow-loading`.

**A landing that puts the cursor on a named row is answered with that row's window.** `pane.pendingSelect`
names the row a listing re-selects: the file the cursor stood on when a search began, the folder a climb
left, a created, pasted or renamed item, Show in folder and Show original. `ui/js/Nav.js
applyPendingSelect` looks for it only in the listing's first `rows` reply and forgets it on a miss, and
that reply covered rows `0..first`, so a search left with the cursor on row 667 of 668 landed on row 0
in the list, columns and grid views. `Nav.openWithoutHistory` now sends the armed path as the `list`
request's `select` (docs/protocol.md "list"), and `Listing::first_start` in `src/backend/listing.rs`
starts the first window at `index - first / 2`, clamped so it ends at the last row, when the row sorts
at or past `first`; for a row inside `0..first`, an absent name or none at all the reply is the one it
always was. The window still rides the `list` answer, so the rows, the cursor and the scroll land in
the one turn the swap already lands in, and the first frame the folder draws is the landing frame.
A `locate` inside the hold was the other way and was not taken: it costs a round trip for the index
and a second for its window, both inside the listing hold of `Swap.CAP_MS`, 66 ms, where the lookup costs one pass
over the name arena that stops at the row. `tests/search-exit-deep.sh` drives the real Pane over the real
backend on 668 rows and counts the folder frames drawn after Esc up to the cursor standing drawn on
its row inside the viewport: 1 for a row inside the first window, past it and on the last row, in all
three views, where the tree before this drew row 0 and never reached the row. `tests/protocol.sh` and
`listing.rs`'s own tests pin the window arithmetic. A rename whose row sorts far now draws its own
window with the cursor on it in the first frame, but the rename anchor still asks for its own window
after the list, so one or two frames can follow with the cursor on a row not yet held, as before; a
watched re-read names no row at all, so the corner below stands for both.

**// corner: a watched re-read can still move after a swap.** A watched re-read with the
cursor deep in a large folder swaps to the listing's first screenful and then jumps back when the
anchor's own window arrives. Columns keeps every answered drawn folder across a navigation when its
path, peek key and order match. `ui/js/ColumnsCarry.js` asks incoming ancestors beside the list and
seeds the folder left from held rows when their order and icons match a peek's. Kept ancestors are
re-read in place, and a seeded folder takes one watching peek when the flight ends. A changing
ancestor keeps its slot's old rows, inert, until its replacement answers; `ui/ColumnsLanding.qml`
bounds that hold with `Swap.CAP_MS` and shows the existing reading mark for `Swap.MIN_SHOW_MS` on
fallback. The third column keeps its shown folder's cache through the landing and its watch through
the replacement wait; a new folder waits under the existing folder cap, and a file preview uses
`thirdSwap`. The backend's `changed` signal continues to re-ask kept folders.

**A right click on the Locked tile opens the locked folder's own menu, never the parent's
background one.** A failed listing leaves no rows, so every background right click in all three views
took the `openBackground` path and drew the parent's rows, New Folder and Paste among them, over the
tile of a folder those rows cannot touch. `ui/js/Nav.js lockedTarget` names the folder the tile
draws, the directory asked for or the parent's own path on a re-read refusal, and `ui/ContextMenu.qml
openBackground` routes to `openLocked` while one is drawn, which is the one entrance all three views
share; the m key reaches it through `ui/Pane.qml openCursorMenu`. The rows are `ui/js/LockedMenu.js
lockedEntries`, Open in terminal, Permissions and Copy path in inventory order and nothing else, each
dispatched by path in `ui/Pane.qml performLocked` with no snapshot and no selection. Permissions reads
red where it cannot act, a folder owned by somebody else (its word is not yours) or a mode never read (no word), and
stays plain where its owner may still fix it, which is the way back in. `tests/js/menu.js` pins the
rows and the absent parent rows, `tests/js/nav.js` the target.

**A watched re-read waits out a held button.** The landing of any re-read of an unchanged listing runs `Nav.forget`, the count passes through 0 and every delegate is rebuilt in the list, the grid and the columns alike, so a press whose release came after the landing never tapped: the right press opened no menu and the left press moved no cursor, on any folder whose files change under it. `ui/PaneStates.qml` holds a passive `PointHandler` above the rows that writes `pane.pointerHeld` (a press a row grabs is still seen, and so is its release after the row is rebuilt). A press that lifts a file drag hands the release to the platform drag and leaves that handler active until the platform sends its own end-of-drag release (QtWayland does, in `QWaylandInputDevice::Pointer::releaseButtons`; the offscreen platform sends none, so the test sends one), so `ui/FileDrag.qml liftEnded` clears the hold, the one other writer of `pointerHeld`. `ui/js/Anchor.js busy` reads it, and `ui/PaneWire.qml reread` therefore stays owed until the button is up, then `onWatchBusyChanged` pays it after the 400 ms burst timer. A press that begins while a listing is out never reached a row (the `MouseArea` above takes it), so the one window is closed; `tests/presskeep.sh` holds a press 700 ms across a `changed` line raised through the backend signal (`pane.backend.changed`) in each view and each button, with a quiet control per case, and lifts a drag from the pressed row in each view, through the offscreen platform drag, to check the hold clears, the owed re-read is paid and the next press is held again. It moves one ceiling, re-derived with `wc -l`: `ui/Pane.qml` 1224 to 1226 for the `pointerHeld` property and its comment.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
