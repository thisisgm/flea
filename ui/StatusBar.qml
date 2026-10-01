import QtQuick
import qs.Commons
import "." as Flea
import "js/Format.js" as Format
import "js/Filter.js" as Filter
import "js/Focus.js" as Focus
import "js/Ops.js" as Ops
import "js/Status.js" as Status
import "js/Trash.js" as Trash

Item {
    id: root

    property string path: ""
    property int total: 0
    property string listingState: "loading"
    // The pane whose listing this strip is counting, or null while the trash view owns the pane.
    property var pane: null
    readonly property int shownTotal: root.pane ? root.pane.shownTotal : root.total
    // selectionVersion is read here so a selection mutated in place still re-runs this binding.
    readonly property real selectionBytes: root.pane && root.pane.selectionVersion >= 0
        ? Status.selectionBytes(root.pane) : -1
    property int selectionCount: 0
    property string fsName: ""
    property real fsFree: 0
    property string notice: ""
    // [{text, detail, place}]: place is the path a no-Trash refusal was raised in, "" for every other error.
    property var errors: []
    // An arm's prompt and its record, kept apart from the notice so the arm's own life, not the text, ranks it.
    property string prompt: ""
    property var arm: null
    // The arm's owner: the listing pane here, the Trash view while ui/WindowBody.qml has that view open.
    property var armOwner: root.pane
    readonly property bool armLive: Status.armLive(root.arm, root.armOwner)
    // The owner zeroing or replacing its own stamp ends the prompt: the second press, any other key, a new listing.
    Connections {
        target: root.arm !== null ? root.arm.owner : null
        function onTrashArmedAtChanged() { if (root.arm !== null && root.arm.owner.trashArmedAt !== root.arm.stamp) root.endArm() }
    }
    onArmOwnerChanged: if (root.arm !== null && root.arm.owner !== root.armOwner) root.loseArm()
    readonly property var shown: Status.transientOf(root.errors, root.notice, root.prompt, root.armLive)
    readonly property string transient_: root.shown.text
    readonly property string errorDetail: root.shown.detail
    // Directive 48, GM's final: no centre lane at all. The transient sits beside the disk facts, one
    // padding before the text those facts actually draw, which is not the left edge of their fixed
    // zone box: a short filesystem line would otherwise leave the pair drifting apart by the slack.
    readonly property real diskTextWidth: Math.min(disk.implicitWidth, disk.width)
    // From the count's right edge, one padding on, to one padding before that text. A transient elides
    // inside this rather than growing into the facts, which is rule 2: they keep their zone and never move.
    readonly property real transientRoom: Math.max(0, strip.width - 2 * Theme.spacing.rowPaddingX - root.diskTextWidth
        - (counts.x + counts.width + Theme.spacing.rowPaddingX))
    readonly property var stripItem: background
    readonly property var transferCard: cardLoader.item
    readonly property var countsItem: counts
    readonly property var primaryItem: primary
    readonly property var secondaryItem: secondary
    readonly property var undoItem: undoLink
    readonly property var diskItem: disk
    readonly property var centreItem: centre
    readonly property bool transientIsError: root.shown.isError
    property var activities: []
    // The bridge board's busy mark walks while a Starting line stands, and only then.
    property bool busy: false
    property var dragFeedbackOwner: null
    readonly property var activity: root.activities.length ? root.activities[0] : null
    // StatusBar rule 8: the card owns a transfer's progress and it is up whenever one runs, so the
    // strip draws nothing for it at all. A drag's own feedback is not a transfer and still reports.
    readonly property string sticky: root.activity && !root.activity.transfer.running ? root.activity.text : ""
    readonly property var transfer: root.activity ? root.activity.transfer : Ops.emptyTransfer()
    readonly property var transferOwner: root.activity ? root.activity.owner : null
    readonly property bool stickyHere: root.sticky.length > 0
    property string searchLine: ""
    property string retryLine: ""
    property bool searchRunning: false
    readonly property bool searching: root.searchLine.length > 0
    readonly property int messageMs: 4000
    readonly property real ruleOpacity: 0.12
    // The hint a result carries: the primary drops it and the secondary draws it, so no sentence on
    // this strip ends in advice. ui/js/Status.js owns the two of them.
    readonly property string noticeHint: root.stickyHere || root.searching
                                         ? "" : Status.hintOf(root.transient_)
    readonly property bool hasUndo: Status.hasUndoHint(root.noticeHint)
    // Durability callout 6: "z undoes" takes a click, the same as z, on every line that carries
    // it. The undo segment draws as its own target while the rest draws as today.
    readonly property bool undoSplit: root.hasUndo
    readonly property string keyHintRest: root.undoSplit ? Status.withoutUndoKey(root.keyHint) : root.keyHint
    // Round two, StatusBar rule 4: a refusal is drawn alone. When the strip's error is the pane's own
    // state sentence, the block under it is already saying so and the key is not information.
    readonly property string keyHint: root.transientIsError
        ? [root.pane && root.errors[0].text === root.pane.stateMessage ? "" : "esc dismisses",
           root.noticeHint.length > 0 ? Status.hintKey(root.noticeHint) : ""]
            .filter(function (s) { return s.length > 0 }).join(" · ")
        : root.noticeHint.length > 0 ? Status.hintKey(root.noticeHint) : ""
    readonly property string secondaryText: Status.secondaryText(root.keyHint, root.transientIsError,
        root.stickyHere, root.sticky, root.activities, root.searching, root.searchLine, root.retryLine)
    readonly property string restText: Status.secondaryText(root.keyHintRest, root.transientIsError,
        root.stickyHere, root.sticky, root.activities, root.searching, root.searchLine, root.retryLine)
    // Three zones that never trade places: the board fixes the outer two at a third of the strip
    // each, so the centre stays put however long the count or the disk line gets.
    readonly property real zoneSpan: Math.max(0, root.width - 2 * Theme.spacing.rowPaddingX)
    readonly property real zoneWidth: Math.round(root.zoneSpan / 3)
    readonly property real hintWidth: hintMetrics.advanceWidth // Advance keeps the hint's edge space; bounding width drops it, 114 against 115.
    signal transferCancelRequested(int id)
    implicitHeight: Theme.chromeHeight + detailView.height

    // Completion messages cannot acknowledge a failure; each error requires its own dismissal.
    function say(text, isError, detail) {
        if (!text) { root.dismiss(); return }
        if (!isError && Status.isPrompt(text)) { root.startArm(text); return }
        if (isError) {
            root.errors = Status.withError(root.errors, text, detail, root.path)
            return
        }
        root.notice = text
        root.syncNoticeTimer()
    }

    // A surface replacing its own standing verdict names the one it is replacing. say("") dismisses
    // the head of the queue, which is somebody else's error whenever more than one is waiting.
    function forget(text) {
        if (!text) return
        root.errors = root.errors.filter(function (entry) { return entry.text !== text })
    }

    // Dismisses the head error when one is drawn, else the notice; a live prompt belongs to its arm.
    function dismiss() {
        if (root.transientIsError) root.errors = root.errors.slice(1)
        else root.notice = ""
    }
    // A refusal and an arm belong to the place they were raised in, so leaving that place drops both.
    onPathChanged: {
        var kept = Status.errorsAt(root.errors, root.path)
        if (kept !== root.errors) root.errors = kept
        if (root.arm !== null) root.loseArm()
    }

    // The prompt supersedes the notice it covers. One said before its owner stamped could never be ended by the arm, so it is only a notice, and said loudly.
    function startArm(text) {
        var stamp = root.armOwner ? root.armOwner.trashArmedAt : 0
        if (!(stamp > 0)) {
            console.warn("StatusBar: \"" + text + "\" was said before its owner armed, so it is only a notice")
            root.notice = text
            root.syncNoticeTimer()
            return
        }
        root.notice = ""
        root.prompt = text
        root.arm = Status.armOf(root.armOwner, stamp)
        armClock.interval = Math.max(1, Status.armLeft(root.arm, Date.now(), Trash.ARM_MS))
        armClock.restart()
        root.syncNoticeTimer()
    }
    function endArm() {
        armClock.stop()
        root.arm = null
        root.prompt = ""
        root.syncNoticeTimer()
    }
    // The bar ending an arm (its owner lost, its place left, its clock run out) ends it at the owner too, so no key completes a hidden arm.
    function loseArm() {
        var arm = root.arm
        root.endArm()
        if (arm !== null && arm.owner && arm.owner.trashArmedAt === arm.stamp) arm.owner.trashArmedAt = 0
    }
    Timer {
        id: armClock
        onTriggered: {
            var left = Status.armLeft(root.arm, Date.now(), Trash.ARM_MS)
            if (left > 0) { armClock.interval = left; armClock.restart() } else root.loseArm()
        }
    }

    function cancelTransfer() {
        var next = Status.cancelActivity(root.activities)
        if (next === root.activities) return
        root.activities = next
        root.transferCancelRequested(root.transfer.id)
    }

    // The click refuses through the gate it shares with the z key.
    function clickUndo() {
        if (!Focus.canUndo(root.pane, root.pane ? root.pane.sidebar : null)) return
        Ops.undo(root.pane)
    }

    function escapePressed() {
        if (root.transientIsError) { root.dismiss(); return true }
        // A final flush cannot be cancelled: every file is already complete.
        if (root.transfer.running && root.transfer.writing !== true) { root.cancelTransfer(); return true }
        return false
    }

    function setActivity(owner, text, transfer) {
        root.activities = Status.activityChanged(root.activities, owner, transfer.running ? Ops.progressLine(transfer) : text, transfer)
        root.busy = Status.displayedStarting(root.activities)
    }

    // A completion hidden by an error or live activity keeps its full display time after acknowledgement.
    function syncNoticeTimer() {
        if (root.notice && Status.noticeShown(root.slot()) && root.notice.indexOf(Status.UNDO_HINT) < 0)
            clear.restart()
        else clear.stop()
    }
    onTransientIsErrorChanged: root.syncNoticeTimer()
    onStickyHereChanged: root.syncNoticeTimer()
    onSearchingChanged: root.syncNoticeTimer()

    function itemText() {
        if (root.listingState === "empty") return "empty"
        if (root.listingState === "error" || root.listingState === "locked") return "unavailable"
        if (root.listingState !== "ready") return ""
        return Format.count(root.total) + (root.total === 1 ? " item" : " items")
    }

    // The left zone answers the question the view raises: the selection if there is one, what the
    // filter left standing if there is one, and otherwise the directory. StatusBar board rule 3.
    function countText() {
        if (root.listingState !== "ready") {
            return root.itemText()
        }
        if (root.selectionCount > 0) {
            var head = Format.count(root.selectionCount) + " of " + Format.count(root.total) + " selected"
            return root.selectionBytes >= 0 ? head + " · " + Format.size(root.selectionBytes) : head
        }
        // SearchFilter rule 2: the same sentence the strip carries, scope and all, because the count
        // is of the rows the filter could see and those are a window on the directory, not all of it.
        // The trash view and the picker both draw this strip with no pane behind it, so the filter's
        // sentence is asked for only where there is a listing to have filtered.
        if (root.pane && root.shownTotal !== root.total) {
            return Filter.summary(root.pane.shown, root.pane.rows.length, root.total)
        }
        return root.itemText()
    }

    // The one fact on this strip a result or an error may not evict, so a pane with no answer for
    // it says unknown rather than describing the filesystem the pane just failed to leave.
    function fsText() {
        return root.path === "flea:stack" ? "" : (root.fsName.length ? root.fsName + " · " + Format.size(root.fsFree) + " free" : "unknown")
    }

    function slot() {
        return { transient: root.transient_, transientIsError: root.transientIsError, armLive: root.armLive,
                 searching: root.searching, searchLine: root.searchLine,
                 stickyHere: root.stickyHere, sticky: root.sticky }
    }

    function centreText() {
        var text = Status.centreText(root.slot())
        return root.noticeHint.length > 0 ? text.replace(root.noticeHint, "") : text
    }
    // Centre ink sits on the surface, so error takes the surface lift and any other role its own.
    function centreColor() {
        var role = Status.centreRole(root.slot())
        if (role === "error")
            return Theme.color.errorOnSurface
        return Theme.color[role] !== undefined ? Theme.color[role] : Theme.color.foreground
    }

    Timer { id: clear; interval: root.messageMs; onTriggered: root.notice = "" }

    Item { id: strip; width: parent.width; height: Theme.chromeHeight }

    Rectangle {
        id: background
        width: parent.width
        height: Theme.chromeHeight
        color: Theme.color.surface
        border.width: 0
    }

    Rectangle {
        width: parent.width
        height: Theme.spacing.hairline
        color: Theme.color.foreground
        opacity: root.ruleOpacity
    }

    // Left: what is in front of you.
    Text {
        id: counts
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: strip.verticalCenter
        width: root.zoneWidth
        text: root.countText()
        color: Theme.color.foreground
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        elide: Text.ElideRight
        textFormat: Text.PlainText
    }

    // Right: the disk, which no result and no error may take the space of.
    Text {
        id: disk
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: strip.verticalCenter
        width: root.zoneWidth
        horizontalAlignment: Text.AlignRight
        text: root.fsText()
        color: Theme.color.foreground
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        elide: Text.ElideRight
        textFormat: Text.PlainText
    }

    // Right, before the disk facts: what just happened, ending one padding short of their own text.
    Row {
        id: centre
        anchors.right: parent.right
        anchors.rightMargin: 2 * Theme.spacing.rowPaddingX + root.diskTextWidth
        anchors.verticalCenter: strip.verticalCenter
        // Rule 2: the disk keeps its zone whatever the transient says, so the pair share this and elide
        // inside it rather than growing into the facts beside them.
        readonly property real room: root.transientRoom
        readonly property real busyRoom: root.busy ? busyLoader.width + centre.spacing : 0
        spacing: root.busy ? Theme.spacing.gap : 0

        // The bridge board's busy mark, built only while a Starting line stands.
        Loader {
            id: busyLoader
            active: root.busy
            visible: root.busy
            anchors.verticalCenter: parent.verticalCenter
            width: root.busy ? Theme.chromeMarkSize : 0
            height: Theme.chromeMarkSize
            sourceComponent: Flea.Spinner {
                width: Theme.chromeMarkSize
                height: Theme.chromeMarkSize
                color: Theme.color.muted
            }
        }

        Text {
            id: primary
            anchors.verticalCenter: parent.verticalCenter
            text: root.centreText()
            color: root.centreColor()
            width: Math.min(implicitWidth, Math.max(0, centre.room - secondary.width - undoDot.width - undoLink.width - centre.busyRoom))
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            elide: Text.ElideMiddle
            textFormat: Text.PlainText
        }

        Text {
            id: secondary
            anchors.verticalCenter: parent.verticalCenter
            text: root.restText
            color: Theme.color.muted
            width: Math.min(implicitWidth, Math.max(0, centre.room - undoDot.width - undoLink.width - Math.min(primary.implicitWidth,
                Math.max(0, centre.room - root.hintWidth))))
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            elide: Text.ElideRight
            textFormat: Text.PlainText
        }

        // The undo target: muted at rest like the hint it was, foreground under the pointer while
        // its middot stays muted. Empty unless split, so no hidden text is held.
        Text {
            id: undoDot
            anchors.verticalCenter: parent.verticalCenter
            visible: root.undoSplit
            width: visible ? implicitWidth : 0
            text: root.undoSplit ? " · " : ""
            color: Theme.color.muted
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            textFormat: Text.PlainText
        }

        Text {
            id: undoLink
            anchors.verticalCenter: parent.verticalCenter
            visible: root.undoSplit
            width: visible ? implicitWidth : 0
            text: root.undoSplit ? "z undoes" : ""
            color: undoHover.hovered ? Theme.color.foreground : Theme.color.muted
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            textFormat: Text.PlainText
            Accessible.role: Accessible.Link
            Accessible.name: "z undoes"
            Accessible.onPressAction: root.clickUndo()
            HoverHandler { id: undoHover; cursorShape: Qt.PointingHandCursor }
            TapHandler {
                acceptedButtons: Qt.LeftButton
                gesturePolicy: TapHandler.ReleaseWithinBounds
                onTapped: root.clickUndo()
            }
        }
    }

    TextMetrics {
        id: hintMetrics
        font: secondary.font
        text: root.keyHint.length ? " · " + root.keyHint
            + (root.secondaryText !== " · " + root.keyHint ? " · …" : "") : ""
    }

    Rectangle {
        x: detailView.x; y: detailView.y
        width: detailView.width; height: detailView.height
        visible: detailView.visible
        color: Theme.color.surface
        border.width: 0
    }
    Flickable {
        id: detailView
        y: Theme.chromeHeight
        width: parent.width
        visible: root.errorDetail.length > 0
        height: visible ? Math.min(contentHeight, root.parent ? root.parent.height / 3 : contentHeight) : 0
        contentWidth: width
        contentHeight: detailText.implicitHeight + 2 * Theme.spacing.rowPaddingY
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        onVisibleChanged: contentY = 0
        Flea.ViewportScrollBar {
            parent: detailView
            anchors { top: parent.top; right: parent.right }
            flickable: detailView
        }
        Text {
            id: detailText
            x: Theme.spacing.rowPaddingX
            y: Theme.spacing.rowPaddingY
            width: parent.width - 2 * Theme.spacing.rowPaddingX
            text: root.errorDetail
            color: Theme.color.errorOnSurface
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
        }
    }

    Loader {
        id: cardLoader
        active: root.transfer.running
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.bottom: parent.top
        anchors.bottomMargin: Theme.spacing.gap
        sourceComponent: TransferCard {
            width: Math.min(implicitWidth, Math.max(0, root.width - 2 * Theme.spacing.rowPaddingX))
            transfer: root.transfer
            owner: root.transferOwner
            cancelling: root.activity ? root.activity.cancelling : false
            onCancelRequested: root.cancelTransfer()
        }
    }
}
