.pragma library

.import "DirSizes.js" as DirSizes
.import "Filter.js" as Filter
.import "Thumbs.js" as Thumbs

// Where the pane has been and how it gets back, taking ui/Pane.qml's root the way Search.js and
// Ops.js do: the pane holds the state, this holds what the state does.

// Every ordinary navigation remembers where it came from. Deliberately no forward stack: the canvas
// draws one arrow, not two.
function open(pane, newPath) {
    if (pane.path.length > 0 && newPath !== pane.path) {
        pane.history = pane.history.concat([pane.path])
    }
    pane.openWithoutHistory(newPath)
}

function back(pane) {
    if (pane.history.length === 0) {
        return
    }
    var target = pane.history[pane.history.length - 1]
    // The pop happens before the open, because open() is what would otherwise push it straight back on.
    pane.history = pane.history.slice(0, pane.history.length - 1)
    pane.openWithoutHistory(target)
}

// Everything a fresh listing has to forget. Called by open, by refresh and by the hidden toggle, so
// the reset is written once and no caller can half-do it. A refresh passes keep, which leaves the
// rows, their count and the cursor standing until the new ones land, so nothing paints an empty
// frame. Anything indexed by row is still forgotten: the rows about to arrive are numbered differently.
function openWithoutHistory(pane, newPath, keep) {
    if (pane.listInFlight) {
        pane.message("A directory is already loading.", false)
        return
    }
    pane.listInFlight = true
    pane.listedSeen = false
    pane.path = newPath
    if (!keep) {
        pane.total = 0
        pane.held = 0
        pane.rows = []
        pane.kindNames = []
        pane.cursorIndex = 0
        // A navigation is not a re-read, so a cursor a failed refresh left pending must not land here.
        pane.pendingCursor = -1
    }
    pane.thumbState = Thumbs.empty()
    pane.dirSizeState = DirSizes.empty()
    pane.trashArmedAt = 0
    // The row the editor sat on belongs to the listing being replaced, so the rename goes with it:
    // leaving the index set opened an empty editor over whatever file arrived at that row instead.
    pane.renamingIndex = -1
    // A filter narrows the rows already listed, so a new listing is exactly what forgets it.
    Filter.close(pane)
    pane.listingState = "loading"
    pane.stateMessage = ""
    pane.lockedMode = 0
    pane.clearSelection()
    pane.listArea.primeSettle()
    pane.backend.list(newPath, pane.windowSize, pane.showHidden)
    // One statfs per directory, not per row: the bar's right half only changes when the pane moves.
    pane.backend.askFsInfo()
}

// Which row the listing re-reveals after a rename. A rename the pointer committed keeps the row the
// pointer chose instead, because re-selecting the renamed one would undo the click a round trip
// after it landed. One shot: the next rename reveals again.
function renameRefreshTarget(pane, path) {
    if (!pane.renameKeepsPointerRow) {
        return path
    }
    pane.renameKeepsPointerRow = false
    return ""
}

// An operation changed the directory under the listing, so it is read again. Passing the path the
// operation produced re-selects that row through pendingSelect; passing nothing keeps the cursor on
// the index it held, which after a trash is the row that shifted up into the removed one's place,
// the row vim's dd lands on. It is not a navigation, so it never touches the history.
function refresh(pane, selectPath) {
    pane.pendingSelect = selectPath ? selectPath : ""
    pane.pendingCursor = selectPath ? -1 : pane.cursorIndex
    openWithoutHistory(pane, pane.path, true)
}

// Only the first rows response looks for the target, then it is forgotten either way, so a later
// directory change never re-reveals it. The target is a full path, which is what --select carries.
// A pending cursor is applied the same way, clamped to the listing that came back: trashing the last
// row leaves the cursor on the new last row rather than one past the end.
function applyPendingSelect(pane) {
    if (pane.pendingCursor >= 0) {
        var row = Math.min(pane.pendingCursor, Math.max(0, pane.total - 1))
        pane.pendingCursor = -1
        pane.setCursor(row)
    }
    if (pane.pendingSelect.length === 0) {
        return
    }
    var target = pane.pendingSelect
    pane.pendingSelect = ""
    for (var i = 0; i < pane.rows.length; i++) {
        if (pane.join(pane.path, pane.rows[i].n) === target) {
            var index = pane.held + i
            pane.setCursor(index)
            pane.selection.only(index)
            pane.selectionAnchor = index
            pane.selectionVersion++
            return
        }
    }
}

// Enter on the cursor row: a directory is navigated into, a file is handed to the opener. The
// in-flight guard is what stops a second Enter queueing a listing behind one already asked for.
function openCursor(pane, opener) {
    if (pane.listInFlight) {
        pane.message("A directory is already loading.", false)
        return
    }
    var row = pane.rowFor(pane.cursorIndex)
    if (!row) {
        pane.message("That row has not loaded yet.", false)
        return
    }
    var path = pane.join(pane.path, row.n)
    if (row.d) {
        pane.open(path)
        return
    }
    opener.open(path)
}

// The path helpers the columns view needs. A root has no parent and no leaf of its own.
function parentOf(path) {
    var cut = String(path).lastIndexOf("/")
    return cut <= 0 ? "/" : String(path).substring(0, cut)
}

function leafOf(path) {
    var text = String(path)
    var cut = text.lastIndexOf("/")
    return cut < 0 || cut === text.length - 1 ? text : text.substring(cut + 1)
}

// Backspace and the chrome's up arrow: the root has no parent, so it is where climbing stops.
function parent(pane) {
    if (pane.listInFlight) {
        pane.message("A directory is already loading.", false)
        return
    }
    if (pane.path === "/") {
        return
    }
    var cut = pane.path.lastIndexOf("/")
    pane.open(cut <= 0 ? "/" : pane.path.substring(0, cut))
}
