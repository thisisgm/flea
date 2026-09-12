.pragma library

// Which columns a list row of a given width draws. The name is the one column a file manager
// cannot do without, so it is the one that never loses: each metadata column drops instead, at
// the width where keeping it would push the name under its floor. ui/Header.qml and ui/Row.qml
// both resolve their set here, from the same width, so the header can never head a column the
// rows below it are not drawing.

// The optional columns, first to drop first, which is the order they drop in. Age drops first: it
// is the newest column and the one to go as the row narrows, and dropping it there keeps every
// older floor exactly where the tests pinned them. Kind drops second for the reason it always
// had -- the row already marks its kind with a glyph and the name carries the extension -- and
// mode goes last, because it is the permanent column.
var DROP_ORDER = ["age", "kind", "date", "size", "mode"]

// The row width each optional column needs before it is drawn, keyed by column. t carries the
// tokens ui/Theme.qml resolved: rowPaddingX, gap, iconSize, nameMin, and one width per column.
// A floor includes every column that outlives it, because they drop in order and a wider column
// never survives a narrower one, which is what makes the five floors nest.
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
// "mode"/"size"/"date"/"kind"/"age", from qs module ViewState), subtracted from what the width
// affords: a hidden column never draws, and width still wins over a column the user wants back,
// so the name cannot be crowded out by a column the pane is too narrow to carry.
function set(width, t, hidden) {
    var f = floors(t)
    var h = {}
    var list = hidden || []
    for (var i = 0; i < list.length; i++) h[list[i]] = true
    return {
        mode: width >= f.mode && !h["mode"],
        size: width >= f.size && !h["size"],
        date: width >= f.date && !h["date"],
        kind: width >= f.kind && !h["kind"],
        age: width >= f.age && !h["age"]
    }
}

// DualPane protects its name floor without reserving absent Mode and Kind columns; Age joins
// Size and Date there, so both layouts draw the same optional columns where width affords them.
function dualSet(width, t, hidden) {
    var base = 2 * t.rowPaddingX + t.iconSize + t.gap + t.nameMin
    var showSize = (hidden || []).indexOf("size") < 0 && width >= base + t.size
    var showDate = (hidden || []).indexOf("date") < 0
        && width >= base + (showSize ? t.size : 0) + t.date
    // Age drops first in the window's order, so in the cascade it is gained last and never
    // outlives date: a dual row without date cannot show the cheaper rendering of the same fact.
    var showAge = showDate && (hidden || []).indexOf("age") < 0
        && width >= base + (showSize ? t.size : 0) + (showDate ? t.date : 0) + t.age
    return {mode: false, kind: false, size: showSize, date: showDate, age: showAge}
}

// The width the drawn cells claim from the row's right edge, padding included: the same chain
// ui/RowCells.qml anchors those cells with, resolved to one number so the row can anchor the
// name and its editor to it. s is the SHOWN set (not the afforded one), so a search row, which
// keeps Size alone, claims only what it draws.
function drawnWidth(s, t, dual) {
    var used = t.rowPaddingX + (s.age ? t.age : 0)
    if (s.kind) used += t.kind + (s.age && !dual ? t.gap : 0)
    if (s.date) used += t.date + (s.kind && !dual ? t.gap : 0)
    if (s.size) used += t.size + (s.date && !dual ? t.gap : 0)
    if (s.mode) used += t.mode + (s.size && !dual ? t.gap : 0)
    return used
}

function names(s) {
    var out = ["name"]
    for (var i = DROP_ORDER.length - 1; i >= 0; i--) {
        var key = DROP_ORDER[i]
        if (s[key])
            out.push(key)
    }
    return out.join(",")
}
