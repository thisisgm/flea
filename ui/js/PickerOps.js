.pragma library

.import "Ops.js" as Ops
.import "Picker.js" as Picker
.import "PickerMarks.js" as Marks

// How the chooser's marks meet ui/js/Ops.js, Sort.js and Drag.js, which read a pane. Those files
// name rows by listing index and a mark is a path, so this is where a mark becomes an index and an
// index becomes a path again. ui/PickerState.qml calls these and holds the state; nothing here
// touches the backend or a window.

// The listing indices of the marks standing in this directory, ascending. A mark is resolved
// against the rows the window holds, [held, held + rows.length): a mark on a row outside that
// window has no index anyone can name, so it is dropped here, the same rule Ops.targetPaths
// applies to a selection wider than the window. A mark from another directory never equals a row
// path of this one, so it drops for free.
function indicesFor(state) {
    var want = {}
    for (var m = 0; m < state.marks.length; m++) {
        want[state.marks[m].path] = true
    }
    var out = []
    for (var i = 0; i < state.rows.length; i++) {
        if (want[Picker.rowPath(state.path, state.rows[i].n)] === true) {
            out.push(state.held + i)
        }
    }
    return out
}

// The paths an operation acts on: the marks standing in this directory, or the cursor row alone
// when none does, the shape of Ops.targetIndices. A mark is a path, so one past the held window
// still answers here. corner: in Recent no mark's parent is the location token, so the cursor row
// alone answers; Recent is a history and an operation on it acts on one entry at a time.
function pathsFor(state) {
    var out = []
    for (var m = 0; m < state.marks.length; m++) {
        if (Picker.parentOf(state.marks[m].path) === state.path) {
            out.push(state.marks[m].path)
        }
    }
    if (out.length > 0) {
        return out
    }
    var row = state.rowFor(state.cursorIndex)
    return row ? [Picker.rowPath(state.path, row.n)] : []
}

// The picker already holds absolute paths, so its process-local clipboard needs no backend reply.
function clip(state, moving) {
    var paths = pathsFor(state)
    if (paths.length === 0) {
        return
    }
    state.clipboard = { paths: paths, moving: moving }
    state.message(Ops.copied(paths.length, moving).replace("p pastes.", "Ctrl+V pastes."), false)
}

// Recent has no destination directory. A cut spends its clipboard and only removes its own marks.
function paste(state) {
    if (!state.clipboard || state.clipboard.paths.length === 0) {
        state.message("Nothing to paste. Use Ctrl+C or Ctrl+X.", false)
        return
    }
    if (state.recent) {
        state.message("Recent is not a folder. Open a folder to paste.", false)
        return
    }
    var clip = { paths: state.clipboard.paths.slice(0), moving: state.clipboard.moving }
    Ops.paste(state)
    if (clip.moving) {
        dropMarks(state, clip.paths)
    }
}

// A trash or a move took these paths off the disk, so the marks naming them come off the list:
// a mark is an identity, and an identity that is gone cannot be sent to the caller.
function dropMarks(state, paths) {
    var gone = {}
    for (var p = 0; p < paths.length; p++) {
        gone[paths[p]] = true
    }
    var kept = []
    for (var m = 0; m < state.marks.length; m++) {
        if (gone[state.marks[m].path] !== true) {
            kept.push(state.marks[m])
        }
    }
    state.marks = kept
}

// Ctrl+A. Every markable row that is drawn joins the marks: what the chip hides stays out, the way
// ui/js/Filter.js selectAll takes the matches alone, and only the held window can be read. A
// request for one item has nothing to select all, and says so rather than marking the last row,
// which is what PickerMarks.toggle would leave standing.
function selectAll(state) {
    if (!state.req.multiple) {
        state.message("The caller takes one " + (state.folderMode ? "folder" : "file") + " only.", false)
        return
    }
    var drawn = state.shown
    if (drawn === null) {
        drawn = []
        for (var i = 0; i < state.rows.length; i++) {
            drawn.push(state.held + i)
        }
    }
    var rows = []
    for (var d = 0; d < drawn.length; d++) {
        var row = state.rowFor(drawn[d])
        if (row && row.d === state.folderMode) {
            rows.push({ path: Picker.rowPath(state.path, row.n), bytes: row.s })
        }
    }
    state.marks = Marks.markRange(state.marks, rows, true)
}

// The re-read after one of the picker's own writes, ui/js/Nav.js refresh for the chooser. The
// directory is listed again without a history entry, because Back after a rename should leave the
// directory and not return to it, and the cursor is put on the path the write produced once the
// rows arrive, through ui/PickerNavigate.qml's own pendingSelect route. The marks stand: they are
// paths, and a re-read cannot rebind one.
function refresh(state, selectPath) {
    state.openWithoutHistory(state.path)
    if (selectPath) {
        state.navigate.seat(selectPath)
    }
}
