.pragma library

.import "DragOut.js" as DragOut
.import "Ops.js" as Ops
.import "Swap.js" as Swap

// A drop is a gesture that calls the transfer request Ops.moveToDropbox already sends, with the folder
// row under the pointer as its destination, so there is no second copy path here. States.dc.html
// "Drop target": the hovered folder takes the accent frame and reads "move here", and copy versus
// move reads in the status bar. ui/List.qml wires the handlers; this file decides.

// The rows a drag carries: the whole selection when the pressed row is in it, that row alone when it
// is not. One file dropped when five were selected is a data surprise; five moved when the operator
// grabbed one is a worse one, so the pressed row decides which the drag means.
function carried(pane, index) {
    var picked = pane.selectedIndices()
    for (var i = 0; i < picked.length; i++) {
        if (picked[i] === index) {
            return picked
        }
    }
    return [index]
}

// Ctrl copies. Shift moves. Both are read at the lift: once Drag.active runs, the window gets no keys.
// Ctrl and Shift together stay a copy. A link is not offered.
function copying(modifiers) {
    return (modifiers & Qt.ControlModifier) !== 0
}

function shifting(modifiers) {
    return (modifiers & Qt.ShiftModifier) !== 0 && !copying(modifiers)
}

// Only a directory row takes a drop, and never one the drag itself carries: a folder cannot move into
// itself, and a selection holding the target is refused whole rather than moved in part.
function canDrop(rows, index, row) {
    if (!row || row.d !== true) {
        return false
    }
    return rows.indexOf(index) < 0
}

// The board's own words on the hovered folder.
function label(copy) {
    return copy ? "copy here" : "move here"
}

// The status bar's half of the board. The hint names the lift because Drag.active runs a nested
// event loop the window gets no key events in, so a key pressed after the drag starts cannot arrive.
function line(n, name, copy) {
    var verb = copy ? "Copy " : "Move "
    var where = name.length > 0 ? " to " + name : " to a folder"
    return verb + Ops.items(n) + where + (copy ? "" : " · ctrl copies and shift moves, read at lift")
}

// Rows as Ops.moveToDropbox sends them, named in the listing of the lift; answers whether the card's question went out.
function drop(pane, rows, index, copy, listing) {
    var row = pane.rowFor(index)
    if (!canDrop(rows, index, row)) {
        return false
    }
    return pane.collide.ask(Swap.named({ c: "transfer", op: copy ? "copy" : "move", rows: rows, dest: pane.join(pane.path, row.n) }, listing))
}

// The local paths an external drag carries. Qt hands these over as file:// URIs, and anything that is
// not one is left behind rather than guessed at, so a drag from a browser carrying an http link
// contributes nothing instead of a bogus path. The wire form is percent-encoded, so it is decoded here.
function pathsFromUrls(urls) {
    return DragOut.filePaths(urls)
}


// The type Flea's own drag carries alongside the uri-list. The compositor hands a window's own
// platform drag back to that window's own DropAreas, so without a marker an internal move would be
// indistinguishable from a foreign drop, take the always-copy path below, and leave the source behind
// while still looking like it worked.
var ROWS_MIME = "application/x-flea-rows"

// DragOut rule 4: Flea is the one named receiver of a shelf drag. The payload is the single-use
// token the shelf minted and the intent it fixed at the lift, in that order, one per line. The token
// is the authority: the backend reads the entries and the intent out of its own record, and this
// second line is only so the receiver can say the right word before the drop lands.
var SHELF_MIME = "application/x-flea-shelf"

function shelfToken(payload) {
    return String(payload || "").split("\n")[0]
}

function shelfCopying(payload) {
    return String(payload || "").split("\n")[1] === "copy"
}

// A value unique to this running Flea. ROWS_MIME names the application, and two Flea windows are two
// processes: a drag from the other one carries row indices that mean nothing in this listing, so the
// instance has to be identifiable on its own or the receiver takes the internal path against a
// selection it never made and the drop does nothing at all.
var INSTANCE = String(Date.now()) + "-" + String(Math.floor(Math.random() * 1000000000))

// The marker's payload: which Flea sent it, the rows it carries, then the modifier the lift read.
// One marker rather than a second mime type, because two types are two things to keep in agreement
// and a drop carrying one but not the other is a state nobody would have written a branch for.
// Field 2 is ctrl at the lift. Field 5 is whether the source can be deleted (missing means yes, so
// another Flea's older marker still moves). Field 6 is shift at the lift.
// The listing directory's own writability. Missing means the reply predates the field, and a move
// stays available; an explicit false is a directory this user cannot delete from, so the drag copies.
function listingDeletable(pane) {
    return !(pane && pane.backend && pane.backend.dirWritable === false)
}

