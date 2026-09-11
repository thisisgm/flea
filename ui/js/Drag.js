.pragma library

.import "Ops.js" as Ops

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

// Ctrl copies and a plain drag moves, the convention of every desktop file manager and the board's
// own default, whose label reads "move here". Shift and alt mean nothing here.
function copying(modifiers) {
    return (modifiers & Qt.ControlModifier) !== 0
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

// The status bar's half of the board: "Move 2 items to omarchy · ctrl at lift copies". The hint
// names the lift because Drag.active runs a nested event loop the window gets no key events in, so
// a ctrl pressed after the drag starts cannot reach anything and must not be advertised as if it can.
function line(n, name, copy) {
    var verb = copy ? "Copy " : "Move "
    var where = name.length > 0 ? " to " + name : " to a folder"
    return verb + Ops.items(n) + where + (copy ? "" : " · ctrl at lift copies")
}

// The drop: rows, not paths, for the reason Ops.moveToDropbox gives, and the clipboard is left alone
// for its reason too. Answers whether a request went out, so a refused drop is silent by design.
function drop(pane, rows, index, copy) {
    var row = pane.rowFor(index)
    if (!canDrop(rows, index, row)) {
        return false
    }
    pane.backend.send({ c: "transfer", op: copy ? "copy" : "move", rows: rows, dest: pane.join(pane.path, row.n) })
    return true
}

// The local paths an external drag carries. Qt hands these over as file:// URIs, and anything that is
// not one is left behind rather than guessed at, so a drag from a browser carrying an http link
// contributes nothing instead of a bogus path. The wire form is percent-encoded, so it is decoded here.
function pathsFromUrls(urls) {
    var paths = []
    if (!urls) {
        return paths
    }
    for (var i = 0; i < urls.length; i++) {
        var url = String(urls[i])
        if (url.indexOf("file:///") !== 0) {
            continue
        }
        paths.push(decodeURIComponent(url.substring(7)))
    }
    return paths
}


// The type Flea's own drag carries alongside the uri-list. The compositor hands a window's own
// platform drag back to that window's own DropAreas, so without a marker an internal move would be
// indistinguishable from a foreign drop, take the always-copy path below, and leave the source behind
// while still looking like it worked.
var ROWS_MIME = "application/x-flea-rows"

// A value unique to this running Flea. ROWS_MIME names the application, and two Flea windows are two
// processes: a drag from the other one carries row indices that mean nothing in this listing, so the
// instance has to be identifiable on its own or the receiver takes the internal path against a
// selection it never made and the drop does nothing at all.
var INSTANCE = String(Date.now()) + "-" + String(Math.floor(Math.random() * 1000000000))

// The marker's payload: which Flea sent it, the rows it carries, then the modifier the lift read.
// One marker rather than a second mime type, because two types are two things to keep in agreement
// and a drop carrying one but not the other is a state nobody would have written a branch for.
// The modifier rides here because Drag.supportedActions is the only field another application ever
// sees, and offering it Qt.MoveAction is what made Chromium report dropEffect move.
function markerPayload(rows, copy, source, dev) {
    return INSTANCE + "\n" + rows.join(",") + "\n" + (copy ? "copy" : "move") + "\n" + String(source || "") + "\n" + String(dev || 0)
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

// Whether the marked drag was lifted with ctrl down. Anything carrying no marker answers false,
// which costs nothing: verbFor already copies everything that did not come from this window.
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
function mimeFor(pane, rows, copy) {
    var mime = {}
    mime[ROWS_MIME] = markerPayload(rows, copy, pane.path, pane.backend ? pane.backend.dirDev : 0)
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
function canDropInto(marker, urls, dest) {
    if (isOwnDrag(marker) && markerSource(marker) === dest) {
        return false
    }
    var paths = pathsFromUrls(urls)
    for (var i = 0; i < paths.length; i++) {
        // A folder into itself or its own subtree: copy_dir would read its own fresh copy until the
        // disk is full, so the drop is refused here and again in src/backend/opsreq.rs.
        if (dest === paths[i] || dest.indexOf(paths[i] + "/") === 0) {
            return false
        }
        // An item into the folder it already lives in, which a foreign drag can ask for: a copy onto itself.
        var slash = paths[i].lastIndexOf("/")
        if ((slash === 0 ? "/" : paths[i].substring(0, slash)) === dest) {
            return false
        }
    }
    return paths.length > 0
}

// The transfer for a drop that resolves by path. verbFor decides move against copy the same way a
// row drop does, from the marker's own device against the destination's; a drop from anywhere but
// this window copies, so no source deletes a file on the strength of a drop it did not deliver.
function dropInto(pane, marker, urls, dest, destDev) {
    if (!canDropInto(marker, urls, dest)) {
        return false
    }
    var verb = verbFor(isOwnDrag(marker), markerCopying(marker), markerDev(marker), destDev)
    pane.backend.send({ c: "transfer", op: verb, paths: pathsFromUrls(urls), dest: dest })
    return true
}

// THE one place the verb is decided, so the label the operator reads and the request that is sent
// cannot drift apart. Finder's rules, all of them: a drag from anywhere but this window copies, a
// drag within one volume moves, a drag across two copies so the original survives the crossing, and
// ctrl forces a copy either way.
//
// ctrlHeld comes off the marker markerCopying reads, and never off the drop event: Qt clamps a
// DragEvent's proposedAction to the actions the source advertised, so a copy-only drag reports
// Qt.CopyAction whether or not ctrl is down.
//
// srcDev is the listing's own filesystem from the listed line and destDev is the dropped-on folder's
// from its row; docs/protocol.md documents both. Either being 0 means the stat failed, which is a
// boundary that cannot be ruled out: copying where Finder would move is an annoyance, and moving
// where Finder would copy loses the original, so unknown copies.
function verbFor(own, ctrlHeld, srcDev, destDev) {
    if (!own || ctrlHeld) {
        return "copy"
    }
    if (!srcDev || !destDev) {
        return "copy"
    }
    return srcDev === destDev ? "move" : "copy"
}

// Said beside the drag line when the selection cannot be handed to another application. The internal
// drop is still whole, so the count stands; this only tells the operator the drag will not leave,
// which beats a drop that silently does nothing over another window.
function reachNote(canLeave) {
    return canLeave ? "" : " · too wide to drag out"
}

// Sample marker: "<instance>\n1,3\nmove\n/source\n42"; feedback never becomes destination row indices.
function feedbackFor(marker, urls) {
    var fields = String(marker).split("\n")
    var own = fields[0] === INSTANCE
    var paths = pathsFromUrls(urls)
    return { own: own, copy: fields[2] === "copy", dev: Number(fields[4]) || 0,
             count: own && fields[1] ? fields[1].split(",").length : paths.length,
             canLeave: paths.length > 0 }
}

function feedbackLine(feedback, name, destDev) {
    if (!feedback || feedback.count === 0) return ""
    return line(feedback.count, name, verbFor(feedback.own, feedback.copy, feedback.dev, destDev) === "copy")
        + reachNote(feedback.canLeave)
}
