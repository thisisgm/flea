.pragma library

.import "Archive.js" as Archive
.import "Convert.js" as Convert
.import "CopyAs.js" as CopyAs
.import "Clipboard.js" as Clipboard
.import "ClipSelection.js" as ClipSelection
.import "Filter.js" as Filter
.import "Format.js" as Format
.import "Menu.js" as Menu
.import "Transfer.js" as Transfer
.import "Status.js" as Status
.import "Search.js" as Search

// Whole sentences with "%1" to "%3" for a name, a count or a place, filled by Format.fill so each reads as one.
var WORDS = {
    duplicated: "Duplicated to %1",
    convertedTo: "Converted to %1",
    alreadyExists: "%1 already exists.",
    shelfOne: "Added it to the shelf",
    shelfMany: "Added %1 items to the shelf",
    deleteOne: "This %1 is deleted from disk. This cannot be undone.",
    deleteMany: "These %1 are deleted from disk. This cannot be undone.",
    deleteAllOne: "%1, %2. This deletes it from disk. This cannot be undone.",
    deleteAllMany: "%1, %2. This deletes them from disk. This cannot be undone.",
    moved: "Moved %1",
    movedOf: "Moved %1 of %2",
    copied: "Copied %1",
    copiedOf: "Copied %1 of %2",
    cut: "Cut %1",
    failedNote: " · %1 failed",
    cancelledNote: " · cancelled",
    skippedNote: " · %1 skipped",
    moveFailed: "Move failed: %1 · %2",
    copyFailed: "Copy failed: %1 · %2",
    linked: "Linked %1",
    selectedForRetry: "%1 selected for retry",
    trashFailedOne: "That item could not be moved to Trash.",
    trashFailedOneWhy: "That item could not be moved to Trash: %1.",
    trashFailedMany: "%1 could not be moved to Trash.",
    trashFailedManyWhy: "%1 could not be moved to Trash: %2.",
    trashedAll: "Moved %1 to Trash",
    trashedSome: "Moved %1 to Trash, %2 failed",
    trashedSomeWhy: "Moved %1 to Trash, %2 failed: %3",
    created: "Created %1",
    taildropReason: "Taildrop · %1",
    compressing: "Compressing %1 to .%2",
    converting: "Converting %1 to .%2",
    undoNothing: "Undid the last action"
}

// What a journal word undoes and redoes, as whole sentences: a word the backend uses for itself is never shown.
var UNDONE = {
    copy: "Undid the copy", move: "Undid the move", rename: "Undid the rename", merge: "Undid the merge",
    permissions: "Undid the permissions", duplicate: "Undid the duplicate", link: "Undid the link",
    bulkrename: "Undid the bulk rename", newfile: "Removed the new file", paste: "Undid the paste", cut: "Undid the cut"
}
var REDONE = {
    copy: "Redid the copy", move: "Redid the move", rename: "Redid the rename", merge: "Redid the merge",
    permissions: "Redid the permissions", duplicate: "Redid the duplicate", link: "Redid the link",
    bulkrename: "Redid the bulk rename", newfile: "Redid the new file", paste: "Redid the paste", cut: "Redid the cut"
}

function emptyClipboard() {
    return Clipboard.empty()
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
    return n === 1 ? Format.fill(WORDS.deleteOne, scopeSingle(scope)) : Format.fill(WORDS.deleteMany, scope)
}

