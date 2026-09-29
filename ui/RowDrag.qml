import QtQuick
import "js/Drag.js" as DragOps
import "js/DragOut.js" as DragOut

// Delegate input and drop eligibility share a view-owned FileDrag; no platform object lives in this row.
Item {
    id: root

    required property var session
    required property int listingIndex
    required property var row
    readonly property var pane: root.session.pane

    anchors.fill: parent
    enabled: root.pane !== null && !root.pane.listInFlight && root.listingIndex >= 0 && !!root.row

    DragHandler {
        id: lift
        enabled: root.pane !== null && !root.pane.renamePending && root.pane.renamingIndex !== root.listingIndex
        target: null
        // Preserve List's grab contract: the Flickable must not take the row drag after its threshold.
        grabPermissions: PointerHandler.CanTakeOverFromItems | PointerHandler.CanTakeOverFromHandlersOfDifferentType | PointerHandler.ApprovesTakeOverByHandlersOfSameType
        onActiveChanged: {
            if (active) root.session.liftBegan(root.listingIndex, lift.centroid)
            else root.session.liftReleased()
        }
        onCentroidChanged: if (active) root.session.liftMoved(lift.centroid)
    }

    DropArea {
        anchors.fill: parent
        keys: [DragOps.ROWS_MIME, "text/uri-list", "text/plain"]
        onEntered: function (drag) {
            var marker = drag.getDataAsString(DragOps.ROWS_MIME)
            var plain = drag.getDataAsString("text/plain")
            var ok = root.row && root.row.d === true
            if (ok)
                ok = DragOps.hasPaths(drag.urls) || (plain && !marker)
                   ? DragOps.canDropInto(marker, drag.urls, root.pane.join(root.pane.path, root.row.n), plain)
                     || DragOut.refusal(DragOut.sources(drag.urls, plain, marker, ""), root.pane.join(root.pane.path, root.row.n), marker, "") !== ""
                   : DragOps.canDropByIndex(marker, root.pane.path, root.session.dragRows, root.listingIndex)
            if (!ok) {
                drag.accepted = false
                return
            }
            root.session.dropIndex = root.listingIndex
            root.session.enterTarget(marker, drag.urls, root.row.n, root.row.v,
                                     drag.getDataAsString(DragOps.SHELF_MIME), drag.proposedAction)
        }
        onPositionChanged: function (drag) {
            if (root.session.dropIndex === root.listingIndex)
                root.session.showTarget(root.row.n, root.row.v)
        }
        onExited: {
            if (root.session.dropIndex === root.listingIndex) {
                root.session.dropIndex = -1
                root.session.leaveTarget()
            }
            if (root.session.dragRows.length === 0) root.session.dragCopy = false
        }
        onDropped: function (drop) {
            var marker = drop.getDataAsString(DragOps.ROWS_MIME)
            var shelf = drop.getDataAsString(DragOps.SHELF_MIME)
            var plain = drop.getDataAsString("text/plain")
            if (root.row && root.row.d === true) {
                var dest = root.pane.join(root.pane.path, root.row.n)
                var line = DragOut.refusal(DragOut.sources(drop.urls, plain, marker, shelf), dest, marker, shelf)
                if (line) {
                    root.pane.message(line, false)
                    return
                }
            }
            if (root.dropped(marker, drop.urls, shelf, plain, drop.proposedAction)) {
                if (root.pane.backend) root.pane.backend.dragLanded = true
                drop.accept(Qt.CopyAction)
            }
        }
    }

    // The drop apart from its platform event, which tests/js/collide.js cannot build: answers whether it was taken.
    function dropped(marker, urls, shelf, plain, proposed) {
        var accepted = false
        if (root.row && root.row.d === true) {
            if (DragOps.hasPaths(urls) || (plain && !marker && !shelf))
                accepted = DragOps.dropInto(root.pane, marker, urls, root.pane.join(root.pane.path, root.row.n), root.row.v, shelf, plain, proposed)
            else if (DragOps.canDropByIndex(marker, root.pane.path, root.session.dragRows, root.listingIndex))
                accepted = DragOps.drop(root.pane, root.session.dragRows, root.listingIndex,
                    root.session.verbAt(marker, root.row) === "copy", root.session.dragListing)
        }
        root.session.dropIndex = -1
        root.session.dragCopy = false
        root.session.leaveTarget()
        return accepted
    }
}
