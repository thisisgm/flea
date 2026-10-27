.pragma library

.import "AnchorHold.js" as Hold

// Anchor locate IDs sit above QML's signed-int menu IDs, keeping the reply askers disjoint.
var LOCATE_ID_FLOOR = 2147483648
var locateSeq = LOCATE_ID_FLOOR

// Re-reading the open listing without moving the user off it, for a foreign change and Flea's own delete.

// A re-read renumbers every row, so it waits while an interaction owns the rows or while any paths asker resolves.
function busy(pane, anchor) {
    if (!pane)
        return true
    if (anchor && anchor.path === pane.path)
        return true
    if (pane.menuActions && (pane.menuActions.pendingAction || pane.menuActions.pendingActivation === true))
        return true
    if (pane.dragActive === true || pane.awaitingPaths === true || pane.pointerHeld === true)
        return true
    if (pane.pathsPending)
        return true
    if (pane.clipPending !== undefined && pane.clipPending !== null)
        return true
    return pane.listInFlight || pane.renamingIndex >= 0 || pane.renamePending
        || pane.menuVisible || (pane.menuActions && pane.menuActions.opened) || pane.filterTyping || pane.searchMode.length > 0
        || pane.selectionBand !== null || (pane.collide && pane.collide.pending !== null)
}

function watched(pane, wantChanged, rowH) {
    return anchoredRefresh(pane, false, selectedMarks(pane), wantChanged, rowH)
}

// Flea's own delete: the anchor is the deleted cursor row and apply()'s fallback lands on whatever took its place, which is Finder's rule. A failed delete matches by name instead.
// Sample input: afterDelete(pane, true) with pane.trashedFirst set by Ops.trash.
function afterDelete(pane, landed) {
    if (landed && pane.trashedFirst >= 0)
        pane.cursorIndex = pane.trashedFirst
    pane.trashedFirst = -1
    return anchoredRefresh(pane, true, null, false)
}

// A listing past its wait stops loading the pane: late rows are dropped as replaced.
function clearWaiting(pane) {
    if (pane.listingState === "waiting") {
        pane.listInFlight = false
        pane.listingState = "loading"
        pane.stateMessage = ""
        return true
    }
    return false
}

// Which row a full target path names, NFC-matched the way the backend lists it.
function selectMatch(rows, target, folder) {
    var base = String(folder || "")
    var text = String(target || "")
    var leaf = base.length <= 1 ? text.substring(1) : text.substring(base.length + 1)
    if (base.length > 1 && text.substring(0, base.length + 1) !== base + "/")
        return -1
    var want = typeof leaf.normalize === "function" ? leaf.normalize("NFC") : leaf
    for (var i = 0; i < rows.length; i++) {
        if (String(rows[i].n || "") === leaf)
            return i
    }
    for (var j = 0; j < rows.length; j++) {
        var name = String(rows[j].n || "")
        var norm = typeof name.normalize === "function" ? name.normalize("NFC") : name
        if (norm === want)
            return j
    }
    return -1
}

// The cursor index for a full target path, or -1 when the listing holds no such row.
function matchListed(pane, target) {
    var at = selectMatch(pane.rows, target, pane.path)
    return at >= 0 ? pane.held + at : -1
}

// A rename commit keeps the renamed row (source leaf mapped to destination) unless moved: a click or key took the cursor to another row.
function pointerRow(pane, request, rowH) {
    var row = pane.rowFor(pane.cursorIndex)
    var name = row ? String(row.n) : ""
    var src = Hold.holdLeaf(request.source)
    var dst = request.destination ? Hold.holdLeaf(request.destination) : ""
    var moved = name.length > 0 && name !== src && name !== dst
    if (name === src && dst.length > 0)
        name = dst
    var place = Hold.viewFields(pane, rowH)
    return { name: name, index: pane.cursorIndex, start: pane.held, path: pane.path, select: true, moved: moved,
             offset: place.offset, rowH: place.rowH, view: place.view, renamed: !moved && dst.length > 0 && name === dst }
}

