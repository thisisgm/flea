.pragma library

.import "Filter.js" as Filter

// What the preview overlay does with a key, split out of Focus.js at its 300-line hard cap the
// same way ui/js/Trash.js was: Focus.js decides which surface owns a key, and this is the surface.

// Left/Right's seek step, Task 22's operator ruling; act is the only reader.
var SEEK_MS = 5000

// A directory has no preview kind of its own, so Space on one is a silent no-op rather than an error.
function open(root) {
    var row = root.rowFor(root.cursorIndex)
    if (row && !row.d)
        root.preview.open(root.join(root.path, row.n), row.i, row.s)
}

// Preview open: j/k move the cursor and the preview follows; escape always closes. Space closes
// a text or unsupported preview as before, but toggles play/pause on a MEDIA one instead
// (Task 22's operator ruling: "the idea is our preview is as good or better than Showtime").
// Any key reveals the media strip, even one that does nothing else, matching "move the mouse or
// press anything" from the same ruling.
function act(action, root) {
    root.preview.revealStrip()
    switch (action) {
    case "cursorDown": Filter.moveCursor(root, 1); follow(root); return
    case "cursorUp": Filter.moveCursor(root, -1); follow(root); return
    case "preview":
        if (root.preview.isMedia) root.preview.togglePlay()
        else root.preview.close()
        return
    case "escape": root.preview.close(); return
    case "seekBack":
        if (root.preview.isPdf) root.preview.turnPage(-1)
        else root.preview.seek(-SEEK_MS)
        return
    case "seekForward":
        if (root.preview.isPdf) root.preview.turnPage(1)
        else root.preview.seek(SEEK_MS)
        return
    // h keeps its own "parent" name from keys.toml; turnPage self-guards, so a media preview
    // ignores both of these rather than seeking on a key the strip never advertised.
    case "parent": root.preview.turnPage(-1); return
    case "pageForward": root.preview.turnPage(1); return
    case "zoomOut": root.preview.zoomBy(-1); return
    case "zoomIn": root.preview.zoomBy(1); return
    case "expand": root.preview.toggleExpand(); return
    }
}

// The row under the moved cursor, handed to Preview.follow so a held key settles before it reloads.
function follow(root) {
    var row = root.rowFor(root.cursorIndex)
    if (row && !row.d)
        root.preview.follow(root.join(root.path, row.n), row.i, row.s)
}
