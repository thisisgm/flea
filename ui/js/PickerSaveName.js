.pragma library
.import "Picker.js" as Picker
.import "PickerEntry.js" as PickerEntry
.import "PickerNavigate.js" as Navigate

// What the save mode's Filename box does with a path, and nothing about the box: the Windows save
// dialog's rule, where a name holding a separator or starting with "~" is a path. A folder opens
// and the box clears; a file opens its parent and its leaf stays as the name, whether or not the
// file is there, because a save names a file that may not exist yet. A URL is refused: a save
// answers a local URI and there is nothing to download. Every function is pure and answers a step
// ui/PickerNavigate.qml carries out, so tests/js/pickersavename.js drives it with no window.

var NEEDS_LOCAL = "Save needs a local path"
var PATH_HINT = "Path · Return opens the folder, or lands the name in it"

// A plain name is saved as it is; a path goes through classify before anything opens.
function isPath(name) {
    var text = String(name)
    return text.indexOf("/") >= 0 || text.charAt(0) === "~"
}

// The step before the backend is asked. Every path is peeked at its parent, the root at itself,
// because the parent is what has to exist for a save to land. Answers one of: say {message};
// peek {parent, path, wantsDir}.
function plan(name, current, home) {
    var answer = PickerEntry.classify(name, current, home)
    if (answer.kind === "refused") {
        return { step: "say", message: Navigate.refusal(answer.reason) }
    }
    if (answer.kind !== "local") {
        return { step: "say", message: NEEDS_LOCAL }
    }
    var parent = answer.path === "/" ? "/" : Picker.parentOf(answer.path)
    return { step: "peek", parent: parent, path: answer.path, wantsDir: answer.wantsDir }
}

// The step after the parent's peek answered. Answers one of: say {message}; open {path};
// name {parent, name}.
function verdict(target, wantsDir, peeked) {
    var parent = target === "/" ? "/" : Picker.parentOf(target)
    // The file need not exist, so the missing thing is the parent, and that is what the footer names.
    if (peeked.readFailed) {
        return { step: "say", message: Navigate.notFound(parent) }
    }
    if (target === "/") {
        return { step: "open", path: "/" }
    }
    var leaf = Navigate.leafOf(target)
    var row = Navigate.rowNamed(peeked.rows, leaf)
    if (row !== null && row.d) {
        return { step: "open", path: target }
    }
    if (wantsDir) {
        return { step: "say", message: row === null ? Navigate.notFound(target) : Navigate.NOT_FOLDER }
    }
    // corner: a peek carries only the first rows of a large directory, so a folder past them is
    // taken for a new name; the caller then hears a folder's URI and refuses the write itself.
    return { step: "name", parent: parent, name: leaf }
}
