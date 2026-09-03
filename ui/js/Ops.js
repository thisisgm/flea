.pragma library

.import "Archive.js" as Archive
.import "Convert.js" as Convert
.import "Transfer.js" as Transfer
.import "Remote.js" as Remote
// The clipboard is entirely client-side: the backend knows about a transfer, never about a pending paste.
function emptyClipboard() {
    return { paths: [], moving: false }
}
// What the status bar and the card are tracking while a transfer runs; id is what a cancel names.
// done, bytes and total are the card's bar: the items already finished, and the one in flight.
function emptyTransfer() {
    return { id: 0, moving: false, n: 0, index: 0, name: "", running: false,
             done: 0, bytes: 0, total: 0, kind: "local" }
}

// "1 item" or "4 items", so no caller builds a plural by hand.
function items(n) {
    return n + (n === 1 ? " item" : " items")
}

// A finished operation names its own reversal, which is why none of them needs a confirmation step.
var UNDO_HINT = " · z undoes"

function started(id, moving, n, kind) {
    return { id: id, moving: moving, n: n, index: 0, name: "", running: true,
             done: 0, bytes: 0, total: 0, kind: kind || "local" }
}

// The canvas's own line: "Copying 2 of 5, photo.heic". The count comes from the card's own
// headline so the bar and the card can never word the same operation two different ways.
function progressLine(t) {
    var head = Transfer.head(t)
    if (t.kind === "remote-to-remote")
        head = Remote.transferPrefix(t.kind, t.moving) + " " + (t.index + 1) + " of " + t.n
    return t.name.length > 0 ? head + ", " + t.name : head
}
function transferDone(t, ok, failed, cancelled) {
    if (cancelled) {
        return ok > 0 ? "Cancelled after " + items(ok) + UNDO_HINT : "Cancelled."
    }
    var verb = t.moving ? "Moved " : "Copied "
    if (failed > 0) {
        var line = verb + items(ok) + ", " + failed + " failed"
        return ok > 0 ? line + UNDO_HINT : line
    }
    return verb + items(ok) + UNDO_HINT
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
function duplicate(pane) {
    var row = pane.rowFor(pane.cursorIndex)
    if (!row) {
        return
    }
    pane.backend.duplicate(pane.join(pane.path, row.n))
}

// No name field: the backend answers with the first free "New Folder", so there is no retry loop.
function newFolder(pane) {
    pane.backend.mkdir(pane.path)
}

// r on a row the client holds opens the editor over that row's name column. Only ui/Row.qml draws
// one, so a rename started in the grid or the columns would set ui/js/Focus.js's guard with nothing
// left to ever clear it, and every later key would be swallowed for the life of the window.
function startRename(pane) {
    if (pane.viewMode !== "list") {
        pane.message("Rename needs the list view.", false)
        return
    }
    if (pane.rowFor(pane.cursorIndex)) {
        pane.renamingIndex = pane.cursorIndex
    }
}

// The row is read before the index is cleared, because clearing it is what closes the editor.
function commitRename(pane, newName) {
    var row = pane.rowFor(pane.renamingIndex)
    pane.renamingIndex = -1
    if (row) {
        pane.backend.rename(pane.join(pane.path, row.n), newName)
    }
}

// Indices, not paths: trash acts on the listing that is up right now, so the backend resolves them.
function trash(pane) {
    var idx = targetIndices(pane)
    if (idx.length === 0) {
        return
    }
    pane.backend.trash(idx)
}

// The clipboard has to hold absolute paths, because a paste happens in a different directory and the
// listing those indices belonged to is gone by then. The backend resolves them while it still can.
function clip(pane, moving) {
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
    pane.pendingTransferKind = Remote.transferKind(clip.paths, pane.path)
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
function sendTaildrop(pane, taildrop, peerId) {
    var row = pane.rowFor(pane.cursorIndex)
    if (!row || row.d) {
        return
    }
    taildrop.send(peerId, [pane.join(pane.path, row.n)])
    // The dispatch is the only result Flea itself ever knows; success or failure is the OEM script's
    // own desktop notification, see the operations design section 4.1.
    pane.message("Sending " + row.n + " to " + taildrop.labelFor(peerId) + ".", false)
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
function compressResolved(pane, list, format) {
    if (list.length === 0) {
        return
    }
    var names = []
    for (var i = 0; i < list.length; i++) {
        names.push(leaf(list[i]))
    }
    var stem = Archive.archiveStem(names, leaf(pane.path))
    pane.backend.compress(list, pane.join(pane.path, stem + "." + format), format)
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
function extract(pane) {
    var row = pane.rowFor(pane.cursorIndex)
    if (!row) {
        return
    }
    var path = pane.join(pane.path, row.n)
    pane.backend.extract(path, pane.join(pane.path, Archive.extractDir(row.n)))
    pane.sticky("Extracting " + row.n)
}

// A directory has nothing to convert, so the popup never opens on one.
function openConvert(pane) {
    var row = pane.rowFor(pane.cursorIndex)
    if (row && !row.d) {
        pane.convertRequested(row.n)
    }
}

function convert(pane, format, strip) {
    var row = pane.rowFor(pane.cursorIndex)
    if (!row) {
        return
    }
    pane.backend.convertImage(pane.join(pane.path, row.n),
                              pane.join(pane.path, Convert.destName(row.n, format)), strip)
    pane.sticky("Converting " + row.n + " to ." + format)
}

// Move to Dropbox is the transfer request with a destination filled in, which is the concrete case
// where "does this need to exist at all" answers no: no new wire, no new Rust.
function moveToDropbox(pane, dropboxPath) {
    var idx = targetIndices(pane)
    if (idx.length === 0 || dropboxPath.length === 0) {
        return
    }
    // Deliberately does not touch pane.clipboard: this is its own move, and clobbering what the
    // operator cut or copied earlier would lose it with no way back.
    // Rows, not paths: a selection reaches past the window the client holds, and targetPaths drops
    // every index outside it in silence, so a wide move relocated a few files and abandoned the rest.
    pane.backend.send({ c: "transfer", op: "move", rows: idx, dest: dropboxPath })
    pane.sticky("Moving " + items(idx.length) + " to Dropbox")
}
