.pragma library

.import "DirSizes.js" as DirSizes
.import "FolderSorts.js" as FolderSorts
.import "RecentMode.js" as RecentMode
.import "Thumbs.js" as Thumbs

// What the header's click and the s and S keys do, taking ui/Pane.qml's root the way Nav.js and
// Ops.js do: the pane holds the state, this holds what the state does. ui/Backend.qml records the
// order the listing is actually in, because list re-sorts by name ascending and only this file
// changes it after that.

// The orders the backend can actually produce, in the order s steps through them, and the only keys
// that may move the recorded order. It is not the list of what gets refused: docs/protocol.md "sort"
// refuses every other key by name, and the backend is the one that says so, see ui/js/Errors.js, aliased from FolderSorts.ORDERS, the one key table.
var ORDERS = FolderSorts.ORDERS

// The three sort decisions, kept apart from what a pane does with them: the chooser makes the same ones over its narrower list.

// The sorted column reverses; any other starts ascending, the order the canvas header draws beside "Name".
function columnOrder(orders, by, desc, key) {
    if (orders.indexOf(key) < 0)
        return null
    return { key: key, desc: by === key ? !desc : false }
}

// Always ascending, column and direction separate; an outside order, a saved kind in the chooser, starts over at the first.
function nextOrder(orders, by) {
    return { key: orders[(orders.indexOf(by) + 1) % orders.length], desc: false }
}

// Whichever order the listing is in, offered or inherited: the user asked for the reverse of what they have.
function reverseOrder(by, desc) {
    return { key: by, desc: !desc }
}

// An edit open or a rename pending holds the sort and commits an open edit; it applies once the rename settles.
function holdForRename(pane, request) {
    if (!(pane.renamingIndex >= 0 || pane.renamePending)) return false
    pane.pendingSort = request
    if (pane.renamingIndex >= 0 && !pane.renamePending) pane.commitOpenRename()
    return true
}

// ui/Header.qml's click: only ORDERS leave this file except the forget row.
function column(pane, key) {
    // The flyout's last row (issue 179) forgets the folder and lists it on the default.
    // Recent stands on the root, so forgetting re-asks its history instead of listing the root over it.
    if (key === "__default__") {
        // Forgetting re-lists, so it holds like any other sort request.
        if (holdForRename(pane, { forget: true })) return
        if ((pane.recentMode || "").length > 0) {
            RecentMode.run(pane, pane.recentPaths || [])
            return
        }
        if (pane.backend && pane.backend.forgetFolderSort)
            pane.backend.forgetFolderSort(pane.path)
        // A preserved pane skips the reset a list would do, so refresh here or it keeps its old order.
        if (pane.backend && pane.backend.resetSort)
            pane.backend.resetSort(pane.path)
        pane.openWithoutHistory(pane.path)
        return
    }
    // Unsupported columns remain labels rather than sending a sort the backend must refuse.
    var order = columnOrder(ORDERS, pane.backend.sortBy, pane.backend.sortDesc, key)
    if (order)
        resort(pane, order.key, order.desc)
}

// s walks ORDERS so it never lands on a refusal-only column; an aimed click earns the reason, a walking key only noise.
function next(pane) {
    var order = nextOrder(ORDERS, pane.backend.sortBy)
    resort(pane, order.key, order.desc)
}

// S: the capital-is-the-variant pair g/G and j/J use.
function reverse(pane) {
    var order = reverseOrder(pane.backend.sortBy, pane.backend.sortDesc)
    resort(pane, order.key, order.desc)
}

// The request goes out for every key, so the refusal is the backend's alone. Only an order it will
// really produce moves the recorded one, or the mark would describe a listing that never changed.
function resort(pane, key, desc) {
    if (holdForRename(pane, { key: key, desc: desc })) return
    // Asking for the order the listing is already in would drop every row-indexed cache and put the
    // cursor back to redraw the rows already on screen, so it is not asked for at all.
    if (pane.backend.sortBy === key && pane.backend.sortDesc === desc) {
        return
    }
    pane.backend.sort(key, desc)
    if (ORDERS.indexOf(key) < 0) {
        return
    }
    pane.backend.sortBy = key
    pane.backend.sortDesc = desc
    // A walk's rows are matches, not the folder's, so sorting them writes nothing. Recent stands
    // on the root, so its order is never remembered as the root's own.
    if (pane.backend && pane.backend.rememberFolderSort
            && (pane.searchMode || "") === "" && (pane.recentMode || "") === "")
        pane.backend.rememberFolderSort(pane.path, key, desc)
    // A reorder moves every row, so the caches keyed by a row index are as stale as a new listing's,
    // and a selection of row indices would silently come to name different files.
    pane.thumbState = Thumbs.empty()
    pane.dirSizeState = DirSizes.empty()
    pane.clearSelection()
    pane.setCursor(0)
    // sort emits no rows of its own, so the reordered window is asked for here; see docs/protocol.md.
    pane.backend.window(0, pane.windowSize)
}

// The held sort a resort deferred; runs when the edit ends or the listing lands.
function applyPending(pane) {
    var held = pane.pendingSort
    if (!held) return
    // Not settled yet, or nowhere to send it: the hold stands.
    if (pane.listInFlight || pane.renamingIndex >= 0 || pane.renamePending) return
    if (!pane.backend || pane.backend.quitting || !pane.backend.running) { pane.pendingSort = null; return }
    pane.pendingSort = null
    if (held.forget) { column(pane, "__default__"); return }
    resort(pane, held.key, held.desc)
}

// Each flag is read the way its reader reads it, and state.sort stays as written because Pane.openWithoutHistory compares it with ViewState's own.
// Sample input: preferences({}) and preferences({hidden: false, foldersFirst: true, groupByKind: false, hiddenLast: false, rememberSort: true}) are the same string.
function preferences(state) {
    return JSON.stringify([state.hidden === true, state.sort, state.foldersFirst !== false,
                           state.groupByKind === true, state.hiddenLast === true, state.rememberSort !== false])
}

// A sorted column's title: the label and the geometric mark of the direction, characters the stock monospace has, so the mark scales with the font like the label.
function title(label, descending) {
    return label + " " + (descending ? "\u25be" : "\u25b4")
}
