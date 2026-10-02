.pragma library

.import "Drag.js" as Drag
.import "Ops.js" as Ops
.import "Swap.js" as Swap
.import "Mounts.js" as Mounts

// A row drag dropped on the rail's Trash row trashes what it carries, the same trash request dd
// sends, so the drop is undone by z like any other delete. ui/RailTrashRow.qml wires the handlers;
// this file decides.
//
// Only a drag lifted in this very window may trash. A drop from another application, another Flea
// or the shelf carries files Flea did not hand over, and Drag.js verbFor already refuses to remove a
// source on the strength of a drop it did not deliver; a delete is that rule's strongest case.

// The rows the marker carries, as numbers. Sample marker: "<instance>\n1,3\nmove\n/d\n42\n7".
function markerRows(marker) {
    var field = String(marker).split("\n")[1] || ""
    if (field.length === 0) return []
    var rows = []
    var parts = field.split(",")
    for (var i = 0; i < parts.length; i++) {
        var n = Number(parts[i])
        if (!Number.isInteger(n) || n < 0) return []
        rows.push(n)
    }
    return rows
}

// The request a drop sends, or null when it takes none. The paths whenever the drag carries them,
// which survive any listing change under the drag; the rows only for a selection too wide to carry
// paths, only onto the listing it was lifted from, and named in the numbering of the lift, so
// src/backend/rowguard.rs refuses them rather than trashing whatever those indices name now.
function request(marker, urls, path) {
    if (!Drag.isOwnDrag(marker)) return null
    // Issue 133: a GVFS mount has no trash, so the drop is refused up front as d and the menu are.
    if (!Mounts.trashable(Drag.markerSource(marker) || path)) return null
    var paths = Drag.pathsFromUrls(urls)
    if (paths.length > 0) return { c: "trash", paths: paths }
    var rows = markerRows(marker)
    var listing = Drag.markerListing(marker)
    if (rows.length === 0 || listing <= 0 || !Drag.sameListing(marker, path)) return null
    return Swap.named({ c: "trash", rows: rows }, listing)
}

// Whether the drag can trash its carried files here, used by the hover check and status line.
function accepts(marker, urls, path) {
    return request(marker, urls, path) !== null
}

// Sends the drop's trash through the pane's own backend, whose trashed reply clears the selection
// and re-reads the listing; answers whether anything was sent.
function drop(pane, marker, urls) {
    if (!pane || !pane.backend) return false
    var sent = request(marker, urls, pane.path)
    if (!sent) return false
    // Where the cursor lands once the rows are gone, as dd sets it; only a drag from this listing has one.
    var rows = Drag.sameListing(marker, pane.path) ? markerRows(marker) : []
    var first = rows.length > 0 ? rows[0] : -1
    for (var i = 1; i < rows.length; i++)
        if (rows[i] < first) first = rows[i]
    pane.trashedFirst = first
    pane.backend.send(sent)
    return true
}

// The status bar while the drag rests on the Trash row. Sample: "Move 2 items to Trash".
function line(marker, urls, path) {
    if (!accepts(marker, urls, path)) return ""
    var rows = markerRows(marker)
    var n = rows.length > 0 ? rows.length : Drag.pathsFromUrls(urls).length
    return "Move " + Ops.items(n) + " to Trash"
}
