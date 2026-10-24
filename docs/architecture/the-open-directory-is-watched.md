## The open directory is watched

Issue 68: a folder open in Flea did not follow a create, rename or delete made by another program.
`list` ran once when the pane arrived, and after that only Flea's own writes re-read the directory,
so a window kept open beside a terminal drew whatever was true when it was opened.

**The backend watches, the client re-reads.** `backend/watch.rs` holds one non-recursive inotify
watch on the directory being listed, armed by `list` BEFORE the scan and dropped by `search` and
`listpaths`, whose listings are not directories. Arming after the scan lost every change the scan
itself raced, a hole the width of one readdir: reproduced on the 100,000 file fixture, where a
create during the scan answered no `changed` line at all and the same create a second later
answered one. The new watch is armed BESIDE the current one rather than in place of it, so a scan
that fails costs the directory still on screen nothing: `commit` drops the old watch only once the
new listing replaced it. The scan runs on the list worker, so the arming does too:
`Watch::add_raw` before the scan, `set_incoming` plus `commit` on success, and `abandon_wd`
dropping the armed descriptor on failure; a worker that outlives its call hands its descriptor
back as `Event::AbandonWatch`, and the loop drops it unless a re-list aliased it onto the live watch.
Replacing it up front
was the first fix and it was wrong, because a failed list then handed the open directory a new
descriptor and `is_current` dropped anything still carrying the old one.
Its reader thread sends the loop an `Event::Changed(wd)`, and the loop answers
`{"t":"changed","path":"<the directory>"}` and changes nothing else: the rows, the count and
the sort order stay exactly what they were, because the client is the only thing that knows whether
it still wants them. See `docs/protocol.md` "changed" for the mask and the coalescing.

**The descriptor is why the payload is read at all.** `inotify_rm_watch` delivers an `IN_IGNORED` for
the watch it removed, so a reader that treated every wakeup as "something changed" answered one
`changed` line for every navigation, for the directory the pane had just arrived in. The reader walks
the burst for its watch descriptors and nothing else, and `Watch::is_current` drops any that is not
the one being listed now. `tests/protocol.sh`'s "a change in the directory just left answers nothing"
is that case.

**One burst is one re-read, and it is not restarted.** `ui/PaneWire.qml` starts a 400 ms timer on the
first `changed` after idle and lets later ones land inside it rather than restarting it: a restart
would mean a directory under continuous writing never settled and so never refreshed at all. Worst
case is one full re-scan per 400 ms while something is actively rewriting the folder the user is
looking at, which is 25 to 38 ms of that on the 100,000 file fixture.
**The test for this has to sample while the writing is still going**, which is what makes it a
test: measured after the writer stops, a restarted timer and an absorbing one both show a re-read
and the check passes either way. `tests/ui.sh watch` samples once inside a background writer's run
and requires the count to have moved; under `restart()` it does not move at all. No number is
quoted here on purpose, because each `omarchy-drive ipc` round trip costs 190 to 565 ms and the
sample therefore lands later than the sleep in front of it says.

**A re-read is a fresh `list`, so it renumbers every row, and that is what the anchor is for.**
`ui/js/Nav.js`'s `refreshWatched` records the NAME under the cursor before the re-read and
`applyAnchor` puts the cursor back on that name when the rows return, because a file created above
the cursor shifts every index below it and restoring by index alone would move the user onto another
file. The listing answers from row 0 unless the re-read names a `select` row at or past `first`
(`Listing::first_start` starts that window at the row), so a cursor deep in a large directory also asks for its own
window back and the anchor stands until that window arrives. The wait is on the window and not on a
number of replies, because `applyAnchor` runs on every rows delivery and a counter was spent by the
first screenful arriving twice; it ends when the listing has shrunk to that offset or below, the one
case where the backend clamps the window to row 0 and the reply being waited for is never coming.
A name that is gone from both falls back to the clamped old index, which keeps the view where the
user left it. The filter query is put
back too: it narrows the rows the pane holds rather than choosing which directory it holds.
A rename commit, by click-away or by Enter, anchors the same way (`Anchor.pointerRow` carries the viewport offset
too): the re-list's count passes through 0 and resets the view, and without the restore `setCursor`'s context-0 reveal
bottom-aligned the clicked row, 58 px up on a list that End had scrolled into its footer, which left the row below it
under the status bar. `AnchorHold.viewport` names the scrolling surface, the active column's list in the columns view.

**A column's directory is watched through a second inotify descriptor.** Issue 244 asked why a listing
does not follow another program, and the open folder always did; the neighbour columns did not, because a
`peek` reads a directory the pane is not on and nothing watched it. `src/backend/peekwatch.rs` holds the
watches a `peek` carrying `"watch":true` arms (only `ui/ColumnsArea.qml` sends it), on their own
descriptor so evicting one can never remove the listed folder's. Every watching peek names the directories the
columns draw in its `keep` list (`Columns.keepAsks`, state in `ColumnsArea.keep`) and the backend unwatches the
rest before it arms, so a column on screen never loses its
watch to one scrolled past; eight is only the guard behind that, and a peek answered after the client moved on
is unwatched at once. A column that returns after leaving the drawn set is re-asked (`again`), because its
cached rows went unwatched. The kernel dropping a watch (a deleted or unmounted directory, `IN_IGNORED`) sends
`PeekGone`, which stops holding it. `PeekWatch` owns its descriptor, an eventfd and the pump thread
(`src/backend/peekpump.rs`): `Drop` writes the eventfd, joins the pump, then closes both.
A fire answers the ordinary `changed` line for that directory, and `ColumnsArea.onChanged` re-asks it with
`ask(path, true)`, which keeps the rows drawn until the reply replaces them. `tests/watch-views.sh` drives the real window offscreen
through list, columns (middle, child and parent column, then again after entering the child and climbing back), grid and both dual panes with a create, a rename
and a delete from another process, and was red on the child and parent columns before this.

