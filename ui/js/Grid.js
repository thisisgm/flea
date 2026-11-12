.pragma library

.import "Filter.js" as Filter
.import "Marks.js" as Marks

// The grid's own geometry: which cell a key means when the rows are tiles rather than lines. Split
// out of ui/js/Focus.js, which holds the key map and the actions every view shares.

// Which way a key steps across a row of tiles, the arrow and the letter the presets spell it with.
function sideways(event) {
    return event.key === Qt.Key_Left || event.text === "h" ? -1
         : event.key === Qt.Key_Right || event.text === "l" ? 1 : 0
}

// Arrow and letter actions share visual cells; absent neighbours clamp even when list navigation wraps.
function arrow(event, action, root) {
    if (root.viewMode !== "grid") return false
    var columns = root.cursorStride
    var across = sideways(event)
    var delta = (action === "cursorDown" || action === "extendDown") ? columns
              : (action === "cursorUp" || action === "extendUp") ? -columns
              : across < 0 && action === "cursorLeft" ? -1
              : across > 0 && action === "cursorRight" ? 1 : 0
    if (!delta) return false
    var index = Filter.viewOf(root.shown, root.cursorIndex)
    var nextIndex = index + delta
    if (nextIndex < 0 || nextIndex >= root.shownTotal
            || (across < 0 && index % columns === 0)
            || (across > 0 && nextIndex % columns === 0)) return true
    if (action === "extendDown" || action === "extendUp") root.extendSelection(delta)
    else { Filter.moveCursor(root, delta); Marks.follow(root) }
    return true
}