function anchoredRefresh(pane, select, marks, wantChanged, rowH) {
    if (pane.listInFlight) {
        return null
    }
    var row = pane.rowFor(pane.cursorIndex)
    var lone = pane.selection && typeof pane.selection.follows === "function" ? !!pane.selection.follows() : false
    var place = Hold.viewFields(pane, rowH)
    // A filter never moves the held window while the cursor walks the folder's matches, so a cursor outside it asks its own window back, around its row (issue 315).
    var outside = pane.shown !== null && pane.shown !== undefined && (pane.cursorIndex < pane.held || pane.cursorIndex >= pane.held + pane.rows.length)
    // The path rides along because the anchor can outlive one rows reply: a navigation between the two below would otherwise put this directory's cursor row onto the next directory's listing.
    var anchor = { name: row ? String(row.n) : "", index: pane.cursorIndex, start: outside ? Math.max(0, pane.cursorIndex - pane.buffer) : pane.held,
                   path: pane.path, select: select === true, wantChanged: wantChanged === true, reloadFrom: pane.total,
                   marks: marks || null, kept: [], hadMarks: marks !== null && marks.length > 0,
                   lone: lone, offset: place.offset, rowH: place.rowH, view: place.view }
    // F2: names outside the held window come from one batched paths request before the swap, tagged so the reply reaches only this asker.
    if (anchor.marks) {
        var need = []
        for (var i = 0; i < anchor.marks.length; i++) {
            if (anchor.marks[i].name === null)
                need.push(anchor.marks[i].index)
        }
        if (need.length > 0 && !pane.pathsPending && (pane.clipPending === undefined || pane.clipPending === null)
                && pane.backend && pane.backend.send) {
            pane.backend.send({ c: "paths", rows: need })
            anchor.needPaths = need
            pane.pathsPending = { kind: "anchor" }
            return anchor
        }
    }
    Hold.startList(pane, anchor)
    return anchor
}

// The batched paths answer for F2: fills unheld names, then starts the list the swap holds.
function fillPaths(pane, anchor, list) {
    return Hold.fillPaths(pane, anchor, list)
}

// True only for the anchor's own tagged reply; a clipboard, drag or compress reply takes nothing.
function takesPaths(pane, anchor) {
    return !!(anchor && anchor.needPaths && pane.pathsPending && pane.pathsPending.kind === "anchor")
}

// A failed paths ask keeps the anchor so the shifted listing lands it by name; a refused locate ends it.
// Sample input: failAnchor(pane, { name: "b", index: 1, marks: [], kept: [] }) keeps the anchor for rows ["NEW","a","b","c"].
function failAnchor(pane, anchor, rowH) {
    if (!anchor)
        return null
    var hadPaths = !!anchor.needPaths
    anchor.needPaths = null
    anchor.locateDone = true
    if (pane.pathsPending && pane.pathsPending.kind === "anchor")
        pane.pathsPending = null
    if (hadPaths) {
        if (pane.path !== anchor.path)
            return null
        // Land by name on the held rows before the re-list renumbers them; a miss waits for the shifted listing.
        var at = indexOf(pane, anchor.name)
        if (pane.total > 0 && at >= 0) {
            landOn(pane, at, anchor)
            Hold.restoreView(pane, anchor, rowH)
        }
        // A failed ask still owes the listing that draws the outside change, unless one is already out.
        if (!pane.listInFlight)
            Hold.startList(pane, anchor)
        // Keep the target across the renumber so apply() re-lands it; ending here would strand the pre-list index.
        return anchor
    }
    var found = indexOf(pane, anchor.name)
    if (pane.total > 0) {
        landOn(pane, found >= 0 ? found : Math.min(anchor.index, pane.total - 1), anchor)
        Hold.restoreView(pane, anchor, rowH)
    }
    finishMarks(pane, anchor)
    return null
}

// PaneWire's located guard consumes only this anchor's request; a refused own reply ends the anchor.
// Sample input: { directory: "/d", id: 2147483649, transferId: 0, ok: true, matches: [{ path: "/d/a", index: 3 }] }.
function takeLocated(pane, anchor, message, rowH) {
    if (!anchor || !anchor.locateSent || anchor.locateDone)
        return { handled: false, anchor: anchor }
    if (!message || message.directory !== pane.path || message.id !== anchor.locateId || message.transferId !== 0)
        return { handled: false, anchor: anchor }
    if (message.ok === false)
        return { handled: true, anchor: failAnchor(pane, anchor, rowH) }
    return { handled: true, anchor: fillLocated(pane, anchor, message.matches || [], rowH) }
}

