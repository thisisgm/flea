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
    // The guard runs before the pop and not only inside openWithoutHistory: that call refuses the
    // listing while one is loading, and the entry was already gone by then, so a back taken during
    // a listing threw the place away and went nowhere. parent() below guards the same way.
    if (pane.listInFlight) {
        pane.message("A directory is already loading.", false)
        return
    }
    var target = pane.history[pane.history.length - 1]
    // The pop happens before the open, because open() is what would otherwise push it straight back on.
    pane.history = pane.history.slice(0, pane.history.length - 1)
    pane.openWithoutHistory(target)
}

// Issue 20: the mouse's back button. Nautilus and Explorer bind it to history, so it is the chrome's
// own left arrow wherever there is somewhere to go back to and the up arrow where there is not, which
// is the climb the issue asked for. No forward stack, because the canvas draws one arrow and not two.
function mouseBack(pane) {
    if (pane.history.length > 0) {
        back(pane)
        return
    }
    parent(pane)
}

// Everything a fresh listing has to forget. Called by open, by refresh and by the hidden toggle, so
// the reset is written once and no caller can half-do it.
function openWithoutHistory(pane, newPath) {
    if (pane.listInFlight) {
        pane.message("A directory is already loading.", false)
        return
    }
    pane.listInFlight = true
    pane.listedSeen = false
    pane.path = newPath
    pane.total = 0
    pane.held = 0
    pane.rows = []
    pane.kindNames = []
    pane.thumbState = Thumbs.empty()
    pane.dirSizeState = DirSizes.empty()
    pane.cursorIndex = 0
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
// operation produced re-selects that row through pendingSelect instead of dropping the cursor to the
// top. It is not a navigation, so it never touches the history.
function refresh(pane, selectPath) {
    pane.pendingSelect = selectPath ? selectPath : ""
    pane.openWithoutHistory(pane.path)
}

// Only the first rows response looks for the target, then it is forgotten either way, so a later
// directory change never re-reveals it. The target is a full path, which is what --select carries.
function applyPendingSelect(pane) {
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

// Issue 45: the chrome's path as the pieces a click can land on. text is what is drawn, including
// the separator that follows it, so the pieces concatenate to exactly the one line they replace;
// path is the directory the piece names, which is what ui/ChromeBar.qml hands to pathEntered. The
// home test is ui/js/Search.js scopeRoot's and not Format.tilde's, because Format.tilde writes
// /home/gmx as "~x" and a crumb built on that would carry a click to /home/gm, another directory.
function crumbs(path, home) {
    var text = String(path)
    var base = String(home)
    var inHome = base.length > 0 && (text === base || text.indexOf(base + "/") === 0)
    var display = inHome ? "~" + text.substring(base.length) : text
    var parts = display.split("/")
    var walked = inHome ? base : ""
    // The leading "~" and the leading "/" are each a crumb of their own: one names home and the
    // other names the root, and neither is a component the split hands back.
    var out = [{ text: parts.length > 1 ? parts[0] + "/" : parts[0],
                 path: walked.length > 0 ? walked : "/", last: false }]
    for (var i = 1; i < parts.length; i++) {
        if (parts[i].length === 0) {
            continue
        }
        walked = walked + "/" + parts[i]
        out.push({ text: parts[i] + "/", path: walked, last: false })
    }
    // Only a crumb with another after it carries a separator, so the last one gives its own back.
    var end = out[out.length - 1]
    if (out.length > 1) {
        end.text = end.text.substring(0, end.text.length - 1)
    }
    end.last = true
    return out
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
