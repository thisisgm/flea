.pragma library

// Which columns a list row of a given width draws. The name is the one column a file manager
// cannot do without, so it is the one that never loses: each metadata column drops instead, at
// the width where keeping it would push the name under its floor. ui/Header.qml and ui/Row.qml
// both resolve their set here, from the same width, so the header can never head a column the
// rows below it are not drawing.

// The optional columns, widest first, which is the order they drop in. Kind goes first: the row
// already marks its kind with a glyph and most names carry the extension, so it is the most
// redundant column as well as the widest. Mode goes last, because it is the permanent column.
var DROP_ORDER = ["kind", "date", "size", "mode"]

// The row width each optional column needs before it is drawn, keyed by column. t carries the
// tokens ui/Theme.qml resolved: rowPaddingX, gap, iconSize, nameMin, and one width per column.
// A floor includes every column that outlives it, because they drop in order and a wider column
// never survives a narrower one, which is what makes the four floors nest.
function floors(t) {
    // The name's own slot at its floor: the row padding either side, the mark and the gap after it.
    var running = t.rowPaddingX + t.iconSize + t.gap + t.nameMin + t.rowPaddingX
    var out = {}
    for (var i = DROP_ORDER.length - 1; i >= 0; i--) {
        var key = DROP_ORDER[i]
        running += t[key] + t.gap
        out[key] = running
    }
    return out
}

// One boolean per optional column, for a row of this width. hidden is the user's own set (keys
// "mode"/"size"/"date"/"kind", from qs module ViewState), subtracted from what the width
// affords: a hidden column never draws, and width still wins over a column the user wants back —
// the name cannot be crowded out by a column the pane is too narrow to carry.
function set(width, t, hidden) {
    var f = floors(t)
    var h = {}
    var list = hidden || []
    for (var i = 0; i < list.length; i++) h[list[i]] = true
    return {
        mode: width >= f.mode && !h["mode"],
        size: width >= f.size && !h["size"],
        date: width >= f.date && !h["date"],
        kind: width >= f.kind && !h["kind"]
    }
}

// The drawn set as one string, in the order the columns are laid out, so a test can read the
// header's and a row's off the same seam and compare them.
function names(s) {
    var out = ["name"]
    for (var i = DROP_ORDER.length - 1; i >= 0; i--) {
        var key = DROP_ORDER[i]
        if (s[key])
            out.push(key)
    }
    return out.join(",")
}
