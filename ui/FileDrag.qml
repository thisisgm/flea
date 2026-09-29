import QtQuick
import "js/Drag.js" as DragOps
import "js/DragOut.js" as DragOut
import "js/Ops.js" as Ops

// The platform drag belongs to the view: a tab switch may destroy the pressed delegate inside QDrag.exec().
Item {
    id: root

    required property var pane
    property var dragRows: []
    // The numbering dragRows were read in, fixed at the lift so a drop after a re-list is refused rather than resolved anew.
    property real dragListing: 0
    property int dropIndex: -1
    property bool dragCopy: false
    property bool dragShift: false
    property bool awaitingPaths: false
    property bool buttonUp: false
    property var dragMime: ({})
    property var feedback: null

    Drag.dragType: Drag.Automatic
    // A plain lift offers both. Files then moves on the same device and copies across.
    // Ctrl offers copy alone and Shift offers move alone: a receiver that takes move
    // whenever it is offered would otherwise ignore the key held at the lift.
    // Chromium prefers move when it is offered, and an uploader may refuse that drag.
    Drag.supportedActions: root.dragCopy ? Qt.CopyAction : root.dragShift ? Qt.MoveAction : (Qt.CopyAction | Qt.MoveAction)
    Drag.proposedAction: root.dragCopy ? Qt.CopyAction : Qt.MoveAction
    Drag.mimeData: root.dragMime
    Drag.onDragFinished: function (dropAction) { root.liftEnded(dropAction) }

    function liftBegan(index, centroid) {
        if (!root.pane || root.pane.listInFlight || index < 0 || !root.pane.rowFor(index)) return
        root.buttonUp = false
        root.awaitingPaths = false
        root.dragRows = DragOps.carried(root.pane, index)
        root.dragListing = root.pane.backend ? root.pane.backend.heldListing : 0
        root.dropIndex = -1
        // Ctrl is the threshold's, and a paths reply must not sample it again.
        root.dragCopy = DragOps.copying(centroid.modifiers)
        root.dragShift = DragOps.shifting(centroid.modifiers)
        root.dragMime = DragOps.mimeFor(root.pane, root.dragRows, root.dragCopy, root.dragShift)
        if (root.dragMime["text/uri-list"]) {
            root.startOffer()
            return
        }
        if (!root.pane.backend || !DragOut.askAllowed(root.pane.pathsPending, root.pane.clipPending)) {
            root.cannotLeave()
            return
        }
        root.awaitingPaths = true
        var dev = root.pane.backend.dirDev || 0
        root.feedback = { own: true, copy: root.dragCopy, shift: root.dragShift, dev: dev,
                          deletable: DragOps.listingDeletable(root.pane), count: root.dragRows.length, canLeave: true }
        root.say(DragOps.feedbackLine(root.feedback, "", dev))
        var rows = root.dragRows.slice()
        var listing = root.dragListing
        var copy = root.dragCopy
        var shift = root.dragShift
        root.pane.pathsPending = {
            kind: "drag", rows: rows, listing: listing, copy: copy, shift: shift,
            deliver: function (list, pending) { root.deliverPaths(list, pending) }
        }
        root.pane.backend.send({ c: "paths", rows: rows, listing: listing })
    }

    function liftReleased() {
        root.buttonUp = true
    }

    function liftMoved(centroid) {
        if (root.awaitingPaths || root.Drag.active) return
        root.dragCopy = DragOps.copying(centroid.modifiers)
        root.dragShift = DragOps.shifting(centroid.modifiers)
    }

    function startOffer() {
        root.feedback = DragOps.feedbackFor(root.dragMime[DragOps.ROWS_MIME],
            (root.dragMime["text/uri-list"] || "").split("\r\n"))
        root.showTarget("", root.feedback.dev)
        // This enters the platform event loop; every payload field must already be fixed.
        root.Drag.active = true
    }

    function deliverPaths(list, pending) {
        root.awaitingPaths = false
        var held = root.pane && root.pane.backend ? root.pane.backend.heldListing : 0
        if (root.buttonUp || held !== pending.listing || !list || list.length !== pending.rows.length) {
            root.cannotLeave()
            return
        }
        root.dragCopy = pending.copy
        root.dragShift = pending.shift === true
        root.dragMime = root.mimeForPaths(pending.rows, list, pending.copy, root.dragShift)
        root.startOffer()
    }

    function mimeForPaths(rows, paths, copy, shift) {
        var mime = {}
        var dev = root.pane.backend ? root.pane.backend.dirDev : 0
        mime[DragOps.ROWS_MIME] = DragOps.markerPayload(rows, copy, root.pane.path, dev, DragOps.listingDeletable(root.pane), shift === true)
        var uris = []
        for (var i = 0; i < paths.length; i++) uris.push(DragOps.uriFor(paths[i]))
        mime["text/uri-list"] = uris.join("\r\n") + "\r\n"
        mime["text/plain"] = paths.join("\n")
        return mime
    }

    function cannotLeave() {
        root.say(DragOps.line(root.dragRows.length, "", root.dragCopy) + DragOps.reachNote(false))
    }

    function verbAt(marker, row) {
        return DragOps.verbFor(DragOps.isOwnDrag(marker), DragOps.markerCopying(marker),
                               DragOps.markerShift(marker), DragOps.markerDev(marker),
                               row ? row.v : 0, DragOps.markerDeletable(marker))
    }

    function liftEnded(dropAction) {
        var landed = !!(root.pane && root.pane.backend && root.pane.backend.dragLanded)
        // Files moves a uri-list after it accepts the drag. Deleting on that acceptance
        // puts the files in Trash before Files has read them. releaseDeletes stays false.
        DragOps.releaseDeletes(dropAction, landed)
        if (root.pane && root.pane.backend) root.pane.backend.dragLanded = false
        root.Drag.active = false
        root.dragRows = []
        root.dragListing = 0
        root.dragMime = ({})
        root.dropIndex = -1
        root.dragCopy = false
        root.dragShift = false
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

    function enterTarget(marker, urls, name, destDev, shelf, proposed) {
        root.feedback = DragOps.feedbackFor(marker, urls, shelf, proposed)
        root.showTarget(name, destDev)
    }

    function showTarget(name, destDev) {
        if (!root.feedback) return
        root.dragCopy = DragOps.copyingFor(root.feedback, destDev)
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
