.pragma library

// The context menu's Open with slot: which listing row the applications answer belongs to, and what
// it answered. The state lives on ui/PaneWire.qml, which is the pane's own reply channel, and this
// library is pure over that wire, so tests/js/openwith.js can drive both without a window. The pane
// carries no open-with state of its own, which is why every read and write below goes through wire.

// Asked by a menu opening on a row, through ui/PaneWire.qml, and never by a cursor move: nothing
// pays for work the operator did not ask for. A directory row, a loading listing and an empty one
// all clear the slot instead of asking, because no row menu can open on them.
function ask(wire) {
    var pane = wire.pane
    var row = pane.cursorRow
    if (!row || row.d) {
        wire.openWithRow = -1
        wire.openWithApps = []
        return
    }
    // One slot, so the row it already holds is either the pending ask or the cached answer; both
    // mean a second menu over the same row costs the wire nothing. A stuck pending state cannot
    // outlive a listing: onListed clears the slot, so the dropped answer of a superseded ask is
    // followed by a fresh one.
    if (wire.openWithRow === pane.cursorIndex)
        return
    wire.openWithRow = pane.cursorIndex
    wire.openWithApps = []
    pane.backend.askHandlers(pane.cursorIndex)
}

// The reply lands here. A late answer for a row the menu has since left is dropped, the same rule
// onThumbed applies to a superseded listing, because one slot holds one row's answer and an inline
// directory answer can land beside an in-flight file answer in either order.
function answered(wire, row, apps) {
    if (wire.pane.listInFlight || row !== wire.openWithRow)
        return
    wire.openWithApps = apps
}
