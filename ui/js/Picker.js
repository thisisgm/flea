.pragma library

.import "Format.js" as Format
.import "PickerFilters.js" as Filters

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
// the window's rows also carry are its schema-default hidden set, not a second stored state.
var HIDDEN_COLS = ["mode", "kind"]
function hiddenColumns(columns, defaults, shared) { return JSON.stringify(columns) === JSON.stringify(defaults) ? HIDDEN_COLS : shared }

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

// The selection clause: what is checked, and what it weighs.
function statusLine(count, bytes) {
    if (count === 0) {
        return "0 selected"
    }
    return count + " selected · " + Format.size(bytes)
}

// How many rows the listing holds, in ui/StatusBar.qml's words. Nothing while it loads: a count
// of a listing that has not arrived would be a zero that is not true.
function countText(listingState, total) {
    if (listingState === "empty") {
        return "empty"
    }
    if (listingState !== "ready") {
        return ""
    }
    return total + (total === 1 ? " item" : " items")
}

// The footer's left half at rest: the count, then the selection clause only while something is
// checked, the idiom the bar uses. The total is the listing's own and never the shown one, which
// shownRows takes from the held window alone and so would undercount a large directory under a chip.
function footerLine(listingState, total, count, bytes) {
    var base = countText(listingState, total)
    if (count === 0) {
        return base
    }
    return base.length > 0 ? base + "   " + statusLine(count, bytes) : statusLine(count, bytes)
}

// The footer's right half, which says only the keys this mode actually answers.
function hints(req) {
    if (req.mode === "save") {
        return "Enter save · Esc cancel"
    }
    var pick = req.directory ? "Enter open · Space mark folder" : "Space select · Enter open/send"
    return pick + " · : location · Esc cancel"
}

// The filters the chips stand for: the caller's when it sent any, else the user's own from
// ~/.config/flea/filters.toml (ui/PickerFilters.qml). An app that sent filters never sees a config
// pill, so its dialog is exactly the one it asked for.
function filterList(req, config) {
    if (req.filters.length > 0) {
        return req.filters
    }
    return Array.isArray(config) ? config : []
}

// The chip row. The caller's filters come first and All files after them, because the caller's
// first is what it wants active. Config pills come after All files, which stands first and active,
// so nothing is hidden the caller did not ask to hide; they are labelled by extension the Windows
// way. An empty list is no row at all, because a chooser with one chip has nothing to choose between.
function chips(req, config) {
    var out = []
    if (req.filters.length > 0) {
        for (var i = 0; i < req.filters.length; i++) {
            out.push({ label: String(req.filters[i].label || ""), index: i })
        }
        out.push({ label: ALL_FILES, index: -1 })
        return out
    }
    var own = filterList(req, config)
    if (own.length === 0) {
        return []
    }
    out.push({ label: ALL_FILES, index: -1 })
    for (var j = 0; j < own.length; j++) {
        out.push({ label: Filters.labelFor(own[j]), index: j })
    }
    return out
}

// Which chip starts active: the caller's current_filter when it names one of them, else the first.
// A caller that sent none starts on All files, whatever config pills stand beside it.
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

// A filter narrows what is easy to find; it never rejects. Globs match the row's name and mime
// rules its icon class, both legs ANDed, so a filter carrying both narrows to the intersection.
// ui/js/PickerFilters.js matchesRow holds the rules and says what an icon name can confirm.
function matchesFilter(row, filter) {
    return Filters.matchesRow(row, filter)
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
        if (rows[i].d === true || matchesFilter(rows[i], filter)) {
            out.push(held + i)
        }
    }
    return out
}

// The rows both the chip and the typed query leave standing, in the backend's order: each list is
// an ascending subsequence of the held window, so what both hold is one too, and directories stay
// ahead of files the way they arrived. Either list alone answers when the other narrows nothing.
function narrow(byChip, byQuery) {
    if (byChip === null || byQuery === null) {
        return byChip === null ? byQuery : byChip
    }
    var keep = {}
    for (var i = 0; i < byQuery.length; i++) {
        keep[byQuery[i]] = true
    }
    return byChip.filter(function (row) { return keep[row] === true })
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

// The filter a pick was made under, as the portal's current_filter result: the caller's own label
// or the one a config pill shows, with its rules. All files is no filter and a filter with no rule
// narrows nothing, so neither is echoed; the caller would learn nothing from either.
function pickedFilter(filter) {
    if (!filter) {
        return null
    }
    var globs = Array.isArray(filter.globs) ? filter.globs.map(String) : []
    var mimes = Array.isArray(filter.mimes) ? filter.mimes.map(String) : []
    if (globs.length + mimes.length === 0) {
        return null
    }
    var label = filter.label ? String(filter.label) : Filters.labelFor(filter)
    return { label: label, globs: globs, mimes: mimes }
}

// What tools/flea-portal reads out of the reply file. A refusal carries no URI and no filter.
function reply(response, list, filter) {
    if (response !== RESPONSE_OK) {
        return JSON.stringify({ response: response })
    }
    var out = { response: RESPONSE_OK, uris: uris(list) }
    var picked = pickedFilter(filter)
    if (picked) {
        out.current_filter = picked
    }
    return JSON.stringify(out)
}
