import QtQuick
import qs.Commons
import "js/Format.js" as Format
import "js/Ops.js" as Ops
import "js/Status.js" as Status

Item {
    id: root

    property string path: ""
    property int total: 0
    property int cursorIndex: 0
    property string listingState: "loading"
    property int selectionCount: 0
    property string fsName: ""
    property real fsFree: 0
    property string notice: ""
    property var errors: []
    readonly property string transient_: root.errors.length ? root.errors[0].text : root.notice
    readonly property string errorDetail: root.errors.length ? root.errors[0].detail : ""
    readonly property var stripItem: background
    readonly property var transferCard: cardLoader.item
    readonly property var countsItem: counts
    readonly property var primaryItem: primary
    readonly property var secondaryItem: secondary
    readonly property bool transientIsError: root.errors.length > 0
    property var activities: []
    property var dragFeedbackOwner: null
    readonly property var activity: root.activities.length ? root.activities[0] : null
    readonly property string sticky: root.activity ? root.activity.text : ""
    readonly property var transfer: root.activity ? root.activity.transfer : Ops.emptyTransfer()
    readonly property var transferOwner: root.activity ? root.activity.owner : null
    readonly property bool stickyHere: root.sticky.length > 0
    property string searchLine: ""
    property string searchKeys: ""
    property string retryLine: ""
    property bool searchRunning: false
    readonly property bool searching: root.searchLine.length > 0
    readonly property int spiralSize: Style.font.body
    readonly property int messageMs: 4000
    readonly property real ruleOpacity: 0.12
    readonly property bool hasUndo: !root.transientIsError && !root.stickyHere && !root.searching
                                    && root.notice.indexOf(Ops.UNDO_HINT) >= 0
    readonly property string keyHint: root.transientIsError ? "esc dismisses"
        : root.transfer.running ? (root.activity.cancelling ? "cancelling" : "esc cancels")
        : root.searchRunning ? "esc cancels" : root.searching ? root.searchKeys
        : root.hasUndo ? "z undoes" : ""
    readonly property string secondaryText: [root.keyHint,
        root.transientIsError && root.stickyHere ? root.sticky : "",
        root.activities.slice(1).map(function (entry) { return entry.text }).join(" · "),
        (root.transientIsError || root.stickyHere) && root.searching ? root.searchText() : "",
        root.transientIsError ? "" : root.retryLine]
        .filter(function (s) { return s.length > 0 }).map(function (s) { return " · " + s }).join("")
    readonly property real slotWidth: Math.max(0, root.width - Theme.spacing.rowPaddingX
        - counts.x - counts.width - 3 * Theme.spacing.gap - root.spiralSize)
    readonly property real hintWidth: hintMetrics.width
    signal transferCancelRequested(int id)
    implicitHeight: Theme.chromeHeight + detailView.height

    // Completion messages cannot acknowledge a failure; each error requires its own dismissal.
    function say(text, isError, detail) {
        if (!text) { root.dismiss(); return }
        if (isError) {
            root.errors = root.errors.concat([{text: text, detail: detail || ""}])
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

    function dismiss() {
        if (root.errors.length) root.errors = root.errors.slice(1)
        else root.notice = ""
    }

    function cancelTransfer() {
        var next = Status.cancelActivity(root.activities)
        if (next === root.activities) return
        root.activities = next
        root.transferCancelRequested(root.transfer.id)
    }

    function escapePressed() {
        if (root.transientIsError) { root.dismiss(); return true }
        if (root.transfer.running) { root.cancelTransfer(); return true }
        return false
    }

    function setActivity(owner, text, transfer) {
        root.activities = Status.activityChanged(root.activities, owner, transfer.running ? Ops.progressLine(transfer) : text, transfer)
    }

    // A completion hidden by an error or live activity keeps its full display time after acknowledgement.
    function syncNoticeTimer() {
        if (root.notice && !root.transientIsError && !root.stickyHere && !root.searching
                && root.notice.indexOf(Ops.UNDO_HINT) < 0)
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
        return root.total + (root.total === 1 ? " item" : " items")
    }

    function countText() {
        var base = root.itemText()
        return root.listingState === "ready" && root.selectionCount > 0
            ? base + " · " + root.selectionCount + " selected" : base
    }

    function fsText() {
        return root.fsName.length ? root.fsName + " · " + Format.size(root.fsFree) + " free" : ""
    }

    function searchText() { return "Search: " + root.searchLine.replace(/^Searching, /, "") }

    function slot() {
        return { transient: root.transient_, transientIsError: root.transientIsError,
                 searching: root.searching, searchKeys: root.searchText(),
                 stickyHere: root.stickyHere, sticky: root.sticky, fsText: root.fsText() }
    }

    function rightText() {
        var text = Status.rightText(root.slot())
        return root.hasUndo ? text.replace(Ops.UNDO_HINT, "") : text
    }
    function rightColor() { return Theme.color[Status.rightRole(root.slot())] }

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

    Text {
        id: counts
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: strip.verticalCenter
        width: Math.min(implicitWidth, root.width / 4)
        text: root.countText()
        color: Theme.color.foreground
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        elide: Text.ElideRight
        textFormat: Text.PlainText
    }

    Text {
        id: secondary
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: strip.verticalCenter
        width: Math.min(implicitWidth, Math.max(0, root.slotWidth
            - Math.min(primary.implicitWidth, Math.max(0, root.slotWidth - hintMetrics.width))))
        text: root.secondaryText
        color: Theme.color.muted
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        elide: Text.ElideRight
        textFormat: Text.PlainText
    }

    TextMetrics {
        id: hintMetrics
        font: secondary.font
        text: root.keyHint.length ? " · " + root.keyHint
            + (root.secondaryText !== " · " + root.keyHint ? " · …" : "") : ""
    }

    Text {
        id: primary
        anchors.right: secondary.left
        anchors.verticalCenter: strip.verticalCenter
        width: Math.min(implicitWidth, Math.max(0, root.slotWidth - secondary.width))
        text: root.rightText()
        color: root.rightColor()
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        elide: Text.ElideMiddle
        textFormat: Text.PlainText
    }

    Spinner {
        visible: !root.transientIsError && (root.stickyHere || root.searchRunning)
        anchors.right: primary.left
        anchors.rightMargin: Theme.spacing.gap
        anchors.verticalCenter: strip.verticalCenter
        width: root.spiralSize
        height: root.spiralSize
        color: Theme.color.muted
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
        Text {
            id: detailText
            x: Theme.spacing.rowPaddingX
            y: Theme.spacing.rowPaddingY
            width: parent.width - 2 * Theme.spacing.rowPaddingX
            text: root.errorDetail
            color: Theme.color.error
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
