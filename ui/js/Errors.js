.pragma library

.import "Format.js" as Format

// Errors reach the user as one sentence, never a raw path or errno.
function sentence(where, message) {
    if (where === "scan") {
        if (denied(where, message)) {
            return "Permission was denied; check access and try again."
        }
        return "That directory could not be read; check the path and try again."
    }
    if (where === "sort") {
        // Size and mtime are real orders, so the one refusal left is a key the wire never defined.
        return "Sorting by that column is not available."
    }
    if (where === "read") {
        return "The backend stopped responding; reopen Flea and try again."
    }
    // The write operations say what they were doing, because the operator is about to try it again.
    if (where === "undo") {
        // The empty journal is the common case and the backend's own sentence is already the right one.
        return capitalised(message)
    }
    if (where === "rename") {
        return exists(message) ? "A file with that name is already here." : "That file could not be renamed."
    }
    // Deliberately not the capitalised branch: every other mkdir refusal reaches the UI through
    // src/error.rs from_io, which passes std::io::Error::to_string straight through, errno and all.
    if (where === "mkdir") {
        return exists(message) ? "A folder or file with that name is already here."
                               : "That folder could not be created."
    }
    // The state file: what was asked for is still on screen, so the sentence says what did not last.
    if (where === "state") {
        return "That setting could not be saved."
    }
    // And the other way round: main() left a ui.json it could not read alone, so none of it is used.
    if (where === "statefile") {
        return "Your saved settings could not be read, so these are the defaults."
    }
    if (where === "duplicate") {
        return "That file could not be duplicated."
    }
    if (where === "trash") {
        return "That could not be moved to Trash."
    }
    if (where === "transfer" || where === "archive" || where === "convert") {
        return capitalised(message)
    }
    return "That action could not be completed; try again."
}

// One condition, two spellings: rename gets the errno's "File exists", while src/backend/ops.rs
// words mkdir's own collision "a folder or file with that name already exists".
function exists(message) {
    var text = String(message).toLowerCase()
    return text.indexOf("file exists") >= 0 || text.indexOf("already exists") >= 0
}

// The backend already writes these as sentences; this only makes one read like one in the bar.
function capitalised(message) {
    var text = String(message)
    if (text.length === 0) {
        return "That action could not be completed; try again."
    }
    var out = text.charAt(0).toUpperCase() + text.substring(1)
    return out.charAt(out.length - 1) === "." ? out : out + "."
}

// The one place the backend's errno wording is read, so the sentence and the pane state can never
// disagree about which failure this is.
function denied(where, message) {
    return where === "scan" && String(message).toLowerCase().indexOf("permission denied") >= 0
}

// Which state the listing area reaches when a listing fails. A denial is the canvas's Locked tile on
// States.dc.html, which draws the lock mark over the directory's own mode string; the rest are Error.
function listingState(where, message) {
    return denied(where, message) ? "locked" : "error"
}

// That tile's one line, drawn verbatim as "rwx------ · not yours". src/backend/meta.rs answers mode 0
// when the stat itself failed, and a real st_mode always carries its file-type bits, so a zero here
// means "I could not look" and earns no line rather than a false "---------".
function lockedLine(mode) {
    if (!(mode > 0)) {
        return ""
    }
    return Format.permissions(mode) + notYours(mode)
}

// Entering a directory needs its execute bit and listing it needs its read bit, so an owner holding
// both would not have been denied: a denial on such a directory proves the operator is not the
// owner. Below that the owner is locked out too, and the line claims nothing it cannot know.
var OWNER_CAN_LIST = 0o500

function notYours(mode) {
    return (mode & OWNER_CAN_LIST) === OWNER_CAN_LIST ? " · not yours" : ""
}

// The whole line a failed listing puts on the pane. Locked draws the directory's own mode string and
// falls back to the sentence when the backend could not stat it either, so the surface is never a
// mark with nothing under it.
function paneLine(state, message, mode) {
    // Named for what it is rather than "sentence", which is this file's own function one scope out.
    var fallback = message === null || message === undefined ? "" : String(message)
    if (state !== "locked") {
        return fallback
    }
    var modeLine = lockedLine(mode)
    return modeLine.length > 0 ? modeLine : fallback
}
