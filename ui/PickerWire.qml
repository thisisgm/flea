import QtQuick
import "js/Ops.js" as Ops
import "js/PickerWire.js" as Wire
import "js/Thumbs.js" as Thumbs

// Every operation reply the backend sends the chooser lands here, ui/PaneWire.qml's handlers for
// the state ui/PickerState.qml holds. It owns nothing and writes only through that state, and each
// handler is one call into ui/js/Wire.js, so tests/js/pickerops.js runs the replies without a
// window. The listing replies stay in ui/picker.qml, which seats the cursor on the row a refresh
// asked for; the one rows handler below reads that landing to arm the rename editor.
QtObject {
    id: root

    property var picker: null

    // The wrapper holds the picker because a Connections owns only its handlers.
    property Connections replies: Connections {
        target: root.picker ? root.picker.backend : null

        // The window's own onRows has run by now: an inline handler connects when its object is
        // created and a Connections when the tree completes, and Qt delivers in connection order.
        // Were that ever reversed the guard in armRename would miss the editor, never misplace it.
        function onRows(start, items, ms, kinds) { Wire.armRename(root.picker) }

        function onTrashed(ok, failed) { Wire.trashed(root.picker, ok, failed) }
        function onRenamed(ok, path) { Wire.renamed(root.picker, ok, path) }
        function onMade(ok, path) { Wire.made(root.picker, ok, path) }
        function onDuplicated(ok, path) { Wire.duplicated(root.picker, ok, path) }
        function onUndone(op, ok) { Wire.undone(root.picker, op, ok) }
        // The answer to Ops.clip's askPaths; nothing reaches the clipboard until this lands.
        function onPaths(list) { Ops.pathsResolved(root.picker, list) }

        function onTransferStarted(id, n, moving) { Wire.transferStarted(root.picker, id, n, moving) }
        function onTransferProgress(id, index, name, bytes, total) {
            Wire.transferProgress(root.picker, id, index, name, bytes, total)
        }
        function onTransferItem(id, index, name, ok, err) { Wire.transferItem(root.picker, id, index, name) }
        function onTransferDone(id, ok, failed, skipped, cancelled) {
            Wire.transferDone(root.picker, id, ok, failed, skipped, cancelled)
        }

        // A thumbed line for the previous listing is still in the pipe when open() clears the map,
        // the same guard ui/PaneWire.qml makes on the pane's listInFlight.
        function onThumbed(row, file) {
            if (root.picker.listingState !== "loading")
                root.picker.thumbState = Thumbs.remember(root.picker.thumbState, row, file, root.picker.thumbCap)
        }

        function onFailed(where, input, msg, mode) { Wire.failed(root.picker, where, msg) }
    }
}
