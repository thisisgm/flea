.pragma library

.import "DirSizes.js" as DirSizes
.import "Thumbs.js" as Thumbs

// What the header's click and the s and S keys do, taking ui/Pane.qml's root the way Nav.js and
// Ops.js do: the pane holds the state, this holds what the state does. ui/Backend.qml records the
// order the listing is actually in, because list re-sorts by name ascending and only this file
// changes it after that.

// The orders the backend can actually produce, in the order s steps through them, and the only keys
// that may move the recorded order. It is not the list of what gets refused: docs/protocol.md "sort"
// refuses every other key by name, and the backend is the one that says so, see ui/js/Errors.js.
var ORDERS = ["name", "size", "mtime"]

// ui/Header.qml's click. The column already sorted reverses; any other column starts ascending,
// which is the order the canvas's own header draws beside "Name". Only ORDERS may leave this file.
function column(pane, key) {
    // Mode and Kind are labels. Asking the backend only to hear a refusal made them look sortable.
    if (ORDERS.indexOf(key) < 0)
        return
    resort(pane, key, pane.backend.sortBy === key ? !pane.backend.sortDesc : false)
}

// s: the next order the backend can produce, always ascending, because the column and the direction
// are separate choices. It walks ORDERS, so it never lands on a column that would only earn a
// refusal; an aimed click on one of those earns the reason, a key that walks onto it earns noise.
function next(pane) {
    var at = ORDERS.indexOf(pane.backend.sortBy)
    resort(pane, ORDERS[(at + 1) % ORDERS.length], false)
}

// S: reverse whichever order the listing is in, the capital-is-the-variant pair g/G and j/J use.
function reverse(pane) {
    resort(pane, pane.backend.sortBy, !pane.backend.sortDesc)
}

// The request goes out for every key, so the refusal is the backend's alone. Only an order it will
// really produce moves the recorded one, or the mark would describe a listing that never changed.
function resort(pane, key, desc) {
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
    // A reorder moves every row, so the caches keyed by a row index are as stale as a new listing's,
    // and a selection of row indices would silently come to name different files.
    pane.thumbState = Thumbs.empty()
    pane.dirSizeState = DirSizes.empty()
    pane.clearSelection()
    pane.setCursor(0)
    // sort emits no rows of its own, so the reordered window is asked for here; see docs/protocol.md.
    pane.backend.window(0, pane.windowSize)
}