**The selection is re-anchored by file identity; the re-read no longer waits for it.** `ui/js/Selection.js` is a set
of row indices and its own rule is that a new listing clears them, because an index into a directory
that has changed names another file. Re-pointing a selection at other files is how a delete hits the
wrong ones, so the watched re-read carries the marks across by NAME instead (`ui/js/Anchor.js`
`selectedMarks`, recorded in `watched()` and restored in `apply()`): every mark stays on the same
file, a removed file's mark goes, the cursor stays on its file or on the clamped old index when its
file went, and the held window is asked for again with no scroll, so the viewport does not jump.
`PaneWire`'s `watchBusy`, decided by `ui/js/Anchor.js busy`, still defers the re-read while
a rename editor is open (the re-read would kill it), the context menu is up or a menu action waits
on its reply (the re-read flips `menuSelectionIdentity` and the reply is refused), a filter is
being typed (the re-read closes the query line), a search listing is showing (the walk owns the
rows outright), a rubber-band drag runs, a file drag runs or waits on its paths reply (the rows
must not shift under a live pointer), any paths asker resolves (clipboard, drag, compress, Copy as,
permissions or the anchor's own tagged ask, so two replies never cross), an
unfinished anchor for the open directory, a list is already in flight
or a transfer waits on the collision card (`ui/CollideHost.qml decide` writes its transfer before
it clears `pending`, so the transfer reaches the backend ahead of any re-read the card held back).
A bare selection, and the path-keyed copy and cut marks which never named a row index, are not a
reason to hold: another window's change shows at once with the same files marked. Marks outside the
held window resolve through one batched `paths` round trip before the swap and one batched `locate`
after it, each tagged to its asker (`ui/PaneWire.qml onPaths` routes only the anchor's tag to
`Anchor.fillPaths`, `Anchor.takeLocated` matches the directory, the anchor's own locate id and
`transferId` 0), and with no backend that can answer either the re-read lists at once and drops the marks it cannot resolve.
A preference re-list also re-marks by name: a
settings change to hidden, sort or grouping captures `Anchor.preference` before it re-lists, and
`ui/PaneSwap.qml` applies `Anchor.applyPreference` after the watched anchor. When any selected row
lies outside the held window, the preference re-list clears the whole selection rather than silently
keeping its in-window subset. Its anchor resolves only on the rows reply for the asked window; a
scroll elsewhere or a cursor move drops it. The debt is kept, not dropped: `onWatchBusyChanged`
starts the 400 ms timer the moment
the last of those clears. **The debt does not travel**: leaving the
directory clears it, because the pane's own `onPathChanged` fires before the navigation clears
whatever was holding it, and without that a change in the folder being left was paid for by a
full re-list of the folder being opened. `ui/Backend.qml`'s `listRequests` counter is what makes
that assertable, the same idiom as `thumbRequests` and `dirSizeRequests`: `tests/ui.sh watch` counts
from before the navigation and requires exactly one listing for it.

**A slow click ends with any key or action.** `ui/js/SlowClick.js` arms the pane's timer on a second tap after
the double-click interval, and its `fire` opens the rename editor. Every key that means an action
(`ui/js/Focus.js handleKey`, after the lookup; a bare modifier means none) and every action the pane
dispatches (`ui/Pane.qml act`, first line) calls `cancelSlowClick`, so Enter, Space, Delete or the menu key
pressed after the click gets its own result and never a rename editor behind it. The slow click's own rename
passes through `act` after `fire` cleared the record, so it is not cancelled by itself. A harness that needs a
row selected must not click one that is already the sole selection under the cursor, since that click is a
slow click (`tests/ui-menus.sh menus_file_menu` reads `dualState` first).

**The menu's Rename has three routes.** `Ops.menuRenameRoute` (called by `ui/PaneMenuActions.qml
openRenameFromMenu`) answers `now` for a ready snapshot over the unchanged `menuSelectionIdentity` (issue #170: the
editor opens at once and the replaced source is refused at commit, `backend.rename` with `renameMenuId`), `f2` for
a snapshot still in flight over the unchanged identity (the F2 route, a fresh snapshot of the cursor row), and
`stale` once the identity moved, which says "Selected items changed; reopen the menu." and opens nothing, as
`activate` does for every other action. The F2 key itself never asks. While the menu is open `Anchor.busy` holds
the watcher's re-read, so a source replaced under an open menu leaves the identity unchanged: the other five
actions are then refused by the backend ("Selected item changed; reopen the menu.") and Rename opens its editor.

**// corner: this is the local kernel's view of one directory.** A change another machine makes to an
NFS or SMB share raises no inotify event here, so a network mount is exactly as live as it was
before. So is a box whose inotify instance limit is exhausted, which says so once on stderr and then
never sends the line. `IN_MODIFY` is deliberately not in the mask: it fires on every `write(2)` and a
listing does not draw a partial size, so a row's size follows `IN_CLOSE_WRITE` instead.

**Flea's own writes pay for one extra scan.** A rename, mkdir, trash or transfer already calls
`pane.refresh()`, and the watch answers for the same write about 400 ms later, so the directory is
read twice. The second read is anchored the same way and moves nothing the user can see. Suppressing
it would mean deciding that a `changed` arriving during a listing belongs to that listing, which is
exactly the guess that would drop a real outside change, so it is paid rather than guessed at.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