// dragFinished runs when the receiver accepts, which is before Files has read a uri-list it is
// about to move. Deleting on that acceptance puts the files in Trash and the destination never
// gets them. Files moves or copies the files itself. A drop this window already transferred
// accepts copy, so it does not delete either.
function releaseDeletes(dropAction, landed) {
    // dropAction is what the receiver accepted. landed means this window already transferred.
    // Files moves a uri-list after accepting it. An in-window drop already moved or copied.
    // Deleting on acceptance races that read, so this is never a delete.
    void dropAction
    void landed
    return false
}

function markerPayload(rows, copy, source, dev, deletable, shift) {
    return INSTANCE + "\n" + rows.join(",") + "\n" + (copy ? "copy" : "move") + "\n" + String(source || "") + "\n" + String(dev || 0) + "\n" + (deletable === false ? "0" : "1") + "\n" + (shift === true ? "1" : "0")
}

// The directory the rows were lifted from, and its filesystem, both baked at the lift: a drop that
// lands after the listing changed under the drag, which hovering a tab now does, cannot use the row
// indices any more and resolves by path against these instead.
function markerSource(payload) {
    return String(payload).split("\n")[3] || ""
}

function markerDev(payload) {
    return Number(String(payload).split("\n")[4]) || 0
}

function markerShift(payload) {
    return String(payload).split("\n")[6] === "1"
}

// Missing means the source can be deleted. Only an explicit 0 copies a same-device drag.
function markerDeletable(payload) {
    var field = String(payload).split("\n")[5]
    return field !== "0"
}

// Whether the listing under the drop is still the one the rows were lifted from, which is the only
// case the by-index transfer is safe in.
function sameListing(payload, path) {
    return isOwnDrag(payload) && markerSource(payload) === path
}

// Whether a drag carries at least one local path, which is what decides a drop resolves by path.
function hasPaths(urls) {
    return pathsFromUrls(urls).length > 0
}

// The by-index drop's gate: only a selection too wide to carry paths takes it, only onto its own
// listing, and never onto a folder it carries itself.
function canDropByIndex(marker, path, rows, index) {
    return rows.length > 0 && sameListing(marker, path) && rows.indexOf(index) < 0
}

// Whether the marked drag was lifted with ctrl down. No marker answers false.
function markerCopying(payload) {
    return String(payload).split("\n")[2] === "copy"
}

// Whether a marked drag began in this very window. An unmarked drag has no payload and answers false,
// which is the right answer: something that is not Flea is not this Flea.
function isOwnDrag(payload) {
    return String(payload).split("\n")[0] === INSTANCE
}

// A path as a file:// URI. Each component is encoded on its own: encodeURIComponent would escape the
// separators too, and encodeURI would leave a "#" or a "?" in a filename unescaped.
function uriFor(path) {
    var parts = path.split("/")
    for (var i = 0; i < parts.length; i++) {
        parts[i] = encodeURIComponent(parts[i])
    }
    return "file://" + parts.join("/")
}

// What the drag puts on the wire: the marker naming this drag as Flea's own, which the backend
// resolves by index and which therefore always carries the whole selection, plus a CRLF-separated
// uri-list for every other application.
//
// The uri-list is offered only when every carried row resolves. A selection reaches past the window
// the client holds and pane.rowFor answers null outside it, so a wide selection cannot be turned into
// paths here at all; pushing only the rows that happen to be realised is the defect Ops.js records as
// "a wide move relocated a few files and abandoned the rest", and here it would hand another
// application a subset while the bar named the whole count. No list at all is refusable and visible.
// Whether the key is present is also what tells the bar the drag cannot leave Flea.
function mimeFor(pane, rows, copy, shift) {
    var mime = {}
    mime[ROWS_MIME] = markerPayload(rows, copy, pane.path, pane.backend ? pane.backend.dirDev : 0, listingDeletable(pane), shift === true)
    var uris = []
    var paths = []
    for (var i = 0; i < rows.length; i++) {
        var row = pane.rowFor(rows[i])
        if (!row) {
            return mime
        }
        paths.push(pane.join(pane.path, row.n))
        uris.push(uriFor(paths[paths.length - 1]))
    }
    if (uris.length > 0) {
        mime["text/uri-list"] = uris.join("\r\n") + "\r\n"
        // GM's ruling: a terminal or a text field that takes plain text gets the absolute paths, one a line.
        mime["text/plain"] = paths.join("\n")
    }
    return mime
}

