.pragma library

.import "Format.js" as Format

// Errors reach the user as one sentence, never a raw path or errno. StatusBar board rule 4: the
// sentence names the input and drops advice nobody can act on. named is the failing directory's own
// leaf, empty when the breadcrumb is already standing on it and there is nothing to add.
function sentence(where, message, named) {
    if (where === "scan") {
        if (denied(where, message)) {
            return named ? "Permission denied on " + named : "Permission denied"
        }
        // A dead share names itself: stale, disconnected and silent each read differently.
        if (shareStale(message)) return "That share changed, so this folder is no longer available."
        if (shareGone(message)) return "That share is disconnected; remount it."
        if (shareSilent(message)) return "That share is not responding."
        return named ? "That directory could not be read: " + named : "That directory could not be read."
    }
    if (where === "sort") {
        // Size and mtime are real orders, so the one refusal left is a key the wire never defined.
        return "Sorting by that column is not available."
    }
    if (where === "read") {
        return "The backend stopped responding; reopen Flea and try again."
    }
    // The write operations say what they were doing, because the operator is about to try it again.
    if (where === "undo" || where === "redo") {
        // The empty journal is the common case and the backend's own sentence is already the right one.
        return capitalised(message)
    }
    if (where === "rename") {
        if (nameRefusal(message)) return capitalised(message)
        if (readOnly(message)) return capitalised(message)
        return exists(message) ? "A file with that name is already here." : "That file could not be renamed."
    }
    // undo reverses a rename through the same call, so this sentence names no direction.
    if (where === "rename-kept") {
        return "Copied, but the old name was only partly removed. Check it."
    }
    // Deliberately not the capitalised branch: every other mkdir refusal reaches the UI through
    // src/error.rs from_io, which passes std::io::Error::to_string straight through, errno and all.
    if (where === "mkdir") {
        if (nameRefusal(message)) return capitalised(message)
        if (readOnly(message)) return capitalised(message)
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
        if (readOnly(message)) return capitalised(message)
        return "That file could not be duplicated."
    }
    if (where === "trash") {
        if (readOnly(message)) return capitalised(message)
        return "That could not be moved to Trash."
    }
    // The backend refused rows read from a listing it had already replaced, so nothing ran at all.
    if (where === "stale") {
        return "The listing changed before that arrived, so nothing was done."
    }
    // A window past its deadline names its mount, so the backend's own sentence is the one to show.
    if (where === "window") {
        return capitalised(message)
    }
    if (where === "transfer" || where === "archive" || where === "convert"
            || where === "link" || where === "linktarget" || where === "rename-stranded") {
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

// A read-only refusal carries its own sentence from src/error.rs, so the write branches keep it.
function readOnly(message) {
    return String(message).toLowerCase().indexOf("read-only") >= 0
}

// A per-filesystem name refusal carries its own sentence from src/backend/fsname.rs, so rename and mkdir keep it.
function nameRefusal(message) {
    var text = String(message)
    return text.indexOf("is not allowed in a name") >= 0 || text.indexOf("is reserved on this") >= 0
        || text.indexOf("cannot end in a dot") >= 0 || text.indexOf("cannot end in a space") >= 0
        || text.indexOf("cannot hold control character") >= 0
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

// ESTALE, ENOTCONN and ETIMEDOUT each read differently; the backend's words are src/error.rs.
function shareStale(message) {
    var text = String(message).toLowerCase()
    return text.indexOf("stale") >= 0 || text.indexOf("no longer available") >= 0
}
function shareGone(message) {
    var text = String(message).toLowerCase()
    return text.indexOf("not connected") >= 0
}
function shareSilent(message) {
    var text = String(message).toLowerCase()
    return text.indexOf("timed out") >= 0 || text.indexOf("timeout") >= 0
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

// A readable, searchable owner mode proves an observed listing denial belongs to another user.
function notYours(mode) {
    return (mode & OWNER_CAN_LIST) === OWNER_CAN_LIST ? " · not yours" : ""
}

/**
 * Return a user-facing sentence for a failed authentication helper's numeric exit status.
 * Timeout and the shell reserve 124 and 126/127. GIO output is suppressed, so other statuses
 * remain unclassified; they establish neither a TLS failure nor an authentication refusal.
 */
function connectFailure(exitCode) {
    if (exitCode === 124) return "Connect failed: host did not respond"
    if (exitCode === 126 || exitCode === 127)
        return "Connect failed: authentication helper is unavailable"
    return "Connect failed: the network location could not be opened"
}
