import QtQuick
import "js/Drag.js" as DragOps

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

    keys: [DragOps.ROWS_MIME, "text/uri-list"]

    onEntered: function (drag) {
        var carrying = DragOps.hasPaths(drag.urls)
        if (!(root.switchesOnHover && carrying)
                && !DragOps.canDropInto(drag.getDataAsString(DragOps.ROWS_MIME), drag.urls, root.dest))
            drag.accepted = false
    }
    onDropped: function (drop) {
        if (DragOps.dropInto(root.pane, drop.getDataAsString(DragOps.ROWS_MIME), drop.urls, root.dest, root.destDev))
            drop.accept(Qt.CopyAction)
    }
}