// Whether a drop may land in a directory named by path: the listing's own floor, another tab's
// directory, or a folder row reached after the listing changed under the drag. A drop into the
// directory the rows came from is nothing to do and is refused; a drag with no uri-list, which is a
// selection too wide to leave the window, carries no paths to send and is refused too.
function canDropInto(marker, urls, dest, plain) {
    if (isOwnDrag(marker) && markerSource(marker) === dest) return false
    var paths = DragOut.sources(urls, plain, marker, "")
    return paths.length > 0 && DragOut.refusal(paths, dest, marker, "") === ""
}

// The transfer for a drop that resolves by path. verbFor is the only copy-versus-move decision.
// proposed is the platform action of a drag that has no marker: Files states move or copy there.
function dropInto(pane, marker, urls, dest, destDev, shelf, plain, proposed) {
    var paths = DragOut.sources(urls, plain, marker, shelf)
    if (!canDropInto(marker, urls, dest, plain) && shelfToken(shelf).length === 0) return false
    // Rule 4: a shelf drag is redeemed rather than re-read as a list of URIs, because a fallback to
    // a URI copy after the shelf promised a move is the silent wrong answer it forbids; its URIs only ask first.
    if (shelfToken(shelf).length > 0) {
        return pane.collide.ask({ c: "transfer", op: "", paths: [], dest: dest, shelf: shelfToken(shelf) }, pathsFromUrls(urls))
    }
    var ctrl = markerCopying(marker)
    var shift = markerShift(marker)
    var srcDev = markerDev(marker)
    if (!marker) {
        var moveBit = (proposed & Qt.MoveAction) !== 0
        var copyBit = (proposed & Qt.CopyAction) !== 0
        if (moveBit && !copyBit) shift = true
        else if (copyBit && !moveBit) ctrl = true
    }
    var verb = verbFor(isOwnDrag(marker), ctrl, shift, srcDev, destDev, markerDeletable(marker))
    return pane.collide.ask({ c: "transfer", op: verb, paths: paths, dest: dest })
}

// The only place copy versus move is chosen. own records which process started the drag and does
// not change the answer: another process follows the device rule. Ctrl copies. Shift moves, even
// across devices, even when a device is unknown, even when the source cannot be deleted. Ctrl wins
// if both are held. An unknown device copies. A source that cannot be deleted copies unless Shift.
function verbFor(own, ctrlHeld, shiftHeld, srcDev, destDev, deletable) {
    if (ctrlHeld) return "copy"
    if (shiftHeld) return "move"
    if (!srcDev || !destDev || deletable === false) return "copy"
    return srcDev === destDev ? "move" : "copy"
}

// Said beside the drag line when the selection cannot be handed to another application. The internal
// drop is still whole, so the count stands; this only tells the operator the drag will not leave,
// which beats a drop that silently does nothing over another window.
function reachNote(canLeave) {
    return canLeave ? "" : " · too wide to drag out"
}

// Sample marker: "<instance>\n1,3\nmove\n/source\n42"; feedback never becomes destination row indices.
function feedbackFor(marker, urls, shelf, proposed) {
    var fields = String(marker).split("\n")
    var own = fields[0] === INSTANCE
    var paths = pathsFromUrls(urls)
    if (shelfToken(shelf).length > 0) {
        return { own: true, copy: shelfCopying(shelf), dev: 0, fixed: shelfCopying(shelf),
                 count: paths.length, canLeave: paths.length > 0 }
    }
    var copy = fields[2] === "copy"
    var shift = fields[6] === "1"
    if (!marker) {
        if ((proposed & Qt.MoveAction) !== 0 && (proposed & Qt.CopyAction) === 0) shift = true
        else if ((proposed & Qt.CopyAction) !== 0 && (proposed & Qt.MoveAction) === 0) copy = true
    }
    return { own: own, copy: copy, shift: shift,
             dev: Number(fields[4]) || 0, deletable: fields[5] !== "0",
             count: own && fields[1] ? fields[1].split(",").length : paths.length,
             canLeave: paths.length > 0 }
}

function copyingFor(feedback, destDev) {
    if (!feedback) return true
    return feedback.fixed !== undefined ? feedback.fixed === true
        : verbFor(feedback.own, feedback.copy, feedback.shift, feedback.dev, destDev, feedback.deletable) === "copy"
}

function feedbackLine(feedback, name, destDev) {
    if (!feedback || feedback.count === 0) return ""
    return line(feedback.count, name, copyingFor(feedback, destDev)) + reachNote(feedback.canLeave)
}
