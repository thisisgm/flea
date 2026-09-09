import QtQuick
import "." as Flea
import "js/Ops.js" as Ops
import "js/PickerWire.js" as Wire
import "js/Thumbs.js" as Thumbs

// Every reply the backend sends the chooser lands here, ui/PaneWire.qml's handlers for the state
// ui/PickerState.qml holds. It owns the chooser's opener and writes replies through that state.
// Each backend handler is one call into ui/js/PickerWire.js, so tests/js/pickerwire.js runs them
// without a window. The listing replies sort a fresh scan into the stored order before its rows
// show, seat the cursor on the row a refresh asked for, and then arm the rename editor over it.
QtObject {
    id: root

    property var picker: null

    // The chooser shares the browser's opener, but opening a file never answers its portal request.
    // A directory handed to Open comes back through isDirectory and becomes the listing instead.
    readonly property Flea.Opener opener: Flea.Opener {
        onBusy: function (path) { root.picker.message("Still opening the last file; try again in a moment.", false) }
        onFailed: function (path) { root.picker.message("That file could not be opened; nothing on this system took it.", true) }
        onIsDirectory: function (path) { root.picker.open(path) }
        onTerminalBusy: function (path) { root.picker.message("Still opening the last terminal; try again in a moment.", false) }
        onTerminalFailed: function (path) { root.picker.message("That directory could not be opened in a terminal; nothing on this system took it.", true) }
    }

    // The wrapper holds the picker because a Connections owns only its handlers.
    property Connections replies: Connections {
        target: root.picker ? root.picker.backend : null

        function onListed(n, readMs, sortMs) { Wire.listed(root.picker, n) }
        // The landing ui/PickerNavigate.qml makes inside rows is what armRename reads, so the editor
        // is armed only over rows the state took; rows a pending sort is about to replace arm nothing.
        function onRows(start, items, ms, kinds) {
            if (Wire.rows(root.picker, start, items, kinds))
                Wire.armRename(root.picker)
        }

        function onTrashed(ok, failed) { Wire.trashed(root.picker, ok, failed) }
        function onRenamed(ok, path) { Wire.renamed(root.picker, ok, path) }
        function onMade(ok, path) { Wire.made(root.picker, ok, path) }
        function onDuplicated(ok, path) { Wire.duplicated(root.picker, ok, path) }
        function onUndone(op, ok) { Wire.undone(root.picker, op, ok) }
        // Picker copy needs no path reply. Keep the generic reply route for future picker operations.
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
