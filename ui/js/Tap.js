.pragma library

.import "Search.js" as Search

// The pointer contract, declared in keys.toml's [[pointer]] table and decided here and nowhere
// else. tests/js/tap.js drives every row of Keymap.POINTER through the three functions below, so a
// click cannot change meaning without the table saying so and the table cannot advertise a click the
// code does not make.
//
// The rule is macOS's and the operator's: one tap selects, the second opens. The single-tap action is
// idempotent on a row, so it runs on both taps of a double click rather than behind a double-click
// timer, which would delay every selection by the whole mouseDoubleClickInterval.

// The listing: the list view, the grid view, and the columns view's own middle column.
function tapped(index, tapCount, modifiers, root) {
    if (index < 0) return
    // Finder's two selection modifiers. Neither ever opens, and only the first tap of one counts,
    // so a modified double click selects once instead of toggling itself back off.
    if (modifiers & Qt.ControlModifier) {
        if (tapCount === 1) root.toggleSelectAt(index)
        return
    }
    if (modifiers & Qt.ShiftModifier) {
        if (tapCount === 1) root.extendSelectionTo(index)
        return
    }
    // What a tap means is the search's to say, not this file's: on a result the operator's ruling is
    // that it takes you to the file rather than launching it, and ui/js/Search.js activateAction
    // answers "open" everywhere else. A result answers on the first tap (2026-09-11), so the reveal
    // has already moved the pane by the time a second one arrives and that tap belongs to another
    // directory's row: it selects nothing and reveals nothing, tappedColumn's rule for its own reveal.
    var verb = Search.activateAction(root)
    if (verb === "reveal" && tapCount !== 1)
        return
    // Finder commits an open inline rename when you click away, and the field's own text is what
    // lands. It goes first so the write happens before the selection moves under it.
    root.commitOpenRename()
    // The plain tap replaces the selection with this row, Finder's rule: leaving the old one
    // standing would extend the next shift+click from an anchor nothing on screen names, and every
    // write operation targets the selection ahead of the cursor row.
    root.selectOnly(index)
    if (tapCount === 2 || verb === "reveal")
        root.act(verb)
}

// The columns view's own middle column, and ui/ColumnsArea.qml is its only caller, which is what
// keeps this out of the list and the grid. Its two neighbour columns already go into a directory on
// one tap, and the operator's 2026-09-11 ruling is that the column holding the cursor does the same,
// which is Finder's column view. A directory alone: everything else is an ordinary listing tap and
// reads tapped() above, so a file still opens on the second tap and a modifier still only selects.
function tappedMiddle(index, tapCount, modifiers, root) {
    var row = index >= 0 ? root.rowFor(index) : null
    var plain = (modifiers & (Qt.ControlModifier | Qt.ShiftModifier)) === 0
    if (!row || !row.d || !plain) {
        tapped(index, tapCount, modifiers, root)
        return
    }
    // The first tap moved the pane, so a second one is another directory's row, the same reason
    // tappedColumn refuses to reveal a neighbour twice.
    if (tapCount !== 1)
        return
    root.commitOpenRename()
    root.selectOnly(index)
    // A directory listed as a search result is still a result, so the one decision point answers here
    // too: it reveals rather than opening, exactly as the same row does in the list view.
    root.act(Search.activateAction(root))
}

// Right click, in all three views: the row under the pointer takes the cursor and the menu opens there.
// Which rows it then means is decided by the pressed row, ui/js/Drag.js carried()'s rule for the
// pointer's other gesture: pressed inside the selection the menu addresses all of it, pressed outside
// it that row replaces the selection. Every entry ui/ContextMenu.qml draws is built from the cursor
// row while Move to Trash, Compress and Move to Dropbox dispatch through Ops.targetIndices, which
// prefers the selection, so a right click that left a selection elsewhere standing trashed rows the
// menu had never described.
function tappedMenu(index, eventPoint, root, menu) {
    var picked = root.selectedIndices()
    if (picked.length > 0 && picked.indexOf(index) < 0)
        root.clearSelection()
    root.setCursor(index)
    menu.openAt(eventPoint.scenePosition)
}

// A neighbour column in the columns view is a peek with no cursor of its own, so its rows answer a
// verb rather than acting. One tap on a directory makes it the pane's listing, which is the column
// view's own reveal and not an open; only a second tap opens a file. A peeked row belongs to another
// directory and every menu action addresses the pane's cursor, so a right click there is routed by
// ColumnPane to menuOnNeighbour before this is asked.
function tappedColumn(row, button, tapCount) {
    if (!row || button === Qt.RightButton)
        return ""
    if (row.d)
        return tapCount === 1 ? "reveal" : ""
    return tapCount === 2 ? "open" : ""
}
