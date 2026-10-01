.pragma library

.import "Archive.js" as Archive
.import "Convert.js" as Convert
.import "Filter.js" as Filter
.import "Format.js" as Format
.import "Transfer.js" as Transfer
.import "Status.js" as Status

// The clipboard is entirely client-side: the backend knows about a transfer, never about a pending paste.
function emptyClipboard() {
    return { paths: [], moving: false }
}

// What the status bar and the card are tracking while a transfer runs; id is what a cancel names.
// done, bytes and total are the card's bar, the items finished and the one in flight; moved is ui/js/Transfer.js's own sum of the finished ones, which the wire never carries.
function emptyTransfer() {
    return { id: 0, moving: false, n: 0, index: 0, name: "", running: false,
             done: 0, bytes: 0, total: 0, moved: 0, writing: false, drive: "" }
}

// Any counted noun, so callers with their own word never hand-build the plural.
function pluralWord(n, one, many) {
    return n === 1 ? one : many
}

// "item" or "items", the noun alone for a line that already carries its count.
function itemWord(n) {
    return pluralWord(n, "item", "items")
}

// "1 item" or "1,204 items", so no caller builds a plural by hand.
function items(n) {
    return Format.count(n) + " " + itemWord(n)
}

// The delete confirm names its scope ("Trash items" in the Trash view, "items" on a menu
// delete), singular for one and plural otherwise, so no caller keeps its own "items" constant.
function scopeSingle(scope) {
    return String(scope).replace("items", "item")
}
function deleteScopeLine(scope, n) {
    return n === 1 ? "This " + scopeSingle(scope) + " is deleted from disk. This cannot be undone."
                   : "These " + scope + " are deleted from disk. This cannot be undone."
}

// The Empty Trash body deletes "it" for one and "them" otherwise; sizeText and undoHint are the
// row's own formatted size and key hint, which this module never formats itself.
function deleteAllLine(n, sizeText, undoHint) {
    return items(n) + ", " + sizeText + ". This deletes " + pluralWord(n, "it", "them")
        + " from disk. " + undoHint + " cannot undo it and the undo journal does not cover it."
}

// "Deleted 1 of 1 item" or "Restored 0 of 2 items": the Trash view and the menu delete status.
function doneOf(verb, done, total) {
    return verb + " " + Format.count(done) + " of " + Format.count(total) + " " + itemWord(total)
}

// extract is the one verb not derived from moving: an extract drives this same card and its Cancel.
function started(id, moving, n, extract) {
    return { id: id, moving: moving, n: n, index: 0, name: "", running: true, extract: extract === true,
             done: 0, bytes: 0, total: 0, moved: 0, writing: false, drive: "" }
}

// The count comes from the card's headline so both surfaces name the same progress sample.
function progressLine(t) {
    return t.name.length > 0 ? Transfer.head(t) + " · " + t.name : Transfer.head(t)
}

function transferDone(t, ok, failed, skipped, cancelled, durable, note) {
    var verb = t.moving ? "Moved " : "Copied "
    var partial = failed > 0 || skipped > 0 || cancelled
    var line = verb + (partial ? Format.count(ok) + " of " + Format.count(t.n) : items(ok))
    if (failed > 0) line += " · " + Format.count(failed) + " failed"
    if (skipped > 0) line += " · " + Format.count(skipped) + " skipped"
    if (cancelled) line += " · cancelled"
    // The durable verdict's own note rides ahead of the undo hint, so the strip draws
    // "Copied N items" with "<note> · z undoes" beside it. Only what landed is said:
    // a verdict note with nothing copied stays unsaid.
    if (ok > 0 && String(note || "").length > 0) line += " · " + note
    return line + (ok > 0 ? Status.UNDO_HINT : "")
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
    if (ok === 0)
        return failed === 1 ? "That item could not be moved to Trash." : items(failed) + " could not be moved to Trash."
    var line = "Moved " + items(ok) + " to Trash"
    if (failed > 0)
        line += ", " + Format.count(failed) + " failed"
    return line + Status.UNDO_HINT
}

// The op an undone line carries is the backend's own word for the operation it reversed.
function undone(op) {
    if (op === "trash")
        return "Put it back from Trash."
    // src/backend/undo.rs reverses a mkdir with remove_dir, and "mkdir" is a wire word the operator
    // never typed, so this one says what left the disk instead.
    if (op === "mkdir")
        return "Removed the new folder."
    return "Undid the " + op + "."
}

