.pragma library
.import "Picker.js" as Picker
.import "PickerMarks.js" as Marks

// What the chooser does with a line its location field reported, and nothing about the window:
// ui/js/PickerEntry.js says what the line is, this says what to do about it. The rules are the
// Windows file dialog's filename box: a folder is walked into and the box cleared, a file is
// selected in its parent, a missing path is refused and the dialog stays open. Every function is
// pure and answers a step ui/PickerNavigate.qml carries out, so tests/js/pickernavigate.js drives
// it with no window and no backend.

var NOT_LOCAL = "Not a local path"
var NO_SCHEME = "Unknown scheme"
var NO_HOST = "No host in the URL"
var NOT_YET = "Not supported yet"
var NOT_FOLDER = "Not a folder"
var CHOOSE_FOLDER = "Choose a folder"
var NOT_IN_WINDOW = "Not found in the first rows"
var NOT_LISTED = "Not shown in this listing"

// src/backend/peek.rs answers at most this many rows, names only, so the peek asks for all of them
// and a leaf goes unfound only in a directory larger than that.
var PEEK_ROWS = 512

function notFound(path) {
    return "Not found: " + path
}

function refusal(reason) {
    if (reason === "scheme") {
        return NO_SCHEME
    }
    if (reason === "host") {
        return NO_HOST
    }
    return NOT_LOCAL
}

function leafOf(path) {
    return path.substring(path.lastIndexOf("/") + 1)
}

// The step before any backend call, from what classify said, where the window stands and what it
// holds marked. A local path is peeked at its parent before anything opens, because a typed path
// is a claim about the disk and the listing is the only proof; the two exceptions are the root,
// which has no parent and is peeked itself, and the current directory, which the listing on
// screen has already proved. A marked file typed again is the second Return the box accepts on.
// Answers one of: nothing; say {message}; remote or share {answer}; accept; settle; peek {parent, path}.
function plan(answer, current, marks, folderMode) {
    if (answer.kind === "empty") {
        return { step: "nothing" }
    }
    if (answer.kind === "refused") {
        return { step: "say", message: refusal(answer.reason) }
    }
    if (answer.kind !== "local") {
        return { step: answer.kind, answer: answer }
    }
    if (!folderMode && !answer.wantsDir && Marks.marked(marks, answer.path)) {
        return { step: "accept" }
    }
    if (answer.path === current) {
        return { step: "settle" }
    }
    var parent = answer.path === "/" ? "/" : Picker.parentOf(answer.path)
    return { step: "peek", parent: parent, path: answer.path }
}

function rowNamed(rows, name) {
    for (var i = 0; i < rows.length; i++) {
        if (rows[i].n === name) {
            return rows[i]
        }
    }
    return null
}

// The step after the parent's peek answered. rows carry a name and the directory bit and nothing
// else, which is all this needs: the peek is name-sorted and the listing may not be, so where the
// row sits is the backend's locate's to say, never the peek's. Answers one of: open {path};
// select {parent, path}; say {message}; openSay {parent, message}.
function verdict(target, wantsDir, folderMode, peeked) {
    if (peeked.readFailed) {
        return { step: "say", message: notFound(target) }
    }
    if (target === "/") {
        return { step: "open", path: "/" }
    }
    var parent = Picker.parentOf(target)
    var row = rowNamed(peeked.rows, leafOf(target))
    if (row === null) {
        // corner: a peek carries only the first rows of a large directory, so a leaf missing from
        // them is unknown rather than absent; the parent opens and the footer says which it was.
        if (peeked.total > peeked.rows.length) {
            return { step: "openSay", parent: parent, message: NOT_IN_WINDOW }
        }
        return { step: "say", message: notFound(target) }
    }
    if (row.d) {
        return { step: "open", path: target }
    }
    if (wantsDir) {
        return { step: "say", message: NOT_FOLDER }
    }
    if (folderMode) {
        return { step: "say", message: CHOOSE_FOLDER }
    }
    return { step: "select", parent: parent, path: target }
}

// A rows line that arrived without the pending target. The backend is asked where the row sits,
// once: a locate sent after the listing settled sees the sort in force, and the rows it then asks
// for are the ones read. Answers one of: locate; wait; giveUp.
function missing(canLocate, held, pendingStart) {
    if (canLocate) {
        return { step: "locate" }
    }
    if (held !== pendingStart) {
        return { step: "wait" }
    }
    return { step: "giveUp" }
}

// The located line. One for another path is not this target's answer. An index below zero means
// the listing has no row with that path, the same fact giveUp says. Answers one of: nothing;
// say {message}; window {start}.
function located(path, index, target, windowSize) {
    if (path !== target) {
        return { step: "nothing" }
    }
    if (index < 0) {
        return { step: "say", message: NOT_LISTED }
    }
    return { step: "window", start: windowStart(index, windowSize) }
}

// The rows window that holds an index, a quarter window ahead of it as the list's own drift refetch
// starts; 0 when the listing's first window already reaches it.
function windowStart(index, windowSize) {
    return Math.max(0, Math.floor(index - windowSize / 4))
}

// The listing index of the row that is the pending target, or -1. A row's identity is the one a
// mark holds, Picker.rowPath's, so a Recent row is found by its whole path and never by a join.
function indexOf(rows, held, location, target) {
    for (var i = 0; i < rows.length; i++) {
        if (Picker.rowPath(location, rows[i].n) === target) {
            return held + i
        }
    }
    return -1
}