// The Empty Trash body, two whole sentences with the row's own count and formatted size; this module never formats a size itself.
function deleteAllLine(n, sizeText) {
    return Format.fill(n === 1 ? WORDS.deleteAllOne : WORDS.deleteAllMany, items(n), sizeText)
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
    var partial = failed > 0 || skipped > 0 || cancelled
    var line = partial
        ? Format.fill(t.moving ? WORDS.movedOf : WORDS.copiedOf, Format.count(ok), Format.count(t.n))
        : Format.fill(t.moving ? WORDS.moved : WORDS.copied, items(ok))
    if (failed > 0) line += Format.fill(WORDS.failedNote, Format.count(failed))
    if (skipped > 0) line += Format.fill(WORDS.skippedNote, Format.count(skipped))
    if (cancelled) line += WORDS.cancelledNote
    // The durable verdict's own note rides ahead of the undo hint, so the strip draws
    // "Copied N items" with "<note> · z undoes" beside it. Only what landed is said:
    // a verdict note with nothing copied stays unsaid.
    if (ok > 0 && String(note || "").length > 0) line += Format.fill(Status.HINTS.note, note)
    return ok > 0 ? Status.withUndo(line) : line
}

function transferFailure(t, name, error) {
    return Format.fill(t.moving ? WORDS.moveFailed : WORDS.copyFailed, name, error)
}

// MenuAdditions040: Paste as links answers one line per request, and one
// journal entry, so one undo removes every link it created.
function linkedLine(ok, failed, skipped, note) {
    var line = Format.fill(WORDS.linked, items(ok))
    if (failed > 0) line += Format.fill(WORDS.failedNote, Format.count(failed))
    if (skipped > 0) line += Format.fill(WORDS.skippedNote, Format.count(skipped))
    // A stranded replace rides ahead of the undo hint, so the counts keep their shape.
    if (String(note || "").length > 0) line += Format.fill(Status.HINTS.note, note)
    return ok > 0 ? Status.withUndo(line) : line
}

// Only the identity-checked locate reply supplies these selected retry matches.
function retrySelectionLine(matches) {
    if (!matches.length) return ""
    return Format.fill(WORDS.selectedForRetry, matches.length === 1 ? leaf(matches[0].path) : items(matches.length))
}

// The canvas draws this one verbatim: "Moved 4 items to Trash · z undoes".
// A failure names its reason, so a hung mount or a denial reads as what happened rather than silence.
function trashed(ok, failed, reason) {
    var why = String(reason || "")
    if (ok === 0) {
        if (failed === 1)
            return why.length > 0 ? Format.fill(WORDS.trashFailedOneWhy, why) : WORDS.trashFailedOne
        return why.length > 0 ? Format.fill(WORDS.trashFailedManyWhy, items(failed), why) : Format.fill(WORDS.trashFailedMany, items(failed))
    }
    if (failed === 0)
        return Format.fill(WORDS.trashedAll, items(ok)) + Status.undoHint()
    return (why.length > 0 ? Format.fill(WORDS.trashedSomeWhy, items(ok), Format.count(failed), why)
                          : Format.fill(WORDS.trashedSome, items(ok), Format.count(failed))) + Status.undoHint()
}

// The op an undone line carries is the backend's own word for the operation it reversed.
function undone(op) {
    if (op === "trash")
        return "Put it back from Trash."
    if (op === "settags")
        return "Restored the previous tags."
    // src/backend/undo.rs reverses a mkdir with remove_dir, and "mkdir" is a wire word the operator
    // never typed, so this one says what left the disk instead.
    if (op === "mkdir")
        return "Removed the new folder."
    return UNDONE[op] !== undefined ? UNDONE[op] : WORDS.undoNothing
}

// The redo line for a journal word, before the undo hint the caller adds.
function redone(op) {
    return REDONE[op] !== undefined ? REDONE[op] : "Redid the last action"
}

// The line for a new file or folder, which the status line draws beside its reversal hint when it has one.
function createdLine(path) {
    return Format.fill(WORDS.created, leaf(path))
}

// The created folder's own line, carrying the same reversal hint the transfer and trash lines do.
function made(path) {
    return Status.withUndo(createdLine(path))
}

