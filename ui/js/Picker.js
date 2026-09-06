.pragma library

.import "Format.js" as Format

// The portal request tools/flea-portal puts in FLEA_PICKER, and the answer ui/picker.qml writes
// back. Everything here is pure so tests/js/picker.js can drive it without a window.

// The portal's own response codes, mirrored from tools/flea-portal: 1 is the user's no, and this
// front end never sends 2, because a window that opened at all can only pick or refuse.
var RESPONSE_OK = 0
var RESPONSE_CANCELLED = 1

var ALL_FILES = "All files"

// The rail's Recent row is a location and not a directory, so it is named by a token no listing
// path can equal: every path this window holds is absolute, and this one does not start with "/".
// SendPicker.html draws it first in the rail and draws this word where the path would be.
var RECENT = "flea:recent"
var RECENT_LABEL = "Recent"

function isRecent(location) {
    return location === RECENT
}

// SendPicker.html draws a chooser row as the name, a 70px size and an 80px date, so the two columns
// the window's rows also carry are hidden here at every width rather than at some of them.
var HIDDEN_COLS = ["mode", "kind"]

// Every field defaulted, because a request that arrived short must still open a window.
function request(text) {
    var read = {}
    try {
        read = JSON.parse(String(text || "{}")) || {}
    } catch (e) {
        read = {}
    }
    return {
        mode: read.mode === "save" || read.mode === "savefiles" ? read.mode : "open",
        title: String(read.title || ""),
        app: String(read.app || ""),
        accept: String(read.accept || ""),
        multiple: read.multiple === true,
        directory: read.directory === true,
        folder: String(read.folder || ""),
        file: String(read.file || ""),
        name: String(read.name || ""),
        files: Array.isArray(read.files) ? read.files : [],
        filters: Array.isArray(read.filters) ? read.filters : [],
        current: String(read.current || "")
    }
}

// The caller's own title wins; the fallback says what the window is for rather than naming Flea.
function title(req) {
    if (req.title.length > 0) {
        return req.title
    }
    if (req.mode === "save" || req.mode === "savefiles") {
        return "Save file"
    }
    return req.directory ? "Choose folder" : "Choose file"
}

// Only a portal identity is ever shown here. A host application has none, and inventing one from
// the window title would put a name the desktop never vouched for under a trusted-looking label.
function subtitle(req) {
    return req.app.length > 0 ? "Requested by " + req.app : ""
}

// The primary button. The count rides on it in the multiple case, which is the board's "Send 3".
function acceptLabel(req, count) {
    var base = req.accept.length > 0 ? req.accept : defaultAccept(req)
    if (req.multiple && count > 1) {
        return base + " " + count
    }
    return base
}

function defaultAccept(req) {
    if (req.mode === "save") {
        return "Save"
    }
    if (req.mode === "savefiles" || req.directory) {
        return "Choose folder"
    }
    return "Open"
}

// The footer's left half: what is checked, and what it weighs.
function statusLine(count, bytes) {
    if (count === 0) {
        return "0 selected"
    }
    return count + " selected · " + Format.size(bytes)
}

// The footer's right half, which says only the keys this mode actually answers.
function hints(req) {
    if (req.mode === "save") {
        return "Enter save · Esc cancel"
    }
    var pick = req.directory ? "Enter open · Space mark folder" : "Space select · Enter open/send"
    return pick + " · Esc cancel"
}

// The chip row: the caller's filters, then All files, which is always explicit. An empty list is no
// row at all, because a chooser with one chip is a chooser with nothing to choose between.
function chips(req) {
    if (req.filters.length === 0) {
        return []
    }
    var out = []
    for (var i = 0; i < req.filters.length; i++) {
        out.push({ label: String(req.filters[i].label || ""), index: i })
    }
    out.push({ label: ALL_FILES, index: -1 })
    return out
}

