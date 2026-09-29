import QtQuick
import "." as Flea
import "js/Drag.js" as DragOps
import "js/DragOut.js" as DragOut

// A directory as a drop target, named by path: the listing's floor in ui/PaneWire.qml and each tab in
// ui/TabBar.qml. The rows keep their own DropAreas above the floor, so only what a row refuses lands
// here; ui/js/Drag.js dropInto decides the verb and sends the one transfer.
DropArea {
    id: root

    property var pane: null
    property string dest: ""
    // The destination's filesystem when it is known, 0 when it is not, which makes the drop a copy.
    property int destDev: 0
    // A tab takes every drag that carries paths, because resting on it is navigation: a drop the
    // directory would refuse, its own rows lifted back onto it, is still refused by dropInto below.
    property bool switchesOnHover: false
    // The floor and the tab being opened set this while a listing is out. The area stays able to receive the drop.
    property bool refuseLoading: false
    property string enteredMarker: ""
    property var enteredUrls: []

    keys: [DragOps.ROWS_MIME, DragOps.SHELF_MIME, "text/uri-list", "text/plain"]

    Flea.FileDrag {
        id: feedback
        pane: root.pane
    }

    function updateFeedback() {
        if (!root.containsDrag || !feedback.feedback) return
        if (DragOps.canDropInto(root.enteredMarker, root.enteredUrls, root.dest))
            feedback.showTarget(root.dest, root.destDev)
        else feedback.leaveTarget()
    }

    function leaveFeedback() {
        feedback.leaveTarget()
        root.enteredMarker = ""
        root.enteredUrls = []
    }

    onDestChanged: root.updateFeedback()
    onDestDevChanged: root.updateFeedback()

    onEntered: function (drag) {
        var marker = drag.getDataAsString(DragOps.ROWS_MIME)
        var shelf = drag.getDataAsString(DragOps.SHELF_MIME)
        var plain = drag.getDataAsString("text/plain")
        var carrying = DragOps.hasPaths(drag.urls) || (plain && DragOut.usePlain(marker, shelf))
        var allowed = DragOps.canDropInto(marker, drag.urls, root.dest, plain)
        var speak = DragOut.refusal(DragOut.sources(drag.urls, plain, marker, shelf), root.dest, marker, shelf) !== ""
        if (!(root.switchesOnHover && carrying) && !allowed && !speak) {
            drag.accepted = false
            return
        }
        root.enteredMarker = marker
        var urls = []
        for (var i = 0; i < drag.urls.length; i++) urls.push(String(drag.urls[i]))
        root.enteredUrls = urls
        // A tab may accept navigation without accepting a drop; only an eligible destination is named.
        feedback.enterTarget(marker, drag.urls, allowed ? root.dest : "",
            allowed ? root.destDev : DragOps.markerDev(marker), drag.getDataAsString(DragOps.SHELF_MIME),
            drag.proposedAction)
    }
    onPositionChanged: root.updateFeedback()
    onExited: root.leaveFeedback()
    onDropped: function (drop) {
        if (root.refuseLoading) {
            if (root.pane) root.pane.message("A directory is already loading.", false)
            root.leaveFeedback()
            return
        }
        var marker = drop.getDataAsString(DragOps.ROWS_MIME)
        var shelf = drop.getDataAsString(DragOps.SHELF_MIME)
        var plain = drop.getDataAsString("text/plain")
        var line = DragOut.refusal(DragOut.sources(drop.urls, plain, marker, shelf), root.dest, marker, shelf)
        if (line && root.pane) {
            root.pane.message(line, false)
            root.leaveFeedback()
            return
        }
        if (DragOps.dropInto(root.pane, marker, drop.urls, root.dest, root.destDev, shelf, plain, drop.proposedAction)) {
            if (root.pane && root.pane.backend) root.pane.backend.dragLanded = true
            // CopyAction: this transfer already moved or copied. MoveAction would make the source delete too.
            drop.accept(Qt.CopyAction)
        }
        root.leaveFeedback()
    }
}
