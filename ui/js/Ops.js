.pragma library

.import "Archive.js" as Archive
.import "Convert.js" as Convert
.import "Transfer.js" as Transfer

// The clipboard is entirely client-side: the backend knows about a transfer, never about a pending paste.
function emptyClipboard() {
    return { paths: [], moving: false }
}

// What the status bar and the card are tracking while a transfer runs; id is what a cancel names.
// done, bytes and total are the card's bar: the items already finished, and the one in flight.
function emptyTransfer() {
    return { id: 0, moving: false, n: 0, index: 0, name: "", running: false,
             done: 0, bytes: 0, total: 0 }
}

// "1 item" or "4 items", so no caller builds a plural by hand.
function items(n) {
    return n + (n === 1 ? " item" : " items")
}

// A finished operation names its own reversal, which is why none of them needs a confirmation step.
var UNDO_HINT = " · z undoes"

function started(id, moving, n) {
    return { id: id, moving: moving, n: n, index: 0, name: "", running: true,
             done: 0, bytes: 0, total: 0 }
}

// The count comes from the card's headline so both surfaces name the same progress sample.
function progressLine(t) {
    return t.name.length > 0 ? Transfer.head(t) + " · " + t.name : Transfer.head(t)
}

function transferDone(t, ok, failed, skipped, cancelled) {
    var verb = t.moving ? "Moved " : "Copied "
    var partial = failed > 0 || skipped > 0 || cancelled
    var line = verb + (partial ? ok + " of " + t.n : items(ok))
    if (failed > 0) line += " · " + failed + " failed"
    if (skipped > 0) line += " · " + skipped + " skipped"
    if (cancelled) line += " · cancelled"
    return line + (ok > 0 ? UNDO_HINT : "")
}

function transferFailure(t, name, error) {
    return (t.moving ? "Move" : "Copy") + " failed: " + name + " · " + error
}

// Only the identity-checked locate reply supplies these selected retry matches.
function retrySelectionLine(matches) {
    if (!matches.length) return ""
    return (matches.length === 1 ? leaf(matches[0].path) : items(matches.length)) + " selected for retry"
}

// The canvas draws this one verbatim: "Moved 4 items to Trash · z undoes".
function trashed(ok, failed) {
    if (ok === 0) {
        return failed === 1 ? "That item could not be moved to Trash." : items(failed) + " could not be moved to Trash."
    }
    var line = "Moved " + items(ok) + " to Trash"
    if (failed > 0) {
        line += ", " + failed + " failed"
    }
    return line + UNDO_HINT
}

// The op an undone line carries is the backend's own word for the operation it reversed.
function undone(op) {
    if (op === "trash") {
        return "Put it back from Trash."
    }
    // src/backend/undo.rs reverses a mkdir with remove_dir, and "mkdir" is a wire word the operator
    // never typed, so this one says what left the disk instead.
    if (op === "mkdir") {
        return "Removed the new folder."
    }
    return "Undid the " + op + "."
}

// The created folder's own line, carrying the same reversal hint the transfer and trash lines do.
function made(path) {
    return "Created " + leaf(path) + UNDO_HINT
}

function copied(n, moving) {
    return (moving ? "Cut " : "Copied ") + items(n) + ", p pastes."
}

// Which rows an operation acts on: the selection when there is one, the cursor row otherwise.
function targetIndices(pane) {
    var picked = pane.selectedIndices()
    return picked.length > 0 ? picked : [pane.cursorIndex]
}

// Only rows inside the held window can be named as a path, so the caller sends indices instead and
// lets the backend resolve them; this is the one place that rule is written down on the client.
function targetPaths(pane, indices) {
    var out = []
    for (var i = 0; i < indices.length; i++) {
        var row = pane.rowFor(indices[i])
        if (row) {
            out.push(pane.join(pane.path, row.n))
        }
    }
    return out
}

// The name a progress line shows for an item the client may not hold a row for.
function leaf(path) {
    var cut = String(path).lastIndexOf("/")
    return cut >= 0 ? String(path).substring(cut + 1) : String(path)
}

// ---- the actions, each taking the pane the way Search.js's own do ----

