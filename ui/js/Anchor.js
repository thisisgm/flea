.pragma library
.import "Search.js" as Search

// Re-reading the open listing without moving the user off it. Three callers with one mechanism: a
// change another program made under the listing (ui/PaneWire.qml's watch), F5 and Flea's own delete.
// Split out of ui/js/Nav.js, which sits at the 300-line JS cap, the same way tests/js/watch.js was
// split out of tests/js/nav.js; ui/js/Nav.js keeps navigation and this keeps the return.

// ui/PaneWire.qml watchBusy: a re-read renumbers every row, so it waits while anything names a row by index or holds one open, the collision card's transfer too.
function busy(pane) {
    return !pane || pane.listInFlight || pane.renamingIndex >= 0 || pane.renamePending
        || pane.menuVisible || pane.menuActions.opened || pane.filterTyping || pane.searchMode.length > 0
        || pane.selectionCount() > 0 || pane.selectionBand !== null || pane.collide.pending !== null
}

// A change another program made under the open listing, unlike ui/js/Nav.js refresh() which follows
// Flea's own write. The rows are read again and the cursor is put back on the file it was on by name,
// because a create above it renumbers every row below and a listing that jumped back to the top
// would move the user while they were reading it. Returns the anchor apply() resolves, or null.
function watched(pane) {
    return anchoredRefresh(pane, false)
}

// F5, the re-read the operator asked for. ui/PaneWire.qml runs watched() only when busy() above lets
// it, and a standing selection holds it; pressing the key is the decision that wait was for, so the
// selection does not hold this one. It is also the only in-place re-read a network share gets, because a change on the
// server raises no inotify event here. The selection is cleared the way every new listing clears it,
// never re-pointed. Search results run their query again, since what they list is a walk.
function manual(pane) {
    if (pane.renameEditor() !== null) {
        pane.message("Finish or cancel the rename first.", false)
        return null
    }
    // Of what busy() waits for, the selection is the only hold that is the operator's to lift: a rename
    // or a collision card still waiting on its answer names a row by index, and would get another file.
    // Both hold ahead of search results too, because running the query again replaces those rows.
    if (pane.collide && pane.collide.pending !== null) {
        pane.message("Choose what to do about the existing files first.", false)
        return null
    }
    if (pane.renamePending) {
        pane.message("Rename is still finishing.", false)
        return null
    }
    if (pane.searchMode === Search.RESULTS) {
        if (pane.searchRunning)
            pane.message("The search is still running.", false)
        else
            Search.run(pane)
        return null
    }
    if (pane.listInFlight) {
        pane.message("A directory is already loading.", false)
        return null
    }
    return anchoredRefresh(pane, false)
}

// Flea's own delete. The rows that were marked are gone, so there is usually no name to return to:
// the anchor is the cursor row that was deleted and apply()'s own fallback then lands on
// whatever took its place, which is Finder's rule. It selects that row as well, so the next delete
// follows without reaching for the mouse; reported 2026-09-11, "deleting one refreshes the entire
// file list and loses my selection, so I have to start over". A delete that failed leaves the row
// standing, and then the name matches and the cursor goes back exactly where it was.
function afterDelete(pane, landed) {
    // A block leaves as a block, so the cursor belongs on the row the block left rather than on the
    // row below wherever it sat inside it; a delete that failed keeps the row it was already on.
    if (landed && pane.trashedFirst >= 0)
        pane.cursorIndex = pane.trashedFirst
    pane.trashedFirst = -1
    return anchoredRefresh(pane, true)
}

function anchoredRefresh(pane, select) {
    if (pane.listInFlight) {
        return null
    }
    var row = pane.rowFor(pane.cursorIndex)
    // The path rides along because the anchor can outlive one rows reply: a navigation between the
    // two below would otherwise put this directory's cursor row onto the next directory's listing.
    var anchor = { name: row ? String(row.n) : "", index: pane.cursorIndex, start: pane.held,
                   path: pane.path, select: select === true }
    // A filter narrows the rows the pane holds rather than choosing which directory it holds, so it
    // survives a re-read of the same directory; every other caller of openWithoutHistory drops it.
    pane.openWithoutHistory(pane.path, { keptQuery: pane.filterQuery })
    // The re-read answers from row 0, so a cursor deep in a large directory needs its own window back
    // before the anchor's name can be looked for anywhere near where it was.
    if (anchor.start > 0) {
        pane.backend.window(anchor.start, pane.windowSize)
    }
    return anchor
}

// Runs on each rows reply while an anchor stands. The name can arrive in the listing's own first
// window or in the one asked for above, so a miss in the first is not yet a miss. A name that is
// gone from both leaves the old index, which keeps the view where the user left it.
function apply(pane, anchor) {
    if (!anchor) {
        return null
    }
    if (pane.path !== anchor.path) {
        return null
    }
    for (var i = 0; i < pane.rows.length; i++) {
        if (String(pane.rows[i].n) === anchor.name) {
            landOn(pane, pane.held + i, anchor)
            return null
        }
    }
    // Still the first window rather than the one asked for above, so keep waiting, but only while that
    // window can still exist: a listing that shrank past the offset comes back clamped to row 0 instead.
    if (anchor.start > 0 && pane.held === 0 && pane.total > anchor.start) {
        return anchor
    }
    if (pane.total > 0) {
        landOn(pane, Math.min(anchor.index, pane.total - 1), anchor)
    }
    return null
}

// A watch's anchor only moves the cursor, because the operator's own selection belongs to them and a
// change another program made must not rewrite it. A delete's anchor selects, because the rows that
// were selected no longer exist and a cursor with nothing marked is a keyboard that has to start over.
function landOn(pane, index, anchor) {
    if (anchor.select)
        pane.selectOnly(index)
    else
        pane.setCursor(index)
}
