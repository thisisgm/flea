.pragma library

.import "Picker.js" as Picker

// What the chooser's check boxes hold and how a key or a click changes it. A mark is a path and
// its size, never a row number; ui/js/Picker.js weighs and answers the list this builds.

// A mark is a path, never a row number: the board's rule is that a checked identity survives Back
// and Parent, and a row number rebinds to whatever else lands in that position.
function marked(marks, path) {
    for (var i = 0; i < marks.length; i++) {
        if (marks[i].path === path) {
            return true
        }
    }
    return false
}

// Space, a plain click and Ctrl+click: the row's check box flips. In single mode the new mark
// replaces the old one, which is the board's "Space replaces the prior check"; unmarking what is
// already marked always wins, so a second Space or click clears it.
function toggle(marks, path, bytes, multiple) {
    if (marked(marks, path)) {
        return multiple ? unmark(marks, path) : []
    }
    if (!multiple) {
        return [{ path: path, bytes: bytes }]
    }
    var out = marks.slice()
    out.push({ path: path, bytes: bytes })
    return out
}

// The mark comes off if it stands. toggle's clearing half, and on its own the second tap of a
// folder double click, so the walk in leaves no check behind whatever the first tap did.
function unmark(marks, path) {
    var out = []
    for (var i = 0; i < marks.length; i++) {
        if (marks[i].path !== path) {
            out.push(marks[i])
        }
    }
    return out
}

// Return. Explicit marks win; without one, a file under the cursor is the answer. Recent keeps its
// explicit-mark rule, and a directory is opened by PickerState before this point.
function answer(marks, location, row) {
    if (marks.length > 0) {
        return Picker.paths(marks)
    }
    return row && !row.d && !Picker.isRecent(location) ? [Picker.rowPath(location, row.n)] : []
}

// Ctrl+A. Every unmarked row joins the marks, in that order, and a row already checked stays
// checked once: nothing comes off here.
function markRange(marks, rows) {
    var out = marks.slice()
    for (var i = 0; i < rows.length; i++) {
        if (!marked(out, rows[i].path)) {
            out.push({ path: rows[i].path, bytes: rows[i].bytes })
        }
    }
    return out
}

// Shift+click. The rows from the anchor to the clicked one act as one check box: when every one
// of them is already checked the click unmarks them all, otherwise it marks the ones missing. In
// single mode the range is the clicked row alone, so it toggles the way Space does.
function toggleRange(marks, rows, multiple) {
    if (rows.length === 0) {
        return marks
    }
    if (!multiple) {
        var last = rows[rows.length - 1]
        return toggle(marks, last.path, last.bytes, false)
    }
    var out = marks
    for (var i = 0; i < rows.length; i++) {
        if (!marked(out, rows[i].path)) {
            return markRange(marks, rows)
        }
        out = unmark(out, rows[i].path)
    }
    return out
}
