.pragma library

.import "Ops.js" as Ops
.import "Picker.js" as Picker
.import "PickerKeys.js" as PickerKeys
.import "Sort.js" as Sort

// What the chooser's context menu does, ui/PickerMenu.qml's two moves: where a right click aims it
// and where a chosen row goes. Pure over ui/PickerState.qml, so tests/js/pickermenu.js drives both
// without a window.

// A right click on a row, ui/js/Tap.js tappedMenu for marks. Pressed on a marked row the menu
// addresses every mark standing in this directory; pressed on an unmarked one those marks drop
// and the row alone is what Cut and Move to Trash then act on, ui/js/PickerOps.js pathsFor's
// rule, so the menu never describes one row while an operation takes others. A mark in another
// directory is not on screen and stands. The cursor moves to the row, which is what the entries
// read. Answers whether there was a row to aim at.
function aim(state, index) {
    var row = state.rowFor(index)
    if (!row)
        return false
    var path = Picker.rowPath(state.path, row.n)
    var standing = []
    var onMark = false
    for (var m = 0; m < state.marks.length; m++) {
        if (Picker.parentOf(state.marks[m].path) !== state.path)
            continue
        standing.push(state.marks[m].path)
        if (state.marks[m].path === path)
            onMark = true
    }
    if (standing.length > 0 && !onMark)
        state.dropMarks(standing)
    state.setCursor(index)
    // The menu restores the focus it found. Put both logical and Qt focus on the list first, so
    // Ctrl+Z after Move to Trash reaches the operation journal instead of a text field's undo.
    state.focusList()
    return true
}

// A chosen row. col: and sort: are the prefixed verbs ui/Pane.qml's onChosen and ui/js/Focus.js
// split off, and columns is qs module ViewState, handed in because a library cannot import a
// singleton. Every other row is a key table action and takes the chord's own route through
// ui/js/PickerKeys.js act, so a menu row and its key cannot come to mean two things.
function route(action, state, ops, columns) {
    if (action.indexOf("col:") === 0) {
        columns.toggleColumn(action.substring("col:".length))
        return
    }
    if (action.indexOf("sort:") === 0) {
        sortBy(state, action.substring("sort:".length))
        return
    }
    // Open and Copy path address the cursor row alone. In Recent join resolves the row's own path,
    // and at / it does not add a second slash. Neither action submits the portal request.
    if (action === "open" || action === "copypath") {
        var row = state.rowFor(state.cursorIndex)
        if (!row)
            return
        var path = state.join(state.path, row.n)
        if (action === "open")
            state.openFile(path)
        else
            state.copyText(path)
        return
    }
    // Duplicate has no key (Ctrl+D pages in keys.toml), so it is the one row that does not pass
    // through the key table. ui/js/Ops.js duplicate unmodified: the cursor row alone, never the
    // marks, one {c:"duplicate",path} line; ui/PickerWire.qml seats the copy when the reply lands.
    if (action === "duplicate") {
        Ops.duplicate(state)
        return
    }
    PickerKeys.act(action, state, ops)
}

// Sort by's flyout: ui/js/Sort.js column, the header click's own path, so the order lands in
// ui.json the same way. Recent is the history's own order, ui/picker.qml's rule for the header
// there, and a row that cannot reorder says why instead of asking the backend to refuse.
function sortBy(state, key) {
    if (state.recent) {
        state.message("Recent keeps the history's own order.", false)
        return
    }
    Sort.column(state, key)
}