// The created folder's own line, carrying the same reversal hint the transfer and trash lines do.
function made(path) {
    return "Created " + leaf(path) + Status.UNDO_HINT
}

function copied(n, moving) {
    return (moving ? "Cut " : "Copied ") + items(n) + Status.PASTE_HINT
}

// The three reasons a cursor is not a target, asked in the order ui/js/Nav.js asks them of Enter.
function sayNoTarget(pane) {
    if (pane.cursorIndex < 0) return pane.message("There is nothing to act on.", false)
    if (!pane.rowFor(pane.cursorIndex)) return pane.message("That row has not loaded yet.", false)
    pane.message("That row is hidden by the filter.", false)
}

// The cursor is a target only while the filter draws it, the rule prune already applies to a selection.
function targetIndices(pane) {
    var picked = pane.selectedIndices()
    return picked.length > 0 ? picked : (Filter.cursorShown(pane) ? [pane.cursorIndex] : [])
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

// The pane's own delete verdict, which ui/PaneMenuActions.qml draws when a menu delete lands.
function deletedLine(message) {
    var text = doneOf("Deleted", message.deleted, message.count)
    if (message.failed) text += " · " + Format.count(message.failed) + " failed"
    if (message.cancelled) text += " · cancelled"
    return text
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
    if (pane.path === "flea:stack") { pane.message("The Stack is a list of files, not a folder.", false); return } pane.backend.mkdir(pane.path)
}

// Every view draws the same inline editor: the list and the grid inside the row, the columns view
// over its active column, see ui/ColumnPane.qml's own corner.
function startRename(pane, menuId, index) {
    if (pane.renamePending) return
    // The row the request named, not wherever the cursor has reached by the time the reply lands.
    var named = index !== undefined && index >= 0
    var at = named ? index : pane.cursorIndex
    var row = pane.rowFor(at)
    if (!row) return
    // A hidden row has no delegate to draw the editor in, so it would open on the filter clearing.
    if (!named && !Filter.cursorShown(pane)) return sayNoTarget(pane)
    pane.setCursor(at)
    pane.renameError = ""
    pane.renameSource = pane.join(pane.path, row.n)
    pane.renameMenuId = menuId || 0
    pane.renamingIndex = at
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
    if (idx.length === 0) return sayNoTarget(pane)
    // Sorted ascending, so this is the block's own first row and not wherever the cursor sat in it.
    pane.trashedFirst = idx[0]
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
    if (idx.length === 0) return sayNoTarget(pane)
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
    if (pane.path === "flea:stack" || !clip || clip.paths.length === 0) {
        pane.message(pane.path === "flea:stack" ? "The Stack is a list of files, not a folder." : "There is nothing to paste; y copies and x cuts.", false)
        return
    }
    // A cut is spent once its paste goes out, see ui/CollideHost.qml; a copy stays so it can be pasted again.
    pane.collide.ask({ c: "transfer", op: clip.moving ? "move" : "copy", paths: clip.paths, dest: pane.path }, null, clip.moving)
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
    if (pane.path === "flea:stack") { pane.message("The Stack is a list of files, not a folder.", false); return } if (idx.length === 0) return sayNoTarget(pane)
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

// Extract unpacks beside the archive; its sticky and card come off the wire's own start line.
function extract(pane, menuId) {
    var row = pane.rowFor(pane.cursorIndex)
    if (!row) {
        return
    }
    var path = pane.join(pane.path, row.n)
    pane.backend.extract(path, pane.join(pane.path, Archive.extractDir(row.n)), menuId)
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
    if (dropboxPath.length === 0) return
    if (idx.length === 0) return sayNoTarget(pane)
    // Deliberately does not touch pane.clipboard: this is its own move, and clobbering what the
    // operator cut or copied earlier would lose it with no way back.
    // Rows, not paths: a selection reaches past the window the client holds, and targetPaths drops
    // every index outside it in silence, so a wide move relocated a few files and abandoned the rest.
    pane.collide.ask({ c: "transfer", op: "move", rows: idx, dest: dropboxPath, menuId: menuId || 0 })
}
