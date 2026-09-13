.pragma library

.import "Filter.js" as Filter

// What the preview overlay does with a key, split out of Focus.js at its 300-line hard cap the
// same way ui/js/Trash.js was: Focus.js decides which surface owns a key, and this is the surface.

// Left/Right's seek step, Task 22's operator ruling; act is the only reader.
var SEEK_MS = 5000

function pdfAction(action, viewer) {
    var controls = viewer.pdfControls
    if (action === "focusNext" || action === "focusPrevious") {
        var step = action === "focusPrevious" ? -1 : 1
        var next = viewer.pdfControlIndex
        if (next < 0) next = step > 0 ? -1 : 0
        for (var i = 0; i < controls.length; i++) {
            next = (next + step + controls.length) % controls.length
            if (controls[next].enabled && controls[next].visible) {
                viewer.pdfControlIndex = next
                break
            }
        }
    } else if (action === "open" || action === "preview") {
        var control = controls[viewer.pdfControlIndex]
        if (control && control.enabled && control.visible) control.activated()
    } else if (action === "seekBack" || action === "parent") viewer.turnPage(-1)
    else if (action === "seekForward" || action === "pageForward") viewer.turnPage(1)
    else if (action === "zoomOut") viewer.zoomBy(-1)
    else if (action === "zoomIn") viewer.zoomBy(1)
    else if (action === "expand") viewer.toggleExpand()
    else if (action === "cursorDown" || action === "cursorUp") viewer.scrollPage(action === "cursorDown" ? 1 : -1)
}

// A directory has no preview kind of its own, so Space on one is a silent no-op rather than an error.
function open(root) {
    var row = root.rowFor(root.cursorIndex)
    if (row && !row.d)
        root.preview.open(root.join(root.path, row.n), row.i, row.s)
}

// Preview open: j/k move the cursor and the preview follows; escape always closes, and so does a
// second space, on every kind including media (GM, 2026-09-11: "pressing space a second time should
// close the preview, just like Finder does"). That reverses Task 22, which had space toggle
// play/pause on a media preview; the strip's own play control still does that with the pointer.
// Any key reveals the media strip, even one that does nothing else, matching "move the mouse or
// press anything" from the same ruling.
function act(action, root) {
    root.preview.revealStrip()
    switch (action) {
    case "cursorDown": Filter.moveCursor(root, 1); follow(root); return
    case "cursorUp": Filter.moveCursor(root, -1); follow(root); return
    case "preview": root.preview.close(); return
    // Space closes every kind now, so playback has its own key; it self-guards, because p reaches
    // this only in the media context and a still image has nothing to play.
    case "playPause":
        if (root.preview.isMedia) root.preview.togglePlay()
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
