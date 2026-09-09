.pragma library

.import "Errors.js" as Errors
.import "Ops.js" as Ops
.import "Picker.js" as Picker
.import "PickerOps.js" as PickerOps
.import "Transfer.js" as Transfer

// What each operation reply does to the chooser's state: ui/PaneWire.qml's handlers, written
// against ui/PickerState.qml and called one to one from ui/PickerWire.qml, so tests/js/pickerwire.js
// runs every reply without a window. The lines come from ui/js/Ops.js's own builders, so the
// chooser and the browser can never word the same operation two different ways.

// A trashed line carries counts and no paths, so the paths it took are read off the state before
// the refresh replaces the rows: the same set PickerOps.trash sent, the marks held here or the
// cursor row alone. corner: a batch that half failed loses the marks on the rows that stayed too;
// the line says how many failed, and a row that stayed can be marked again.
function trashed(state, ok, failed) {
    var paths = state.trashPending
    state.trashPending = []
    state.sticky("")
    state.message(Ops.trashed(ok, failed), ok === 0)
    if (ok > 0) {
        PickerOps.dropMarks(state, paths)
    }
    PickerOps.refresh(state, "")
}

function renamed(state, ok, path) {
    var source = state.renameFromPath
    var listing = state.renameListingPath
    var pointer = state.renamePointerPath
    if (ok)
        PickerOps.renameMark(state, source, path)
    state.renameFromPath = ""
    state.renameListingPath = ""
    state.renamePointerPath = ""
    // The reply belongs to the listing that sent it. This also names Recent, whose token is not the
    // parent of any source row. A later navigation must not re-read its new location.
    if (!source || listing !== state.path)
        return
    PickerOps.refresh(state, pointer && pointer !== source ? pointer : path)
}

// The new folder has no row until the refresh lands, so armRename opens the editor on the rows
// reply that carries it rather than here. The path is remembered whole, never as an index.
function made(state, ok, path) {
    var listing = state.mkdirListingPath
    state.mkdirListingPath = ""
    if (!listing || listing !== state.path)
        return
    state.renameOnArrival = path
    state.message(Ops.made(path), false)
    PickerOps.refresh(state, path)
}

function duplicated(state, ok, path) {
    state.message("Duplicated to " + Ops.leaf(path) + Ops.UNDO_HINT, false)
    PickerOps.refresh(state, path)
}

function undone(state, op, ok) {
    state.sticky("")
    state.message(Ops.undone(op), false)
    PickerOps.refresh(state, "")
}

// The verb comes off the wire, never off the clipboard: a paste spends a cut before this arrives.
function transferStarted(state, id, n, moving) {
    state.transfer = Ops.started(id, moving, n)
    state.sticky(Ops.progressLine(state.transfer))
}

// Reassigned rather than mutated in place, so the footer's binding sees every sample.
function transferProgress(state, id, index, name, bytes, total) {
    if (id !== state.transfer.id) {
        return
    }
    state.transfer = Transfer.sampled(state.transfer, index, name, bytes, total)
    state.sticky(Ops.progressLine(state.transfer))
}

function transferItem(state, id, index, name) {
    if (id !== state.transfer.id) {
        return
    }
    state.transfer = Transfer.itemDone(state.transfer, index, name)
    state.sticky(Ops.progressLine(state.transfer))
}

function transferDone(state, id, ok, failed, skipped, cancelled) {
    if (id !== state.transfer.id) {
        return
    }
    var line = Ops.transferDone(state.transfer, ok, failed, cancelled)
    state.transfer = Ops.emptyTransfer()
    state.sticky("")
    state.message(line, failed > 0 && ok === 0)
    PickerOps.refresh(state, "")
}

// Run on every rows reply, after the window has seated the cursor. The editor opens only when the
// cursor really landed on the folder that was made: on a listing wider than the window the refresh
// may not hold that row at all, and an editor armed on the wrong row would rename another file.
// The first reply spends the path either way, so no later listing can arm it.
function armRename(state) {
    if (state.renameOnArrival.length === 0) {
        return
    }
    var target = state.renameOnArrival
    state.renameOnArrival = ""
    var row = state.rowFor(state.cursorIndex)
    if (row && Picker.rowPath(state.path, row.n) === target) {
        state.renamingIndex = state.cursorIndex
    }
}

// The listing failures keep the wording the chooser has always shown, the backend's own line, and
// blank the listing the way they always did. The operations, new here, take ui/js/Errors.js's
// sentences and leave the listing standing: a refused rename changes no row.
function failed(state, where, msg) {
    var renameListing = state.renameListingPath
    if (where === "rename" || where === "rename-kept") {
        state.renameFromPath = ""
        state.renameListingPath = ""
        state.renamePointerPath = ""
    }
    // A refused sort changes nothing in the backend, so it changes nothing here: a plain notice.
    if (where === "sort") {
        state.message(Errors.sentence(where, msg), false)
        return
    }
    var terminal = where === "backend" || where === "read"
    if (where === "mkdir" || terminal)
        state.mkdirListingPath = ""
    var listing = terminal || where === "scan"
    if (where === "trash" || terminal) {
        state.trashPending = []
    }
    // Only these mean the refresh will never deliver rows, so an armed editor would wait forever.
    if (listing) {
        state.renameOnArrival = ""
    }
    // No transferdone is coming from a backend that is gone, and nothing else ends a transfer.
    if (terminal) {
        state.transfer = Ops.emptyTransfer()
        state.sticky("")
    }
    if (listing || state.listingState === "loading") {
        state.listingState = "empty"
    }
    state.message(listing ? msg : Errors.sentence(where, msg), true)
    // The copy is whole and only the name it came from is unknown: re-read and select nothing.
    if (where === "rename-kept" && renameListing === state.path) {
        PickerOps.refresh(state, "")
    }
}