// The marks the re-read carries across by file identity: an index renumbers under a fresh list, so each marked row's name is recorded and looked up again when the rows land. A gone name loses its mark rather than re-pointing at another file.
function selectedMarks(pane) {
    if (!pane.selectedIndices)
        return []
    var out = []
    var indices = pane.selectedIndices()
    for (var i = 0; i < indices.length; i++) {
        var row = pane.rowFor(indices[i])
        out.push({ index: indices[i], name: row ? String(row.n) : null })
    }
    return out
}

// The listing row holding a name in the window the pane holds now, or -1.
function indexOf(pane, name) {
    for (var i = 0; i < pane.rows.length; i++) {
        if (String(pane.rows[i].n) === name)
            return pane.held + i
    }
    return -1
}

// Moves whatever the held window holds from pending to kept, naming nothing on screen: the take lands once, when the anchor resolves, so no rows reply in between draws a half-restored set.
function sweepMarks(pane, anchor) {
    if (!anchor.marks)
        return
    var pending = []
    for (var m = 0; m < anchor.marks.length; m++) {
        var mark = anchor.marks[m]
        if (mark.name === null)
            continue
        var at = indexOf(pane, mark.name)
        if (at >= 0)
            anchor.kept.push(at)
        else
            pending.push(mark)
    }
    anchor.marks = pending
}

// The one take, on the anchor resolving: the rows the re-read cleared are marked again on the same files, and whatever is gone stays gone. A lone selection restores with only(), the way Tabs.restoreSelection does, so a plain click never turns sticky behind the re-read.
function finishMarks(pane, anchor) {
    var kept = anchor.kept || []
    anchor.kept = []
    anchor.marks = []
    if (!anchor.hadMarks)
        return
    if (anchor.lone === true && kept.length === 1) {
        pane.selection.only(kept[0])
        pane.selectionAnchor = pane.cursorIndex
        pane.selectionVersion += 1
        return
    }
    for (var i = 0; i < kept.length; i++)
        pane.selection.toggle(kept[i])
    pane.selectionAnchor = pane.cursorIndex
    if (kept.length > 0)
        pane.selectionVersion += 1
}

// Runs on each rows reply while an anchor stands. A miss in the listing's first window is not yet a miss when the anchor asked for its own window above. A gone name falls back to the clamped old index, which keeps the view where the user left it.
function apply(pane, anchor, rowH) {
    if (!anchor) {
        return null
    }
    if (pane.path !== anchor.path) {
        return null
    }
    if (anchor.needPaths)
        return anchor
    sweepMarks(pane, anchor)
    // F3: past the first window the cursor name alone must not finish held marks past it.
    if (anchor.start > 0 && pane.held !== anchor.start && pane.total > anchor.start)
        return anchor
    var at = indexOf(pane, anchor.name)
    // F2: marks past the held window resolve through one batched locate after.
    if (anchor.marks && anchor.marks.length > 0 && !anchor.locateDone && pane.backend && pane.backend.send) {
        var paths = []
        for (var m = 0; m < anchor.marks.length; m++) {
            if (anchor.marks[m].name !== null)
                paths.push(pane.join(pane.path, anchor.marks[m].name))
        }
        if (paths.length > 0 && !anchor.locateSent) {
            anchor.locateId = ++locateSeq
            pane.backend.send({ c: "locate", paths: paths, id: anchor.locateId })
            anchor.locateSent = true
            anchor.locatePaths = paths
            // A gone cursor file still lands on the clamped old index while its marks resolve.
            if (pane.total > 0) {
                landOn(pane, at >= 0 ? at : Math.min(anchor.index, pane.total - 1), anchor)
                Hold.restoreView(pane, anchor, rowH)
            }
            return anchor
        }
        if (anchor.locateSent && !anchor.locateDone)
            return anchor
    }
    if (at >= 0) {
        landOn(pane, at, anchor)
        Hold.restoreView(pane, anchor, rowH)
        finishMarks(pane, anchor)
        return null
    }
    // Still the first window rather than the one asked for above, so keep waiting, but only while that window can still exist: a listing that shrank past the offset comes back clamped to row 0 instead.
    if (anchor.start > 0 && pane.held === 0 && pane.total > anchor.start) {
        return anchor
    }
    if (anchor.renamed && !anchor.locateSent) anchor.locateId = ++locateSeq
    if (Hold.locateRenamed(pane, anchor)) return anchor
    if (pane.total > 0) {
        landOn(pane, Math.min(anchor.index, pane.total - 1), anchor)
        Hold.restoreView(pane, anchor, rowH)
    }
    finishMarks(pane, anchor)
    return null
}

