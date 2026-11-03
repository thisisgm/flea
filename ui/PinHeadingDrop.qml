import QtQuick
import Quickshell
import qs.Commons
import "js/Drag.js" as DragOps
import "js/PinDrop.js" as PinDrop

// Downstream (driveway-fm): the FAVORITES heading's own receiver. One folder held over the heading
// asks the backend's kind line and arms the pin (accent cue, "Pin to Favorites"); the drop pins
// through Favourites.add, labelled the way the menu's Add to Favorites labels it. The caption is
// declared at the use site, so the rail's own heading list keeps its id; anything this area refuses
// falls through to ui/RailDrop.qml, whose row meanings are untouched, and the heading stays inert
// for shelf tokens, files, multi-row drags and folders already pinned.
DropArea {
    id: root

    required property var sidebar
    required property var captionItem
    readonly property var pane: root.sidebar.navigationPane
    readonly property bool armed: PinDrop.armed(root.pendingKind, root.pending)
    property string pending: ""
    property string pendingKind: ""
    property int requestId: 0
    property int pinSeq: 0
    property string pinRequestId: ""
    property string pinLabel: ""
    // The target is the whole rail row's width, the way a row is; a caption-width strip would be a target nobody can hit.
    width: parent ? parent.width : (captionItem ? captionItem.width : 0)
    height: captionItem ? captionItem.height : 0
    keys: [DragOps.ROWS_MIME, DragOps.SHELF_MIME, "text/uri-list", "text/plain"]

    // Sample input: a drag offers a row marker, a shelf token, text/plain and file:///home/u/Work.
    function payload(drag) {
        var urls = []
        for (var i = 0; i < drag.urls.length; i++) urls.push(String(drag.urls[i]))
        return { marker: drag.getDataAsString(DragOps.ROWS_MIME), urls: urls,
            shelf: drag.getDataAsString(DragOps.SHELF_MIME), plain: drag.getDataAsString("text/plain") }
    }

    function disarm() {
        root.pending = ""
        root.pendingKind = ""
    }

    onEntered: function (drag) {
        root.disarm()
        var data = root.payload(drag)
        var path = PinDrop.candidate(data.marker, data.urls, data.plain, data.shelf)
        if (!path || PinDrop.alreadyPinned(Favourites.records, path, Quickshell.env("HOME"))) { drag.accepted = false; return }
        root.pending = path
        root.requestId += 1
        root.pane.backend.send({ c: "kind", paths: [path], id: root.requestId })
    }
    onPositionChanged: function (drag) { if (!root.pending) drag.accepted = false }
    onExited: root.disarm()
    onDropped: function (drop) {
        var path = root.armed ? root.pending : ""
        root.disarm()
        if (!path) { drop.accepted = false; return }
        // add answers only that the save started; the saved-or-not answer is Favourites.completed.
        // A refused add must not touch the pinned request: the save it refused is still owed its
        // own completion, and a replaced id would swallow that answer's toast.
        var label = PinDrop.label(path)
        var requestId = "pindrop-" + (++root.pinSeq)
        if (Favourites.add(path, label, requestId)) {
            root.pinLabel = label
            root.pinRequestId = requestId
            drop.accept(Qt.CopyAction)
        } else drop.accepted = false
    }
    Connections {
        target: root.pane ? root.pane.backend : null
        function onMetaResult(message) {
            if (message.t !== "kind" || message.id !== root.requestId || !root.pending) return
            root.pendingKind = PinDrop.kindAnswer(message.kinds, root.pending)
        }
    }
    // A refused add (a save still in flight) fires Favourites.failed, not completed, and the
    // refused drop's slide-back already said it; only a started save gets the pinned toast.
    Connections {
        target: Favourites
        function onCompleted(requestId, success, message) {
            if (requestId !== root.pinRequestId) return
            // success with a message is the readback-failed shape: the file kept the row, but the
            // confirmation read did not come back, which is not a pin the user can trust.
            var completed = success && !message
            root.sidebar.message(completed ? "Pinned " + root.pinLabel + " to Favorites"
                : (message || "The favorite did not save."), !completed)
        }
    }

    // The row drop cue's own shape (ui/DropTarget.qml), drawn over the caption while armed.
    Rectangle {
        visible: root.armed
        anchors.fill: parent
        color: Util.alpha(Theme.color.accent, Style.hoverFillAlpha)
        border.width: Theme.spacing.hairline
        border.color: Theme.color.accent
    }
    Text {
        visible: root.armed
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        color: Theme.color.accent
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
        text: "Pin to Favorites"
    }
}