// Which chip starts active: the caller's current_filter when it names one of them, else the first.
function currentChip(req) {
    if (req.filters.length === 0) {
        return -1
    }
    for (var i = 0; i < req.filters.length; i++) {
        if (String(req.filters[i].label || "") === req.current) {
            return i
        }
    }
    return 0
}

// Sample input: "*.tar.gz" becomes /^.*\.tar\.gz$/i. Every other character is taken literally, so a
// filename with a bracket in it cannot turn the caller's glob into a character class.
function globToRegExp(glob) {
    var out = ""
    for (var i = 0; i < glob.length; i++) {
        var c = glob.charAt(i)
        if (c === "*") {
            out += ".*"
        } else if (c === "?") {
            out += "."
        } else {
            out += c.replace(/[.\\+^$[\]{}()|\/-]/g, "\\$&")
        }
    }
    return new RegExp("^" + out + "$", "i")
}

// A filter narrows what is easy to find; it never rejects. A filter carrying only mime rules cannot
// be answered by a listing row, which knows an icon name and not a mime type, so it narrows nothing.
function matchesFilter(name, filter) {
    if (!filter || !Array.isArray(filter.globs) || filter.globs.length === 0) {
        return true
    }
    for (var i = 0; i < filter.globs.length; i++) {
        if (globToRegExp(String(filter.globs[i])).test(name)) {
            return true
        }
    }
    return false
}

// The listing rows a chip leaves standing, in the backend's own order, or null when nothing is
// narrowing. Directories always stand: a filter that hides the way out of a directory is a trap.
// Same two index spaces as ui/js/Filter.js, and ui/js/Filter.js at() and viewOf() convert them.
function shownRows(rows, held, filter) {
    if (!filter) {
        return null
    }
    var out = []
    for (var i = 0; i < rows.length; i++) {
        if (rows[i].d === true || matchesFilter(rows[i].n, filter)) {
            out.push(held + i)
        }
    }
    return out
}

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

function totalBytes(marks) {
    var sum = 0
    for (var i = 0; i < marks.length; i++) {
        sum += marks[i].bytes
    }
    return sum
}

function paths(marks) {
    var out = []
    for (var i = 0; i < marks.length; i++) {
        out.push(marks[i].path)
    }
    return out
}

// The save name is the one filename a client hands this window, so it is a trust boundary and it
// gets the rule the rest of the product already applies. Mirrored from src/backend/ops.rs
// valid_name() and kept identical to it, character for character, so the two cannot drift.
function validName(name) {
    var text = String(name)
    return text.length > 0 && text !== "." && text !== ".."
        && text.indexOf("/") < 0 && text.indexOf("\0") < 0
}

// What both the strip and the status line say about a name validName() refuses, in ops.rs's words.
var NAME_REFUSED = "a name cannot be empty, . or .. , or contain a separator"

function join(dir, name) {
    return dir === "/" ? "/" + name : dir + "/" + name
}

// A row's own identity, which is what a mark holds and what the caller is answered with. A Recent
// listing's base is "/" and its rows are named by their path under it, so the row's path is that
// name and never a join onto the location token; see docs/protocol.md "listpaths".
function rowPath(location, name) {
    return isRecent(location) ? "/" + name : join(location, name)
}

function parentOf(path) {
    var cut = String(path).lastIndexOf("/")
    if (cut <= 0) {
        return "/"
    }
    return String(path).substring(0, cut)
}

// The one place a path becomes a URI: xdg-desktop-portal drops anything that is not file://, and
// omarchy-file-select turns what comes back into a path with GLib, which refuses a bad encoding.
function uris(list) {
    var out = []
    for (var i = 0; i < list.length; i++) {
        out.push(Format.fileUri(list[i]))
    }
    return out
}

// What tools/flea-portal reads out of the reply file. A refusal carries no URI at all.
function reply(response, list) {
    if (response !== RESPONSE_OK) {
        return JSON.stringify({ response: response })
    }
    return JSON.stringify({ response: RESPONSE_OK, uris: uris(list) })
}
