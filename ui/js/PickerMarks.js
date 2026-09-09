.pragma library

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

// Space. In single mode the new mark replaces the old one, which is the board's "Space replaces the
// prior check"; unmarking what is already marked always wins, so a second Space clears it.
function toggle(marks, path, bytes, multiple) {
    var out = []
    var found = false
    for (var i = 0; i < marks.length; i++) {
        if (marks[i].path === path) {
            found = true
            continue
        }
        out.push(marks[i])
    }
    if (found) {
        return multiple ? out : []
    }
    if (!multiple) {
        return [{ path: path, bytes: bytes }]
    }
    out.push({ path: path, bytes: bytes })
    return out
}

// Shift+click. Every unmarked row from the anchor to the clicked one joins the marks, in that
// order, and a row already checked stays checked once: nothing comes off under Shift. In single
// mode the range is the clicked row alone, so it toggles the way Space does.
function markRange(marks, rows, multiple) {
    if (rows.length === 0) {
        return marks
    }
    if (!multiple) {
        var last = rows[rows.length - 1]
        return toggle(marks, last.path, last.bytes, false)
    }
    var out = marks.slice()
    for (var i = 0; i < rows.length; i++) {
        if (!marked(out, rows[i].path)) {
            out.push({ path: rows[i].path, bytes: rows[i].bytes })
        }
    }
    return out
}
