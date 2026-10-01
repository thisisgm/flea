.pragma library

.import "Format.js" as Format

// A search row's name is its path relative to the search root, so the display splits it here and
// the wire never carries a second shape; see docs/protocol.md "search".
function base(path) {
    var i = String(path).lastIndexOf("/")
    return i < 0 ? String(path) : String(path).substring(i + 1)
}

// The location column: the parent path relative to the scope, empty for a match in the root itself.
function location(path) {
    var i = String(path).lastIndexOf("/")
    return i < 0 ? "" : String(path).substring(0, i)
}

// The Stack's location column: the parent directory, with home written as a tilde.
function place(path, home) {
    var abs = String(path || "")
    if (abs.charAt(0) !== "/")
        abs = "/" + abs
    var cut = abs.lastIndexOf("/")
    var dir = cut <= 0 ? "/" : abs.substring(0, cut)
    return Format.tilde(dir, String(home || ""))
}

// Where to paint the accent run inside a name, mirroring the backend's own case-insensitive match.
// corner: a character whose lowercase form is a different length (Turkish dotted capital I) can
// shift the run by one; the name still renders whole, and the backend alone decides what matched.
function run(hay, needle) {
    var h = String(hay)
    var n = String(needle)
    if (n.length === 0) {
        return { start: -1, length: 0 }
    }
    var at = h.toLowerCase().indexOf(n.toLowerCase())
    return at < 0 ? { start: -1, length: 0 } : { start: at, length: n.length }
}
