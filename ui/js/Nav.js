.pragma library

.import "DirSizes.js" as DirSizes
.import "Filter.js" as Filter
.import "Kinds.js" as Kinds
.import "Thumbs.js" as Thumbs
.import "Search.js" as Search
.import "ColumnMenu.js" as ColumnMenu

// Where the pane has been and how it gets back, taking ui/Pane.qml's root the way Search.js and
// Ops.js do: the pane holds the state, this holds what the state does.

// A new destination discards the forward branch; refreshing the same directory preserves it.
function open(pane, newPath) {
    // The guard runs before the push for the same reason back()'s runs before the pop: the listing
    // is refused while one is loading, and by then the entry pushed was a duplicate of the directory
    // the pane never left, which the next back press then went "back" to.
    if (pane.listInFlight) {
        pane.message("A directory is already loading.", false)
        return
    }
    if (pane.path.length > 0 && newPath !== pane.path) {
        pane.history = pane.history.concat([pane.path])
        pane.forwardHistory = []
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
    pane.forwardHistory = (pane.forwardHistory || []).concat([pane.path])
    // The pop happens before the open, because open() is what would otherwise push it straight back on.
    pane.history = pane.history.slice(0, pane.history.length - 1)
    pane.openWithoutHistory(target)
}

function forward(pane) {
    if (!pane.forwardHistory || pane.forwardHistory.length === 0) return
    if (pane.listInFlight) {
        pane.message("A directory is already loading.", false)
        return
    }
    var target = pane.forwardHistory[pane.forwardHistory.length - 1]
    pane.history = pane.history.concat([pane.path])
    pane.forwardHistory = pane.forwardHistory.slice(0, -1)
    pane.openWithoutHistory(target)
}

// The pane's own context menu covers the listing and no navigation closes it, so a press behind
// one left the menu standing over another directory's rows and its next row acted on whichever
// file had arrived at that index. ui/shell.qml refuses the window's overlays; the collision card is the pane's.
function mouseRefused(pane) {
    return pane.menuVisible || pane.collide.opened
}

// The mouse back button follows history, or climbs when no history exists.
function mouseBack(pane) {
    if (mouseRefused(pane)) {
        return
    }
    if (pane.history.length > 0) {
        back(pane)
        return
    }
    parent(pane)
}

// The mouse forward button retraces a back, behind the same refusals; with nothing ahead it does nothing.
function mouseForward(pane) {
    if (!mouseRefused(pane)) forward(pane)
}

// Every listing the pane asks for; of options, ui/Pane.qml reads keepHidden and ui/js/Swap.js begin() the rest.
// A walk owns the rows until this lands, so leaving it is the shared step with the tabs:
// tab switches, Back, Up, rail clicks and jumps all funnel through here.
function openWithoutHistory(pane, newPath, options) {
    if (pane.listInFlight) {
        pane.message("A directory is already loading.", false)
        return
    }
    // A hop elsewhere drops a waiting background menu, while the armed hop keeps its own request.
    if (pane.pendingBackground && pane.pendingBackground.length > 0 && !ColumnMenu.samePath(newPath, pane.pendingBackground))
        clearPendingBackground(pane)
    Search.leaveWalk(pane)
    pane.listInFlight = true
    pane.listedSeen = false
    // The path is not written here. A refused listing never answers a listed line, so leaving the
    // pane's own path alone is what keeps a refused hop from moving the breadcrumb onto a directory
    // nobody could read; ui/PaneSwap.qml applyListed takes it from the answer instead. The directory
    // asked for is recorded, because a drop landing while the reply is out means that one and not
    // the directory being left; ui/Pane.qml dropPath reads it and only while this listing is out.
    pane.listingPath = newPath
    // The class below is the directory being left until fsinfo answers for this one, and a
    // settle firing in between would spend it; a re-read of the same path keeps its class.
    if (newPath !== pane.path) { pane.storageClass = ""; pane.storageKnown = false }
    var ask = options || {}
    // A settled listing stays drawn until the new rows land, see AGENTS.md "The listing swap".
    if (!pane.swap.hold(ask))
        forget(pane, ask.keptQuery)
    // A filter line held with its rows gives up the caret, so keys typed meanwhile meet the gate, not a query the swap forgets.
    else if (pane.filterTyping)
        Filter.commit(pane)
    pane.appliedListingPreferences = pane.listingPreferences
    pane.backend.list(newPath, pane.windowSize, pane.showHidden)
    // One statfs per directory, not per row: the bar's right half only changes when the pane moves.
    pane.backend.askFsInfo()
}

// Everything a fresh listing forgets, written once: at the request, or by ui/PaneSwap.qml when held rows go.
function forget(pane, keptQuery) {
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
    // A filter narrows the rows already listed, so a new listing forgets it unless ui/js/Anchor.js hands it back.
    Filter.close(pane)
    if (keptQuery)
        pane.filterQuery = keptQuery
    pane.listingState = "loading"
    pane.stateMessage = ""
    pane.lockedMode = 0
    pane.clearSelection()
    pane.listArea.primeSettle()
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
    pane.pendingMenu = false
    clearPendingBackground(pane)
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
            if (pane.pendingMenu) {
                pane.pendingMenu = false
                pane.openCursorMenu()
            }
            return
        }
    }
    // The row is not in this listing, so the intent behind it must not fire on some later match.
    pane.pendingMenu = false
}

