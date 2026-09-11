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
        current: String(read.current || ""),
        currentIndex: typeof read.currentIndex === "number" ? read.currentIndex : -1
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
    if (req.currentIndex >= 0 && req.currentIndex < req.filters.length) return req.currentIndex
    for (var i = 0; i < req.filters.length; i++) {
        if (String(req.filters[i].label || "") === req.current) {
            return i
        }
    }
    return 0
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

function reviewedMarks(previous, reviewed) {
    return reviewed.map(function(item) {
        var old = previous.find(function(mark) { return mark.path === item.path })
        return {path: item.path, bytes: item.bytes, uri: old && old.uri ? old.uri : Format.fileUri(item.path)}
    })
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

function directory(row) {
    return !!row && (row.d === true || (Format.isSymlink(row.p) && row.i === "folder"))
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
function reply(response, list, filterIndex) {
    if (response !== RESPONSE_OK) {
        return JSON.stringify({ response: response })
    }
    var answer = { response: RESPONSE_OK, uris: list.map(function(item) {
        return typeof item === "string" ? Format.fileUri(item) : item.uri
    }) }
    if (typeof filterIndex === "number") answer.filter = filterIndex
    return JSON.stringify(answer)
}
