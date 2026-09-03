.pragma library

.import "Filter.js" as Filter
.import "Format.js" as Format

// Directory tabs in one window. The pane and the backend still hold one listing: a hidden tab is a
// snapshot, not a second view, because a hidden view is not a free view. t, w and 1-9 are already
// in keys.toml; this file is what they do. Nine is the cap because those digits are the jump keys.

var MAX = 9

// Where a tab snapshotted now should reopen: a search sets pane.path to the scope it walks, so the
// directory the user was in is searchFrom, and every caller reads this BEFORE dropOverlay clears it.
function restingPath(pane) {
    if (pane.searchMode === "results" && pane.searchFrom.length > 0)
        return pane.searchFrom
    return pane.path
}

function snapshot(pane, path) {
    var where = path === undefined ? restingPath(pane) : path
    // A cursor and a selection read off a search's own listing name nothing in the directory the
    // tab records, so a tab stepping back out of a search starts at its first row with none.
    var elsewhere = where !== pane.path
    return {
        path: where,
        history: pane.history.slice(),
        cursorIndex: elsewhere ? 0 : pane.cursorIndex,
        viewMode: pane.viewMode,
        showHidden: pane.showHidden,
        selected: elsewhere ? [] : pane.selectedIndices().slice(),
        sortBy: pane.backend.sortBy,
        sortDesc: pane.backend.sortDesc
    }
}

function pack(items, index) {
    return {
        items: items,
        index: index,
        pendingCursor: -1,
        pendingSortBy: "",
        pendingSortDesc: false
    }
}

function label(path, home) {
    if (!path || path === "/")
        return "/"
    if (home && path === home)
        return "Home"
    var leaf = Format.leafPart(Format.tilde(path, home || ""))
    return leaf.length > 0 ? leaf : path
}

// The current tab's path lives on the pane, not in the snapshot, so a navigate does not wait for a switch.
function labelAt(pane, i) {
    return label(pathAt(pane.tabs, currentIndex(pane), i, pane.path), pane.home)
}

// The path tab i draws: the live pane path for the current tab, the snapshot's for a hidden one.
// Takes the values rather than the pane so ui/TabBar.qml's binding can read each one by name.
function pathAt(tabs, index, i, currentPath) {
    if (i === index)
        return currentPath
    return tabs && tabs.items && tabs.items[i] ? tabs.items[i].path : ""
}

function labels(pane) {
    var n = count(pane)
    var out = []
    for (var i = 0; i < n; i++)
        out.push(labelAt(pane, i))
    return out
}

function count(pane) {
    return pane.tabs && pane.tabs.items && pane.tabs.items.length > 0 ? pane.tabs.items.length : 1
}

function currentIndex(pane) {
    return pane.tabs ? pane.tabs.index : 0
}

function dropOverlay(pane) {
    if (pane.searchMode.length > 0) {
        if (pane.searchRunning)
            pane.backend.searchcancel()
        pane.searchMode = ""
        pane.searchQuery = ""
        pane.searchRunning = false
        pane.searchCancelled = false
        pane.searchFrom = ""
        pane.searchScanned = 0
    }
    Filter.close(pane)
}

function closePreview(pane) {
    if (pane.preview && pane.preview.active)
        pane.preview.close()
}

function busy(pane) {
    if (pane.listInFlight) {
        pane.message("A directory is already loading.", false)
        return true
    }
    return false
}

// Only ever called for a switch that re-listed nothing; the clamp is the belt on top of that, since
// an index past the end would select a row that is not there at all.
function restoreSelection(pane, selected) {
    pane.clearSelection()
    if (!selected || selected.length === 0)
        return
    var kept = 0
    for (var i = 0; i < selected.length; i++) {
        if (selected[i] < pane.total) {
            pane.selection.toggle(selected[i])
            kept++
        }
    }
    if (kept > 0)
        pane.selectionVersion++
}

