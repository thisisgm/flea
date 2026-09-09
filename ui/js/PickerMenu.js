.pragma library

.import "Keymap.js" as Keymap
.import "Picker.js" as Picker
.import "PickerKeys.js" as PickerKeys
.import "Sort.js" as Sort

// What the chooser's context menu does, ui/PickerMenu.qml's two moves: where a right click aims it
// and where a chosen row goes. Pure over ui/PickerState.qml, so tests/js/pickermenu.js drives both
// without a window.

// The rows ui/js/Menu.js draws that no picker verb answers yet, each with the name the footer
// prints, ui/js/PickerKeys.js SHARED's own rule for a chord. Open, Copy path and Duplicate are
// menu rows and not key table actions, so they are named here and not there.
var UNBUILT = { open: "Open", copypath: "Copy path", duplicate: "Duplicate" }

function hintFor(action) {
    if (action === "cut") return "Ctrl+X"
    if (action === "copy") return "Ctrl+C"
    if (action === "paste") return "Ctrl+V"
    return Keymap.hintFor(action)
}

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
    if (action in UNBUILT) {
        state.message(UNBUILT[action] + " is not built in the chooser yet.", false)
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
