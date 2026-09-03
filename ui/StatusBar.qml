import QtQuick
import qs.Commons
import "js/Format.js" as Format
import "js/Ops.js" as Ops

Item {
    id: root

    property string path: ""
    property int total: 0
    property int cursorIndex: 0
    property string listingState: "loading"
    property int selectionCount: 0
    // The filesystem under the current directory, from the backend's own statfs; empty draws nothing.
    property string fsName: ""
    property real fsFree: 0

    property string transient_: ""
    property bool transientIsError: false

    // A running operation's line, which unlike transient_ does not time out: it stands until the
    // operation ends and replaces it with its own result. Compress, extract, convert and the
    // Dropbox move have no card and say what they are doing here.
    property string sticky: ""

    // The transfer the card draws, pushed in by whoever owns the scene. The card owns the transfer
    // while it runs and this bar keeps the result it ends with, so the two halves of one operation
    // are never said twice at once; left idle, which is what it is until ui/shell.qml binds it.
    property var transfer: Ops.emptyTransfer()
    signal transferCancelRequested(int id)

    // The card takes the transfer's own line off this bar while it runs; every other sticky
    // operation keeps it, because none of them has a card.
    readonly property bool stickyHere: root.sticky.length > 0 && !root.transfer.running

    // The search's own two halves, the lines the design canvas draws; empty means the bar is its normal self.
    property string searchLine: ""
    property string searchKeys: ""
    property bool searchRunning: false
    readonly property bool searching: root.searchLine.length > 0
    // Operations.dc.html and Search.dc.html both draw the bar's crawl at 14, the base size, two above the caption the bar's text is set in.
    readonly property int spiralSize: Style.font.body

    readonly property int messageMs: 4000
    readonly property real ruleOpacity: 0.12

    // A status strip is chrome, not a data row; see Theme.qml's chromeHeight comment.
    implicitHeight: Theme.chromeHeight

    function say(text, isError) {
        root.transient_ = text
        root.transientIsError = isError
        // Errors and the undo hint stay until the next line or Escape. A 4 s toast was the only
        // place many failures were explained, and z undoes is easy to miss if it vanishes.
        if (!text || isError || text.indexOf(Ops.UNDO_HINT) >= 0) {
            clear.stop()
            return
        }
        clear.restart()
    }

    // The end of an operation: the sticky line goes and its result takes the ordinary transient slot.
    function settle(text, isError) {
        root.sticky = ""
        root.say(text, isError)
    }

    // The canvas's own left half: how many rows there are, and how many of them are picked.
    function countText() {
        if (root.listingState === "empty") {
            return "empty"
        }
        if (root.listingState === "error" || root.listingState === "locked") {
            return "unavailable"
        }
        if (root.listingState === "ready") {
            var base = root.total + (root.total === 1 ? " item" : " items")
            // Say nothing about the selection while it is empty, the same idiom the rest of the bar uses.
            return root.selectionCount > 0 ? base + "   " + root.selectionCount + " selected" : base
        }
        return ""
    }

    // The canvas's own right half: "btrfs · 412 GB free". A filesystem the backend could not read
    // draws nothing rather than a zero, because a wrong number is worse than no number.
    function fsText() {
        if (root.fsName.length === 0) {
            return ""
        }
        return root.fsName + " · " + Format.size(root.fsFree) + " free"
    }

    // A running operation outranks the transient slot, which outranks the standing count.
    function rightText() {
        if (root.searching) {
            return root.searchKeys
        }
        if (root.stickyHere) {
            return root.sticky
        }
        return root.transient_.length > 0 ? root.transient_ : root.fsText()
    }

    function rightColor() {
        if (!root.searching && root.stickyHere) {
            return Theme.color.foreground
        }
        if (root.transient_.length > 0 && root.transientIsError) {
            return Theme.color.error
        }
        return Theme.color.muted
    }

    Timer {
        id: clear
        interval: root.messageMs
        repeat: false
        onTriggered: root.transient_ = ""
    }

    Rectangle {
        anchors.fill: parent
        color: Theme.color.surface
    }

    Rectangle {
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        height: Theme.spacing.hairline
        color: Theme.color.foreground
        opacity: root.ruleOpacity
    }

    // The crawl only turns while the walk does, and it is the status bar's own transfer-mark slot.
    Spinner {
        id: crawl
        visible: root.searching && root.searchRunning
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        width: root.spiralSize
        height: root.spiralSize
        color: Theme.color.muted
    }

    Text {
        visible: root.searching
        anchors.left: root.searchRunning ? crawl.right : parent.left
        anchors.leftMargin: root.searchRunning ? Theme.spacing.gap : Theme.spacing.rowPaddingX
        anchors.right: right.left
        anchors.rightMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        text: root.searchLine
        color: Theme.color.foreground
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        elide: Text.ElideRight
        textFormat: Text.PlainText
    }

    // The path used to live here; the canvas gives that job to the window chrome and this half to
    // the counts, so the two halves of the bar say what there is and what there is room for.
    Text {
        visible: !root.searching
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.right: right.left
        anchors.rightMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        text: root.countText()
        color: Theme.color.muted
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        elide: Text.ElideRight
        textFormat: Text.PlainText
    }

    // The transfer mark: the same crawl the search walk draws, in the transient slot rather than the
    // left one, and it appears and disappears with the sticky line so nothing else in the bar moves.
    Spinner {
        id: transferMark
        visible: root.stickyHere && !root.searching
        anchors.right: right.left
        anchors.rightMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        width: root.spiralSize
        height: root.spiralSize
        color: Theme.color.muted
    }

    Text {
        id: right
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        text: root.rightText()
        // A sticky line is the app saying what it is doing right now, so it reads at full contrast.
        color: root.rightColor()
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
    }

    // Bottom right of the window and above this strip, which is the operator's own placement. It is
    // a child of the bar rather than of the window because a bar is not a clipping item, so a child
    // anchored past its top edge both draws and takes clicks over the listing.
    TransferCard {
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.bottom: parent.top
        anchors.bottomMargin: Theme.spacing.gap
        transfer: root.transfer
        onCancelRequested: function (id) { root.transferCancelRequested(id) }
    }
}