// The batched locate answer for F2: indices outside the held window join kept, the gone stay gone, and the cursor lands found-or-clamped either way.
// Sample input: fillLocated(pane, anchor, [{ path: "/d/a", index: 3 }]) keeps 3 and lands the cursor.
function fillLocated(pane, anchor, matches, rowH) {
    if (!anchor)
        return null
    Hold.fillLocated(pane, anchor, matches)
    var at = indexOf(pane, anchor.name)
    if (pane.total > 0 && !anchor.landed) {
        landOn(pane, at >= 0 ? at : Math.min(anchor.index, pane.total - 1), anchor)
        Hold.restoreView(pane, anchor, rowH)
    }
    finishMarks(pane, anchor)
    return null
}

// A watch's anchor moves the cursor and re-marks the same files: the operator's marks belong to them, and a change another program made must not move those marks onto other files. A delete's anchor selects instead, because its rows no longer exist and a bare cursor would restart the keyboard.
function landOn(pane, index, anchor) {
    if (anchor.select)
        pane.selectOnly(index, 0)
    else
        pane.setCursor(index, 0)
}

// A preference re-list keeps selection and cursor by name; a mark outside the held window has no name to keep, so a partial selection clears whole rather than keeping its subset.
function preference(pane) {
    var hasRowFor = !!pane.rowFor
    var cursorRow = hasRowFor ? pane.rowFor(pane.cursorIndex) : null
    var names = []
    var indices = pane.selectedIndices ? pane.selectedIndices() : []
    if (hasRowFor) {
        for (var i = 0; i < indices.length; i++) {
            var row = pane.rowFor(indices[i])
            if (!row) { names = []; break }
            names.push(String(row.n))
        }
    }
    return { name: cursorRow ? String(cursorRow.n) : "", index: pane.cursorIndex,
             start: pane.held, path: pane.path, selected: names }
}

// Resolves only on the rows reply for the asked window; a scrolled reply or a moved cursor drops the anchor instead of yanking it.
function applyPreference(pane, anchor) {
    if (!anchor)
        return null
    if (pane.path !== anchor.path)
        return null
    if (pane.cursorIndex !== 0 && pane.cursorIndex !== anchor.index)
        return null
    if (pane.held !== anchor.start) {
        if (anchor.start > 0 && pane.held === 0 && pane.total > anchor.start)
            return anchor
        if (!(anchor.start > 0 && pane.total <= anchor.start))
            return null
    }
    var cursorAt = -1
    for (var i = 0; i < pane.rows.length; i++) {
        if (String(pane.rows[i].n) === anchor.name) {
            cursorAt = pane.held + i
            break
        }
    }
    if (cursorAt < 0 && pane.total > 0)
        cursorAt = Math.min(anchor.index, pane.total - 1)
    var marks = []
    for (var s = 0; s < anchor.selected.length; s++) {
        for (var r = 0; r < pane.rows.length; r++) {
            if (String(pane.rows[r].n) === anchor.selected[s]) {
                marks.push(pane.held + r)
                break
            }
        }
    }
    if (cursorAt >= 0)
        pane.setCursor(cursorAt, 0)
    pane.selection.clear()
    for (var m = 0; m < marks.length; m++)
        pane.selection.toggle(marks[m])
    if (marks.length > 0 || cursorAt >= 0)
        pane.selectionVersion++
    return null
}
