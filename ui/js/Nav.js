.pragma library

.import "DirSizes.js" as DirSizes
.import "Filter.js" as Filter
.import "Kinds.js" as Kinds
.import "Thumbs.js" as Thumbs
.import "Search.js" as Search
.import "ColumnMenu.js" as ColumnMenu
.import "BackReturn.js" as BackReturn
.import "Anchor.js" as Anchor
.import "MouseNav.js" as MouseNav
.import "Swap.js" as Swap

// Where the pane has been and how it gets back, taking ui/Pane.qml's root the way Search.js and
// Ops.js do: the pane holds the state, this holds what the state does.

// A listing inside its deadline refuses a row action; a waiting one is let go by the navigation itself, see ui/Pane.qml openWithoutHistory.
function loading(pane) { return pane.listInFlight && pane.listingState !== "waiting" }

// While a listing is out, the folder it asked for is the pane's current place for every navigation, and the drawn path only until its answer lands.
function place(pane) { return pane.listInFlight && !!pane.listingPath ? pane.listingPath : pane.path }

// A new destination discards the forward branch and pushes the drawn place unless the listing out already did; a re-read keeps both.
function open(pane, newPath) {
    if (place(pane).length > 0 && newPath !== place(pane)) {
        if (!BackReturn.pending(pane))
            pane.history = pane.history.concat([BackReturn.capture(pane)])
        // A Back out pushed the drawn place onto the forward branch cleared below, so history takes it instead.
        else if (BackReturn.pushedOnto(pane) === "forwardHistory")
            pane.history = pane.history.concat([BackReturn.drawn(pane)])
        pane.forwardHistory = []
    }
    pane.openWithoutHistory(newPath, { supersede: true })
}

function back(pane) {
    if (pane.history.length === 0) {
        return
    }
    BackReturn.travel(pane, "history", "forwardHistory")
}

function forward(pane) {
    if (!pane.forwardHistory || pane.forwardHistory.length === 0) return
    BackReturn.travel(pane, "forwardHistory", "history")
}

// The mouse back button follows history, or climbs when no history exists.
function mouseBack(pane) {
    if (MouseNav.refused(pane)) {
        return
    }
    if (pane.history.length > 0) {
        back(pane)
        return
    }
    parent(pane)
}

// Every listing the pane asks for; of options, ui/Pane.qml reads keepHidden and ui/js/Swap.js begin() the rest.
// A walk owns the rows until this lands, so leaving it is the shared step with the tabs:
// tab switches, Back, Up, rail clicks and jumps all funnel through here.
function openWithoutHistory(pane, newPath, options) {
    Anchor.clearWaiting(pane)
    var ask = options || {}
    if (pane.listInFlight) {
        // Only a navigation takes the place of a listing out; a re-list of the folder shown must not undo it.
        if (ask.supersede !== true) {
            pane.message("A directory is already loading.", false)
            return
        }
        // The listing let go answers into nothing: its listed line names another folder and its rows follow it, see ui/js/Swap.js onListed.
        pane.swap.stopWaiting()
        BackReturn.dropOwed(pane)
    }
    // A hop elsewhere drops a waiting background menu, while the armed hop keeps its own request.
    if (pane.pendingBackground && pane.pendingBackground.length > 0 && !ColumnMenu.samePath(newPath, pane.pendingBackground))
        clearPendingBackground(pane)
    Search.leaveWalk(pane)
    pane.listInFlight = true
    pane.listedSeen = false
    // A hop into another view clears at once like a tab switch, so held rows never draw in a view they were not listed in.
    var shown = pane.viewMode
    if (pane.restoreView)
        pane.restoreView(newPath)
    // A manual reload's notice belongs to its own re-read: a navigation that lands first spends it.
    pane.reloadFrom = -1
    // The path is not written here. A refused listing never answers a listed line, so leaving the
    // pane's own path alone is what keeps a refused hop from moving the breadcrumb onto a directory
    // nobody could read; ui/PaneSwap.qml applyListed takes it from the answer instead. The directory
    // asked for is recorded, because a drop landing while the reply is out means that one and not
    // the directory being left; ui/Pane.qml dropPath reads it and only while this listing is out.
    if (newPath !== pane.path) pane.linkTargetPendingId = 0 // A waiting Show original belongs to the listing being left, so leaving it drops the reveal.
    pane.listingPath = newPath
    // Verdicts stay the left directory's until fsinfo answers, so re-read keeps them.
    if (newPath !== pane.path) { pane.storageClass = ""; pane.storageKnown = false; pane.fsAnswered = false; pane.systemVolume = false; pane.canTag = false }
    if (pane.viewMode !== shown) ask.clearAtOnce = true
    // A walk closed above keeps its rows and its strip until this listing lands.
    if (pane.searchLeaving === true) ask.holdWalk = true
    // A filter line held with its rows gives up the caret, so keys typed meanwhile meet the gate, not a query the swap forgets.
    if (holdOrForget(pane, ask, ask.keptQuery) && pane.filterTyping)
        Filter.commit(pane)
    pane.appliedListingPreferences = pane.listingPreferences
    // leaveWalk above has closed or is closing any search, so the armed row is this listing's to find; a path in another folder names none.
    pane.backend.list(newPath, pane.windowSize, pane.showHidden, ask.wantChanged === true, pane.pendingSelect)
    // One statfs per directory, not per row: the bar's right half only changes when the pane moves.
    pane.backend.askFsInfo()
}