// Duplicate acts on the cursor row alone: the operations design gives it one path, not a batch.
function duplicate(pane, menuId) {
    var row = pane.rowFor(pane.cursorIndex)
    if (!row) {
        return
    }
    pane.backend.duplicate(pane.join(pane.path, row.n), menuId)
}

// No name field: the backend answers with the first free "New Folder", so there is no retry loop.
function newFolder(pane) {
    pane.backend.mkdir(pane.path)
}

// Every view draws the same inline editor: the list and the grid inside the row, the columns view
// over its active column, see ui/ColumnPane.qml's own corner.
function startRename(pane, menuId, index) {
    if (pane.renamePending) return
    // The row the request named, not wherever the cursor has reached by the time the reply lands.
    var at = index !== undefined && index >= 0 ? index : pane.cursorIndex
    var row = pane.rowFor(at)
    if (row) {
        pane.setCursor(at)
        pane.renameError = ""
        pane.renameSource = pane.join(pane.path, row.n)
        pane.renameMenuId = menuId || 0
        pane.renamingIndex = at
    }
}

// Closing before acceptance loses the draft on a refused write; only success or Escape closes it.
function commitRename(pane, newName) {
    if (pane.renamePending) return
    var row = pane.rowFor(pane.renamingIndex)
    if (!row) {
        pane.renameError = "Item changed; reopen Rename."
        return
    }
    if (!newName.length || newName === "." || newName === ".." || newName.indexOf("/") >= 0 || newName.indexOf("\u0000") >= 0) {
        pane.renameError = "A name cannot be empty, . or .., or contain /."
        return
    }
    pane.renameError = ""
    var source = pane.renameSource || pane.join(pane.path, row.n)
    // The request survives a hidden/reused editor so a late reply cannot finish another rename.
    pane.renameRequest = {source: source, destination: source.substring(0, source.lastIndexOf("/") + 1) + newName, folder: pane.path}
    pane.backend.rename(source, newName, pane.renameMenuId || 0)
}

// Indices, not paths: trash acts on the listing that is up right now, so the backend resolves them.
function trash(pane, menuId) {
    var idx = targetIndices(pane)
    if (idx.length === 0) {
        return
    }
    pane.backend.trash(idx, menuId)
}

// The clipboard has to hold absolute paths, because a paste happens in a different directory and the
// listing those indices belonged to is gone by then. The backend resolves them while it still can.
function clip(pane, moving, paths) {
    if (paths) {
        pane.clipboard = {paths: paths, moving: moving}
        pane.message(copied(paths.length, moving), false)
        return
    }
    var idx = targetIndices(pane)
    if (idx.length === 0) {
        return
    }
    pane.clipPending = moving
    pane.backend.askPaths(idx)
}

// The answer to the askPaths above; nothing is on the clipboard until this lands.
function clipResolved(pane, list) {
    if (pane.clipPending === null) {
        return
    }
    var moving = pane.clipPending
    pane.clipPending = null
    pane.clipboard = { paths: list, moving: moving }
    pane.message(copied(list.length, moving), false)
}

// Two kinds of success, said apart: an extract whose archive index could not be read was published
// without being checked against it, and the operator is the one who decides whether to care.
function archiveDoneLine(verified) {
    return verified ? "Archive written."
                    : "Extracted. The archive index could not be read, so this was not verified."
}

function paste(pane) {
    var clip = pane.clipboard
    if (!clip || clip.paths.length === 0) {
        pane.message("There is nothing to paste; y copies and x cuts.", false)
        return
    }
    pane.backend.send({ c: "transfer", op: clip.moving ? "move" : "copy", paths: clip.paths, dest: pane.path })
    // A cut is spent by its paste; a copy stays on the clipboard so it can be pasted again.
    if (clip.moving) {
        pane.clipboard = emptyClipboard()
    }
}

function undo(pane) {
    pane.backend.undo()
}