// A failed listing drops the deferred background menu, so no later rows open it.
function clearPendingBackground(pane) { ColumnMenu.clearPendingBackground(pane) }

// The deferred neighbour background, consumed on every landing by identity.
function applyPendingBackground(pane) { ColumnMenu.applyPendingBackground(pane) }

// Enter on the cursor row: a directory navigates, an archive opens Flea's own view, anything else
// goes to the opener. The in-flight guard is what stops a second Enter queueing a second listing.
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
    if (!Filter.cursorShown(pane)) {
        pane.message("That row is hidden by the filter.", false)
        return
    }
    var path = pane.join(pane.path, row.n)
    if (row.d) {
        pane.open(path)
        return
    }
    // Handing an archive on opens another file manager, and this is ui/Preview.qml's own classifier.
    if (Kinds.quickLookKind(row.i, path) === Kinds.ARCHIVE) {
        pane.preview.open(path, row.i, row.s, pane.kindNames[row.k] || "")
        return
    }
    opener.open(path)
}

// The folder the Locked tile names, for its own menu and every row that menu offers: the
// directory asked for while its tile is up, which is the parent's own path when the folder
// failed on its own re-read. "" while no Locked tile is drawn, so the background menu stands.
function lockedTarget(pane) {
    if (pane.listingState !== "locked") return ""
    var asked = pane.listingPath || ""
    return asked.length > 0 ? trimSlash(asked) : pane.path
}

// The path helpers the columns view needs. A root has no parent and no leaf of its own.
function trimSlash(path) {
    // A bookmark can carry one trailing slash, which no row name ever has; "/" alone keeps its own.
    var text = String(path)
    return text.length > 1 && text.charAt(text.length - 1) === "/" ? text.substring(0, text.length - 1) : text
}

function parentOf(path) {
    var text = trimSlash(path)
    var cut = text.lastIndexOf("/")
    return cut <= 0 ? "/" : text.substring(0, cut)
}

function leafOf(path) {
    var text = String(path)
    var cut = text.lastIndexOf("/")
    return cut < 0 || cut === text.length - 1 ? text : text.substring(cut + 1)
}

// Backspace, h, and the chrome's up arrow. The root has no parent, so it is where climbing stops.
// pendingSelect is the directory being left, so the parent listing puts the cursor on it rather than
// on its first row; applyPendingSelect reads the window the listing answered with, so a child
// sorted past that first screenful is not found and the cursor stays where a climb always left it.
function parent(pane) {
    if (pane.listInFlight) {
        pane.message("A directory is already loading.", false)
        return
    }
    // Issue 193: a refused hop climbs from the folder it asked for, trailing slash trimmed, selecting the refused row.
    if (pane.listingState === "locked" && pane.listingPath !== pane.path) {
        pane.pendingSelect = trimSlash(pane.listingPath)
        pane.open(parentOf(pane.listingPath))
        return
    }
    if (pane.path === "/") {
        return
    }
    var here = pane.path
    var cut = here.lastIndexOf("/")
    pane.pendingSelect = here
    pane.open(cut <= 0 ? "/" : here.substring(0, cut))
}

// A click on the folder already shown is a no-op, so the rail and columns routes land here.
function showing(pane, path) {
    if (trimSlash(path) !== pane.path) return false
    if (pane.listInFlight) return false
    if ((pane.searchMode || "").length > 0) return false
    if (!pane.trash || pane.trash.opened) return false
    return pane.listingState === "ready" || pane.listingState === "empty"
}

// Same-path Enter re-lists only a failed or locked listing; Trash, search and in-flight stay a no-op.
function pathFailed(pane) {
    if (!pane || pane.listInFlight) return false
    if ((pane.searchMode || "").length > 0) return false
    if (!pane.trash || pane.trash.opened) return false
    return pane.listingState === "error" || pane.listingState === "locked"
}

// The rail's and the columns view's route in: the shown folder returns false and lists nothing.
function openPlace(pane, path) {
    if (showing(pane, path)) return false
    pane.open(path)
    return true
}
