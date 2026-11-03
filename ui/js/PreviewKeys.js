.pragma library

.import "Filter.js" as Filter
.import "Marks.js" as Marks
.import "Search.js" as Search
.import "Thumbs.js" as Thumbs

// What the preview overlay does with a key, split out of Focus.js at its 300-line hard cap the
// same way ui/js/Trash.js was: Focus.js decides which surface owns a key, and this is the surface.

// Left/Right's seek step, Task 22's operator ruling; act is the only reader.
var SEEK_MS = 5000
// The zoom ladder Quick Look's PDF strip walks: the bottom rung fits the frame, one step a press, up to four times.
var PDF_ZOOM_MIN = 1
var PDF_ZOOM_MAX = 4
var PDF_ZOOM_STEP = 0.25

// A zoom after steps presses (negative steps down), kept on the ladder; the image pane walks the same rungs as the PDF.
function zoomStep(zoom, steps) {
    return Math.max(PDF_ZOOM_MIN, Math.min(PDF_ZOOM_MAX, zoom + steps * PDF_ZOOM_STEP))
}

// The column's PDF has no strip: pages scroll, and Space, Return or e opens Quick Look at the page in view.
function columnPdfAction(action, viewer) {
    if (action === "open" || action === "preview" || action === "expand") viewer.toggleExpand()
    else if (action === "seekBack" || action === "parent") viewer.turnPage(-1)
    else if (action === "seekForward" || action === "pageForward") viewer.turnPage(1)
    else if (action === "cursorDown" || action === "cursorUp") viewer.scrollPage(action === "cursorDown" ? 1 : -1)
}

function pdfAction(action, viewer) {
    var controls = viewer.pdfControls
    if (controls === undefined) return columnPdfAction(action, viewer)
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

// Space on a folder is a silent no-op; a file carries its cached thumbnail so the overlay shows it at once.
function open(root) {
    if (!Filter.cursorShown(root)) return
    var row = root.rowFor(root.cursorIndex)
    if (row && !row.d)
        root.quickLook().open(root.join(Search.baseOf(root), row.n), row.i, row.s, root.kindNames[row.k] || "",
            root.thumbState && root.thumbState.file ? Thumbs.fileFor(root.thumbState, root.cursorIndex) : "")
}

// Preview open: j/k move the cursor and the preview follows; escape always closes, and so does a
// second space, on every kind including media (decided 2026-09-11: pressing Space a second time
// closes the preview, the way Finder does). That reverses Task 22, which had space toggle
// play/pause on a media preview; the strip's own play control still does that with the pointer.
// Any key reveals the media strip, even one that does nothing else, matching "move the mouse or
// press anything" from the same ruling.
function act(action, root) {
    root.preview.revealStrip()
    switch (action) {
    // A Compare overlay names two files, so the cursor moving under it opens no other file in its place.
    case "cursorDown":
    case "cursorUp":
        var direction = action === "cursorDown" ? 1 : -1
        if (root.preview.isHtml && root.preview.markdownShown && root.preview.markdownShown.shownView === "rendered")
            root.preview.markdownShown.scrollBy(direction)
        else if (!root.preview.isCompare) { Filter.moveCursor(root, direction); Marks.follow(root); follow(root) }
        return
    case "pageDown": root.preview.scrollDocument(1, true); return
    case "pageUp": root.preview.scrollDocument(-1, true); return
    case "preview": root.preview.close(); return
    // Space closes every kind now, so playback has its own key; it self-guards, because p reaches
    // this only in the media context and a still image has nothing to play.
    case "playPause":
        if (root.preview.isMedia) root.preview.togglePlay()
        return
    case "escape": root.preview.close(); return
    // Compare's y: the whole patch goes to the clipboard; the overlay guards on its own kind.
    case "copyPatch": root.preview.copyPatch(); return
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
    case "zoomFit": root.preview.zoomFit(); return
    case "expand": root.preview.toggleExpand(); return
    // MediaMute rule 5: the flag is the preview's to flip, and it silences without pausing.
    case "mute": root.preview.toggleMute(); return
    // r flips only the open document Quick Look between rendered and source.
    case "markdownView":
        if (root.preview.isDocument) root.preview.toggleMarkdownView()
        return
    // Focus.js lets w through for a code or text Quick Look only; the choice is remembered in ui.json.
    case "toggleWrap": root.preview.toggleWrap(); return
    // The text bar toggles Close; Compare owns its actual Copy and Close focus walk.
    case "focusNext":
    case "focusPrevious":
        if (root.preview.isCompare) {
            var compare = root.preview.surfaceItem()
            if (compare) compare.stepFocus(action === "focusPrevious")
        } else if (root.preview.hasBar) root.preview.toggleMarkdownClose()
        return
    case "open":
        if (root.preview.hasBar && root.preview.markdownCloseFocused) root.preview.close()
        return
    }
}

// The row under the moved cursor, handed to Preview.follow so a held key settles before it reloads.
function follow(root) {
    if (!Filter.cursorShown(root)) return
    var row = root.rowFor(root.cursorIndex)
    if (row && !row.d)
        root.preview.follow(root.join(Search.baseOf(root), row.n), row.i, row.s, root.kindNames[row.k] || "",
            root.thumbState && root.thumbState.file ? Thumbs.fileFor(root.thumbState, root.cursorIndex) : "")
}