// Sending the cursor row alone rather than the whole selection is Task 9's own scope; row.d is
// defensive, because the menu already empties its peer list for a directory cursor.
function sendTaildrop(pane, taildrop, peerId, path) {
    if (typeof path !== "string" || path.charAt(0) !== "/") {
        pane.message("Cursor source was not validated; reopen the menu.", true)
        return
    }
    var row = pane.rowFor(pane.cursorIndex)
    if (!row || row.d) {
        return
    }
    if (taildrop.send(peerId, [path]) === false) {
        // The reason is one shared token: the menu, both front ends and the probes all read it, and
        // the TUI draws it as "taildrop · signed out". The bar names the subject the same way rather
        // than printing a bare fragment beside errors that are sentences.
        pane.message(taildrop.reason ? "Taildrop · " + taildrop.reason
                                     : "That Taildrop peer is no longer available.", true)
        return
    }
    // The dispatch is the only result Flea itself ever knows; success or failure is the OEM script's
    // own desktop notification, see the operations design section 4.1.
    pane.message("Sending " + leaf(path) + " to " + taildrop.labelFor(peerId) + ".", false)
}

// ---- archives and convert, whose menu rows are only offered when a tool for them exists ----

// One row compresses under its own name, several under the directory holding them; the name is free
// before the request goes out, and the backend refuses a destination that appeared meanwhile anyway.
function compress(pane, format) {
    var idx = targetIndices(pane)
    if (idx.length === 0) {
        return
    }
    // The archive request names paths and has no rows form, so the indices are resolved first and the
    // request is built in compressResolved. Naming them here would drop every row outside the window.
    pane.pathsPending = { kind: "compress", format: format }
    pane.backend.askPaths(idx)
}

// The answer to the askPaths above, and the only place an archive request is built.
function compressResolved(pane, list, format, menuId) {
    if (list.length === 0) {
        return
    }
    var names = []
    for (var i = 0; i < list.length; i++) {
        names.push(leaf(list[i]))
    }
    var stem = Archive.archiveStem(names, leaf(pane.path))
    pane.backend.compress(list, pane.join(pane.path, stem + "." + format), format, menuId)
    pane.sticky("Compressing " + items(list.length) + " to ." + format)
}

// One paths reply, two possible askers. The clipboard is the default because it is what every reply
// meant before compress joined, so a reply nobody claimed still lands where it always did.
function pathsResolved(pane, list) {
    var pending = pane.pathsPending
    pane.pathsPending = null
    if (pending && pending.kind === "compress") {
        compressResolved(pane, list, pending.format)
        return
    }
    clipResolved(pane, list)
}

// Extract unpacks beside the archive, into a directory named after it.
function extract(pane, menuId) {
    var row = pane.rowFor(pane.cursorIndex)
    if (!row) {
        return
    }
    var path = pane.join(pane.path, row.n)
    pane.backend.extract(path, pane.join(pane.path, Archive.extractDir(row.n)), menuId)
    pane.sticky("Extracting " + row.n)
}

// A directory has nothing to convert, so the popup never opens on one.
function openConvert(pane, menuId) {
    var row = pane.rowFor(pane.cursorIndex)
    if (row && !row.d) {
        pane.convertSource = {path: pane.join(pane.path, row.n), name: leaf(row.n), menuId: menuId || 0}
        pane.convertRequested(row.n)
    }
}

function convert(pane, source, format, strip, requestId) {
    pane.convertSource = Object.assign({}, source, {requestId: requestId})
    pane.sticky("Converting " + source.name + " to ." + format)
    pane.backend.convertImage(source.path, Convert.destination(source, format), strip, source.menuId, requestId, false)
}

// Move to Dropbox is the transfer request with a destination filled in, which is the concrete case
// where "does this need to exist at all" answers no: no new wire, no new Rust.
function moveToDropbox(pane, dropboxPath, menuId) {
    var idx = targetIndices(pane)
    if (idx.length === 0 || dropboxPath.length === 0) {
        return
    }
    // Deliberately does not touch pane.clipboard: this is its own move, and clobbering what the
    // operator cut or copied earlier would lose it with no way back.
    // Rows, not paths: a selection reaches past the window the client holds, and targetPaths drops
    // every index outside it in silence, so a wide move relocated a few files and abandoned the rest.
    pane.backend.send({ c: "transfer", op: "move", rows: idx, dest: dropboxPath, menuId: menuId || 0 })
    pane.sticky("Moving " + items(idx.length) + " to Dropbox")
}
