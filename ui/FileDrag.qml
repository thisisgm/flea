import QtQuick
import "js/Drag.js" as DragOps
import "js/Ops.js" as Ops

// The platform drag belongs to the view: a tab switch may destroy the pressed delegate inside QDrag.exec().
Item {
    id: root

    required property var pane
    property var dragRows: []
    property int dropIndex: -1
    property bool dragCopy: false
    property var dragMime: ({})
    property var feedback: null

    Drag.dragType: Drag.Automatic
    // Foreign applications see copy only; Flea resolves its own move from the source identity and device.
    Drag.supportedActions: Qt.CopyAction
    Drag.proposedAction: Qt.CopyAction
    Drag.mimeData: root.dragMime
    Drag.onDragFinished: root.liftEnded()

    function liftBegan(index, centroid) {
        if (!root.pane || root.pane.listInFlight || index < 0 || !root.pane.rowFor(index)) return
        root.dragRows = DragOps.carried(root.pane, index)
        root.dropIndex = -1
        root.liftMoved(centroid)
        root.dragMime = DragOps.mimeFor(root.pane, root.dragRows, root.dragCopy)
        root.feedback = DragOps.feedbackFor(root.dragMime[DragOps.ROWS_MIME],
            (root.dragMime["text/uri-list"] || "").split("\r\n"))
        root.showTarget("", root.feedback.dev)
        // This enters the platform event loop; every payload field must already be fixed.
        root.Drag.active = true
    }

    function liftMoved(centroid) {
        root.dragCopy = DragOps.copying(centroid.modifiers)
    }

    function verbAt(marker, row) {
        return DragOps.verbFor(DragOps.isOwnDrag(marker), DragOps.markerCopying(marker),
                               DragOps.markerDev(marker), row ? row.v : 0)
    }

    function liftEnded() {
        root.Drag.active = false
        root.dragRows = []
        root.dragMime = ({})
        root.dropIndex = -1
        root.dragCopy = false
        root.feedback = null
        // A different view may now speak for this gesture; its activity is separate from every transfer.
        var bar = root.pane ? root.pane.statusBar : null
        if (bar && bar.dragFeedbackOwner) bar.dragFeedbackOwner.say("")
    }

    function say(text) {
        var bar = root.pane ? root.pane.statusBar : null
        if (!bar) return
        if (!text) {
            if (bar.dragFeedbackOwner === root) {
                bar.dragFeedbackOwner = null
                bar.setActivity(root, "", Ops.emptyTransfer())
            }
            return
        }
        if (bar.activities.some(function (entry) { return entry.transfer.running })) return
        if (bar.dragFeedbackOwner && bar.dragFeedbackOwner !== root)
            bar.setActivity(bar.dragFeedbackOwner, "", Ops.emptyTransfer())
        bar.dragFeedbackOwner = root
        bar.setActivity(root, text, Ops.emptyTransfer())
    }

    function enterTarget(marker, urls, name, destDev) {
        root.feedback = DragOps.feedbackFor(marker, urls)
        root.showTarget(name, destDev)
    }

    function showTarget(name, destDev) {
        if (!root.feedback) return
        root.dragCopy = DragOps.verbFor(root.feedback.own, root.feedback.copy, root.feedback.dev, destDev) === "copy"
        root.say(DragOps.feedbackLine(root.feedback, name, destDev))
    }

    function leaveTarget() {
        var bar = root.pane ? root.pane.statusBar : null
        if (!bar || bar.dragFeedbackOwner !== root) return
        root.say(root.feedback && root.feedback.own
            ? DragOps.feedbackLine(root.feedback, "", root.feedback.dev) : "")
    }

    Connections {
        target: root.pane ? root.pane.statusBar : null
        function onActivitiesChanged() {
            var bar = root.pane.statusBar
            if (bar.dragFeedbackOwner === root
                    && bar.activities.some(function (entry) { return entry.transfer.running })) root.say("")
        }
    }
}