// A settled listing stays drawn until the new rows land, see AGENTS.md "The listing swap"; true when held, else forgotten at once, and a fallen-back loading mark stays through the newest request.
function holdOrForget(pane, ask, keptQuery) {
    var fell = pane.swap.fellBack === true
    var held = pane.swap.hold(ask)
    if (!held) {
        forget(pane, keptQuery)
        if (fell) pane.swap.phase = Swap.expired(pane.swap.phase)
    }
    return held
}

// Everything a fresh listing forgets, written once: at the request, or by ui/PaneSwap.qml when held rows go.
function forget(pane, keptQuery) {
    // An exit landing clears the drawn search state in the same turn its rows do.
    if (pane.searchLeaving === true) Search.closeLeaving(pane)
    // The editor row and the held sort belong to the replaced listing, so both go with it, before the count drops.
    pane.pendingSort = null
    pane.renamingIndex = -1
    pane.total = 0
    pane.held = 0
    pane.rows = []
    pane.kindNames = []
    pane.thumbState = Thumbs.empty()
    pane.dirSizeState = DirSizes.empty()
    pane.cursorIndex = 0
    pane.trashArmedAt = 0
    // A re-list puts a different file at that index, so the tap record goes with the rename.
    if (pane.cancelSlowClick) pane.cancelSlowClick()
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
function refresh(pane, selectPath, keepSelection) {
    pane.pendingSelect = selectPath ? selectPath : ""
    pane.pendingMenu = false
    clearPendingBackground(pane)
    // A search keeps its rows and numbering, so one window ask re-stats the held rows; marks and cursor stay.
    if (keepSelection === true && pane.searchMode === Search.RESULTS) {
        pane.backend.window(pane.held, pane.windowSize)
        return
    }
    if (keepSelection === true) {
        // Metadata-only writes use the watcher's identity anchor and interaction debt.
        pane.wire.stale = true
        pane.wire.reread()
        return
    }
    pane.openWithoutHistory(pane.path, { inPlace: true })
}

// The list request names the target, so its first rows answer alone looks for it at any depth, by listed name (NFC finds NFD).
function applyPendingSelect(pane) {
    // A search arms its return while the query stands, so its own landings skip it until Esc.
    if (pane.pendingSelect.length === 0 || (pane.searchMode || "").length > 0) {
        return
    }
    var target = pane.pendingSelect
    pane.pendingSelect = ""
    var at = Anchor.selectMatch(pane.rows, target, pane.path)
    if (at >= 0) {
        var index = pane.held + at
        pane.setCursor(index)
        pane.selection.only(index, true)
        pane.selectionAnchor = index
        pane.selectionVersion++
        if (pane.pendingMenu) {
            pane.pendingMenu = false
            pane.openCursorMenu()
        }
        return
    }
    // The row is not in this listing, so the intent behind it must not fire on some later match.
    pane.pendingMenu = false
}

// A failed listing drops the deferred background menu, so no later rows open it.
function clearPendingBackground(pane) { ColumnMenu.clearPendingBackground(pane) }

// The deferred neighbour background, consumed on every landing by identity.
function applyPendingBackground(pane) { ColumnMenu.applyPendingBackground(pane) }

// The cursor row once Enter's three guards pass, else null with the reason shown; the in-flight guard stops a second Enter queueing a second listing.
function openableRow(pane) {
    if (loading(pane)) {
        pane.message("A directory is already loading.", false)
        return null
    }
    var row = pane.rowFor(pane.cursorIndex)
    if (!row) {
        pane.message("That row has not loaded yet.", false)
        return null
    }
    if (!Filter.cursorShown(pane)) {
        pane.message("That row is hidden by the filter.", false)
        return null
    }
    return row
}

// Enter on the cursor row or the selection: a directory navigates, an archive opens Flea's own view,
// anything else goes to the opener. The in-flight guard stops a second Enter queueing a second listing.
function openCursor(pane, opener, paths) {
    // A menu or neighbour row hands the paths it already resolved.
    if (paths) {
        opener.open(paths)
        return
    }
    var indices = pane.selectedIndices ? pane.selectedIndices() : []
    // A selection of more than one opens together, and refuses the whole batch rather than part of it.
    if (indices.length > 1) {
        if (loading(pane)) { pane.message("A directory is already loading.", false); return }
        var base = Search.baseOf(pane)
        var targets = []
        for (var i = 0; i < indices.length; i++) {
            var row = pane.rowFor(indices[i])
            if (!row) { pane.message("Some selected files are outside the loaded rows.", false); return }
            var path = pane.join(base, row.n)
            if (row.d) { pane.message("Directories cannot be opened alongside files.", false); return }
            if (Kinds.quickLookKind(row.i, path) === Kinds.ARCHIVE) { pane.message("Archives open on their own, one at a time.", false); return }
            targets.push(path)
        }
        opener.open(targets)
        return
    }
    // The cursor row, through the three guards above, unchanged.
    var row = openableRow(pane)
    if (!row) return
    var path = pane.join(Search.baseOf(pane), row.n)
    if (row.d) { pane.open(path); return }
    // Handing an archive on opens another file manager, and this is ui/Preview.qml's own classifier; an AppImage is a program first, so it asks on the run card.
    if (Kinds.quickLookKind(row.i, path) === Kinds.ARCHIVE && !/\.appimage$/i.test(row.n)) {
        pane.quickLook().open(path, row.i, row.s, pane.kindNames[row.k] || "",
            pane.thumbState && pane.thumbState.file ? Thumbs.fileFor(pane.thumbState, pane.cursorIndex) : "")
        return
    }
    opener.open(path)
}

// The run card's entrances always reach the opener, so Run asks through flea --open like Enter does.
function runCursor(pane, opener, verb) {
    var row = openableRow(pane)
    if (!row) return
    var path = pane.join(Search.baseOf(pane), row.n)
    if (verb === "open") opener.open(path)
    else pane.runText(path)
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

// Issue 193: a hop that is out, refused or waiting climbs from the folder it asked for; ui/Pane.qml canGoUp reads it too.
function climbsFromListing(pane) {
    return (pane.listInFlight || pane.listingState === "locked" || pane.listingState === "waiting") && !!pane.listingPath && pane.listingPath !== pane.path
}

// Backspace, h and the chrome's up arrow, stopping at the root; the parent lands on the directory left, at any depth.
function parent(pane) {
    if (climbsFromListing(pane)) {
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
