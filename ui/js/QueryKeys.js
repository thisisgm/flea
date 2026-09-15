.pragma library

// Query editing consumes text paste before the listing's file-paste action can see it.
function paste(event, pane) {
    var mask = event.modifiers & (Qt.ControlModifier | Qt.ShiftModifier | Qt.AltModifier | Qt.MetaModifier)
    var chord = event.key === Qt.Key_V && (mask === Qt.ControlModifier || mask === Qt.MetaModifier)
    chord = chord || (event.key === Qt.Key_Insert && mask === Qt.ShiftModifier)
    if (!chord) return false
    pane.queryClipboard.paste()
    return true
}