function copied(n, moving) {
    return Status.withPaste(Format.fill(moving ? WORDS.cut : WORDS.copied, items(n)))
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

// The marked indices in listing order, which makes the first the pair's file a and the second its file b.
function markedIndices(pane) {
    return targetIndices(pane).slice().sort(function (a, b) { return a - b })
}

function markedRows(pane) {
    return markedIndices(pane).map(function (index) { return pane.rowFor(index) })
}

// Each row's own type bits; a row carries no target's type, so a link stays a link and Menu.canCompare leaves it out.
function kinds(rowList) {
    return rowList.map(function (row) { return row ? Number(row.p) || 0 : 0 })
}

// A ready matching snapshot wins over held modes that may predate an external replacement.
function menuKinds(pane) {
    var picked = markedRows(pane)
    var snapshot = pane.menuActions
    return snapshot && snapshot.ready && snapshot.identity === pane.menuSelectionIdentity()
        && snapshot.selectedKinds.length === picked.length ? snapshot.selectedKinds : kinds(picked)
}

// The choice uses the menu's validated paths, reordered like the marked indices; unvalidated choices can only use held rows.
function open(pane, paths) {
    var indices = markedIndices(pane)
    var picked = indices.map(function (index) { return pane.rowFor(index) })
    if (!Menu.canCompare(menuKinds(pane))) return
    if (paths && paths.length === indices.length) {
        var original = targetIndices(pane)
        paths = indices.map(function (index) { return paths[original.indexOf(index)] })
    } else {
        paths = targetPaths(pane, indices)
    }
    if (paths.length !== 2 || paths.some(function (path) { return typeof path !== "string" || path.charAt(0) !== "/" })) return
    pane.quickLook().openCompare(paths[0], paths[1], picked[0] ? picked[0].i : "text-x-generic")
}

// Only rows inside the held window can be named as a path, so the caller sends indices instead and
// lets the backend resolve them; this is the one place that rule is written down on the client.
function targetPaths(pane, indices) {
    var out = []
    for (var i = 0; i < indices.length; i++) {
        var row = pane.rowFor(indices[i])
        if (row) {
            out.push(pane.join(Search.baseOf(pane), row.n))
        }
    }
    return out
}

// The name a progress line shows for an item the client may not hold a row for.
function leaf(path) {
    var cut = String(path).lastIndexOf("/")
    return cut >= 0 ? String(path).substring(cut + 1) : String(path)
}

// The single target the menu may name as a zip: the cursor or the one selection, "" otherwise.
function zipTarget(pane) {
    // The probe fires on listing signals that also run before the first listing binds shown; with it unbound there is no target.
    if (pane.shown === undefined) return ""
    var indices = targetIndices(pane)
    if (indices.length !== 1) return ""
    var row = pane.rowFor(indices[0])
    if (!row || row.d || Archive.imageKind(row.n) !== "zip") return ""
    return pane.join(Search.baseOf(pane), row.n)
}

// A zip rewritten in place keeps its name, so the ask key carries its size and mtime too.
function zipSig(pane) {
    var indices = targetIndices(pane)
    if (indices.length !== 1) return ""
    var row = pane.rowFor(indices[0])
    return row ? String(row.s) + "|" + String(row.m) : ""
}

// Asks the backend once per target zip; a rewritten zip asks again, a member naming another file clears at once.
function checkZip(pane) {
    if (!pane.backend) return
    var target = zipTarget(pane)
    var key = target ? target + "|" + zipSig(pane) : "|"
    if (key === pane.flashZipAsked) return
    pane.flashZipAsked = key
    pane.flashZipMember = ""
    if (target.length > 0) pane.backend.flashImage(target)
}

// Only the reply for the current target lands, and never into an open menu: the next opening draws it.
function noteZip(pane, message) {
    var want = zipTarget(pane)
    if (!want || !message || message.path !== want) return
    var member = message.member || ""
    pane.flashZipMember = message.kind === "zip" && Archive.isDiskImage(leaf(want), member) ? member : ""
}

// The pane's own delete verdict, which ui/PaneMenuActions.qml draws when a menu delete lands.
function deletedLine(message) {
    var text = doneOf("Deleted", message.deleted, message.count)
    if (message.failed) text += " · " + Format.count(message.failed) + " failed"
    if (message.cancelled) text += " · cancelled"
    return text
}

// Bulk rename answers one line for the whole batch, as whole-sentence templates.
var BULK_DONE_ONE = "Renamed 1 item%1"
var BULK_DONE_MANY = "Renamed %1 items%2"
function bulkDoneLine(ok) {
    if (ok === 1) return BULK_DONE_ONE.replace("%1", Status.undoHint())
    return BULK_DONE_MANY.replace("%1", Format.count(ok)).replace("%2", Status.undoHint())
}

// ---- the actions, each taking the pane the way Search.js's own do ----

// Duplicate acts on the cursor row alone: the operations design gives it one path, not a batch.
function duplicate(pane, menuId) {
    var row = pane.rowFor(pane.cursorIndex)
    if (!row) {
        return
    }
    pane.backend.duplicate(pane.join(Search.baseOf(pane), row.n), menuId)
}

// No name field: the backend answers with the first free "New Folder", so there is no retry loop.
function newFolder(pane) {
    pane.backend.mkdir(pane.path)
}

// A ready snapshot over the unchanged identity opens at once, one still in flight takes the F2 route, a moved identity is stale.
function menuRenameRoute(ready, identity, currentIdentity) {
    if ((identity || "") === "" || identity !== currentIdentity) return "stale"
    return ready === true ? "now" : "f2"
}

// No live editor sends the refusal to the status bar and closes the edit.
function refuseRename(pane, reason) {
    if (pane.renamingIndex >= 0 && pane.renameEditor() !== null) {
        pane.renameError = reason
        return "editor"
    }
    pane.message(reason, true)
    if (pane.renamingIndex >= 0)
        pane.renamingIndex = -1
    return "status"
}

// One inline editor in every view; a pointer rename passes context 0 so the list never moves under it.
function startRename(pane, menuId, index, context) {
    if (pane.renamePending) return
    // The row the request named, not wherever the cursor has reached by the time the reply lands.
    var named = index !== undefined && index >= 0
    var at = named ? index : pane.cursorIndex
    var row = pane.rowFor(at)
    if (!row) return
    // A hidden row has no delegate to draw the editor in, so it would open on the filter clearing.
    if (!named && !Filter.cursorShown(pane)) return sayNoTarget(pane)
    pane.setCursor(at, context)
    pane.renameError = ""
    pane.renameSource = pane.join(Search.baseOf(pane), row.n)
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
    var source = pane.renameSource || pane.join(Search.baseOf(pane), row.n)
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

// One line while a paths round trip is out; every asker refuses through it, so none steals another's reply.
function pathsBusy(pane) {
    pane.message("Still resolving the last selection; try again.", false)
}

function refuseWhileBusy(pane) {
    if (!(pane.pathsPending || pane.clipPending !== null)) return false
    pathsBusy(pane)
    return true
}

// Resolve absolute paths with each request's own verb, deferring the latest overlapping selection.
function clip(pane, moving, paths) {
    if (!paths && pane.pathsPending) { pathsBusy(pane); return }
    var indices = paths ? [] : targetIndices(pane)
    if (!paths && indices.length === 0) return sayNoTarget(pane)
    ClipSelection.take(pane, moving, paths, indices, copied)
}

function clipFailed(pane) {
    ClipSelection.failed(pane)
}

// MenuAdditions040: Path stays absolute; Relative reads pane.path, kept on the claim, and Recent (no folder) refuses it.
var RECENT_RELATIVE = "This listing is a history, and has no folder to copy a relative path from."
function copyAs(pane, kind, paths) {
    if (kind === "relative" && pane.recentMode && pane.recentMode.length > 0) { pane.message(RECENT_RELATIVE, false); return }
    if (paths && paths.length > 0) {
        pane.opener.copyText(CopyAs.lines(paths, kind, pane.path))
        return
    }
    if (refuseWhileBusy(pane)) return
    var idx = targetIndices(pane)
    if (idx.length === 0) return sayNoTarget(pane)
    pane.pathsPending = { kind: "copyAs", format: kind, base: pane.path }
    pane.backend.askPaths(idx)
}

// Two kinds of success, said apart: an extract whose archive index could not be read was published
// without being checked against it, and the operator is the one who decides whether to care.
function archiveDoneLine(verified) {
    return verified ? "Archive written."
                    : "Extracted. The archive index could not be read, so this was not verified."
}

function paste(pane, forceMove) { Clipboard.paste(pane, "", forceMove === true) }
function pasteLink(pane, kind) { Clipboard.paste(pane, kind, false) }

function undo(pane) {
    pane.backend.undo()
}

// Sending the cursor row alone rather than the whole selection is Task 9's own scope; row.d is
// defensive, because the menu already empties its peer list for a directory cursor.
function sendTaildrop(pane, taildrop, peerId, path) {
    if (typeof path !== "string" || path.charAt(0) !== "/") {
        pane.message("That menu is out of date. Reopen it.", true)
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
        pane.message(taildrop.reason ? Format.fill(WORDS.taildropReason, taildrop.reason)
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
    if (idx.length === 0) return sayNoTarget(pane)
    if (refuseWhileBusy(pane)) return
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
    pane.backend.compress(list, pane.join(Search.baseOf(pane), stem + "." + format), format, menuId)
    pane.sticky(Format.fill(WORDS.compressing, items(list.length), format))
}

// One paths reply, two possible askers. The clipboard is the default because it is what every reply
// meant before compress joined, so a reply nobody claimed still lands where it always did.
function pathsResolved(pane, list) {
    var pending = pane.pathsPending
    pane.pathsPending = null
    // A pending carrying its own deliver answers itself, the drag's pattern the password flow reuses.
    if (pending && typeof pending.deliver === "function") { pending.deliver(list, pending); return }
    if (pending && pending.kind === "compress") {
        compressResolved(pane, list, pending.format)
        return
    }
    // MenuAdditions040: every Copy as variant covers the whole selection, one
    // path per line, through wl-copy; nothing reaches the file clipboard.
    if (pending && pending.kind === "copyAs") {
        if (list.length > 0)
            pane.opener.copyText(CopyAs.lines(list, pending.format, pending.base))
        return
    }
    // MenuAdditions040: Permissions takes the whole selection, so the dialog
    // opens over every resolved path rather than the cursor row alone.
    if (pending && pending.kind === "permissions") {
        pane.openPermissionsWith(list)
        return
    }
    // Bulk rename takes the whole selection for its card.
    if (pending && pending.kind === "bulkrename") {
        pane.openBulkRenameWith(list)
        return
    }
    ClipSelection.resolved(pane, list, copied)
}

// Extract unpacks beside the archive; its sticky and card come off the wire's own start line.
function extract(pane, menuId) {
    var row = pane.rowFor(pane.cursorIndex)
    if (!row) {
        return
    }
    pane.backend.extract(pane.join(Search.baseOf(pane), row.n), pane.join(Search.baseOf(pane), Archive.extractDir(row.n)), menuId)
}

// A directory has nothing to convert, so the popup never opens on one.
function openConvert(pane, menuId) {
    var row = pane.rowFor(pane.cursorIndex)
    if (row && !row.d) {
        pane.convertSource = {path: pane.join(Search.baseOf(pane), row.n), name: leaf(row.n), menuId: menuId || 0}
        pane.convertRequested(row.n)
    }
}

function convert(pane, source, format, strip, requestId) {
    pane.convertSource = Object.assign({}, source, {requestId: requestId})
    pane.sticky(Format.fill(WORDS.converting, source.name, format))
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
    pane.asks().ask({ c: "transfer", op: "move", rows: idx, dest: dropboxPath, menuId: menuId || 0 })
}