function apply(pane, item) {
    var same = pane.path === item.path && pane.showHidden === item.showHidden
    pane.history = item.history.slice()
    pane.viewMode = item.viewMode
    pane.showHidden = item.showHidden
    if (same) {
        if (pane.backend && (pane.backend.sortBy !== item.sortBy || pane.backend.sortDesc !== item.sortDesc)) {
            pane.backend.sort(item.sortBy, item.sortDesc)
            pane.backend.sortBy = item.sortBy
            pane.backend.sortDesc = item.sortDesc
            pane.backend.window(0, pane.windowSize)
            pane.tabs.pendingCursor = item.cursorIndex
            return
        }
        // The one switch that re-reads nothing, so the rows behind these indices are the rows the
        // selection was made on and restoring it is safe. Every other path below drops it.
        pane.setCursor(item.cursorIndex)
        restoreSelection(pane, item.selected)
        return
    }
    pane.tabs.pendingCursor = item.cursorIndex
    pane.tabs.pendingSortBy = item.sortBy
    pane.tabs.pendingSortDesc = item.sortDesc
    pane.openWithoutHistory(item.path)
}

function applyPending(pane) {
    if (!pane.tabs)
        return
    var t = pane.tabs
    if (t.pendingSortBy && t.pendingSortBy.length > 0
            && pane.backend
            && (pane.backend.sortBy !== t.pendingSortBy || pane.backend.sortDesc !== t.pendingSortDesc)) {
        var by = t.pendingSortBy
        var desc = t.pendingSortDesc
        t.pendingSortBy = ""
        pane.backend.sort(by, desc)
        pane.backend.sortBy = by
        pane.backend.sortDesc = desc
        pane.backend.window(0, pane.windowSize)
        return
    }
    if (t.pendingCursor >= 0) {
        var last = pane.total > 0 ? pane.total - 1 : 0
        pane.setCursor(Math.min(t.pendingCursor, last))
        t.pendingCursor = -1
    }
}

function currentItems(pane, here) {
    if (pane.tabs && pane.tabs.items && pane.tabs.items.length > 0)
        return pane.tabs.items.slice()
    return [snapshot(pane, here)]
}

function openNew(pane) {
    if (busy(pane))
        return
    // Before the preview and the search go: a refused tenth tab must cost the user nothing.
    if (count(pane) >= MAX) {
        pane.message("Nine tabs is the most.", false)
        return
    }
    var here = restingPath(pane)
    closePreview(pane)
    dropOverlay(pane)
    var items = currentItems(pane, here)
    var index = currentIndex(pane)
    items[index] = snapshot(pane, here)
    items.push(snapshot(pane, here))
    pane.tabs = pack(items, items.length - 1)
    // dropOverlay clears the search but leaves the pane on the scope it walked, so the new tab has
    // to land on the path it just recorded; this is what Escape out of a search already does.
    if (pane.path !== here)
        pane.openWithoutHistory(here)
}

function selectAt(pane, i) {
    if (busy(pane))
        return
    var here = restingPath(pane)
    var items = currentItems(pane, here)
    if (i < 0 || i >= items.length) {
        pane.message("No tab " + (i + 1) + ".", false)
        return
    }
    var index = currentIndex(pane)
    if (i === index)
        return
    closePreview(pane)
    dropOverlay(pane)
    items[index] = snapshot(pane, here)
    pane.tabs = pack(items, i)
    apply(pane, items[i])
}

function closeAt(pane, i) {
    if (busy(pane))
        return
    var items = currentItems(pane)
    if (items.length <= 1) {
        pane.message("Can't close the last tab.", false)
        return
    }
    if (i < 0 || i >= items.length) {
        pane.message("No tab " + (i + 1) + ".", false)
        return
    }
    var index = currentIndex(pane)
    items.splice(i, 1)
    var next = index
    if (i < index)
        next = index - 1
    else if (i === index)
        next = Math.min(i, items.length - 1)
    if (i === index) {
        closePreview(pane)
        dropOverlay(pane)
        pane.tabs = pack(items, next)
        apply(pane, items[next])
    } else {
        pane.tabs = pack(items, next)
    }
}

function act(action, pane) {
    if (action === "tabNew") {
        openNew(pane)
        return
    }
    if (action === "tabClose") {
        closeAt(pane, currentIndex(pane))
        return
    }
    if (action.length === 4 && action.indexOf("tab") === 0 && action.charAt(3) >= "1" && action.charAt(3) <= "9")
        selectAt(pane, parseInt(action.charAt(3), 10) - 1)
}
