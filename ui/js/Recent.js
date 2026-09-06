.pragma library

// The picker's Recent location, read out of the desktop's own history and never written to. Flea
// keeps no history of its own; this is the freedesktop file every application on the box appends to,
// so everything here treats it as untrusted input. Pure, so tests/js/recent.js drives it with no
// window; ui/PickerRecent.qml is the Qt XML reader that feeds it.

// The freedesktop recent file, and where XDG_DATA_HOME defaults to when the session did not set it.
var HISTORY_LEAF = "recently-used.xbel"
var DEFAULT_DATA_HOME = "/.local/share"

// A hostile or merely enormous history is still a listing this window has to build, so the output
// loop below stops here and hands the rail the newest LIMIT paths. It bounds that loop and nothing
// else: ui/PickerRecent.qml reads the whole model, because bounding the read in file order sorts
// an arbitrary LIMIT of a file XBEL never promised an order for, and a 5,000 bookmark history in
// oldest-first order then answered with its 500 oldest.
var LIMIT = 500

// Sample input: ("/home/gm/.local/share", "/home/gm") and ("", "/home/gm"); an XDG_DATA_HOME that
// is not an absolute path is not a data home, so the default answers for it too.
function historyPath(dataHome, home) {
    var root = String(dataHome || "")
    if (root.charAt(0) !== "/") {
        root = String(home || "") + DEFAULT_DATA_HOME
    }
    return root.replace(/\/+$/, "") + "/" + HISTORY_LEAF
}

// Sample input: "file:///home/gm/a%20b.png" becomes "/home/gm/a b.png"; everything else becomes "".
// The rules, in the order they are applied, because each one is a way a bookmark could otherwise
// become a path this window never meant to open:
// a scheme that is not file is not a local file, so smb:// and trash:// are refused whole;
// an authority that is neither empty nor localhost names another machine and is refused with it;
// a malformed percent sequence makes decodeURIComponent throw and is refused there, while a
// well-formed one decodes to whatever it names, so the decoded form is re-checked rather than
// trusted; and a decoded path must still be one absolute path, so a NUL, a newline or any other
// control character refuses it.
function pathOf(href) {
    var raw = String(href || "")
    if (raw.substring(0, 7).toLowerCase() !== "file://") {
        return ""
    }
    var rest = raw.substring(7)
    // The authority is everything before the path's own leading slash.
    var cut = rest.indexOf("/")
    if (cut < 0) {
        return ""
    }
    var authority = rest.substring(0, cut).toLowerCase()
    if (authority.length > 0 && authority !== "localhost") {
        return ""
    }
    var decoded = ""
    try {
        decoded = decodeURIComponent(rest.substring(cut))
    } catch (e) {
        return ""
    }
    if (decoded.charAt(0) !== "/" || decoded === "/") {
        return ""
    }
    // A control character cannot appear in a path this window will draw or send back as a URI.
    if (/[\x00-\x1f\x7f]/.test(decoded)) {
        return ""
    }
    return decoded
}

// The rail's rows, newest first. Each bookmark is { href, stamp }, exactly what ui/PickerRecent.qml
// reads off the XBEL. Sample stamp: "2026-08-30T11:32:04Z", which sorts as a string because it is
// fixed-width UTC; a stamp in any other shape sorts among its own kind and never throws.
// A path seen twice keeps its first, newest position, the same rule Places.favorites follows.
function paths(bookmarks) {
    var rows = []
    for (var i = 0; i < bookmarks.length; i++) {
        var path = pathOf(bookmarks[i].href)
        if (path.length === 0) {
            continue
        }
        rows.push({ path: path, stamp: String(bookmarks[i].stamp || ""), at: rows.length })
    }
    // The file's own order is the tie-break, so two bookmarks sharing a stamp never swap between reads.
    rows.sort(function (a, b) {
        if (a.stamp === b.stamp) {
            return a.at - b.at
        }
        return a.stamp < b.stamp ? 1 : -1
    })
    var out = []
    var seen = {}
    for (var j = 0; j < rows.length && out.length < LIMIT; j++) {
        if (seen[rows[j].path]) {
            continue
        }
        seen[rows[j].path] = true
        out.push(rows[j].path)
    }
    return out
}
