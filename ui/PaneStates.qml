import QtQuick
import "." as Flea
import "js/Swap.js" as Swap
import "js/Search.js" as Search
import "js/KeyName.js" as KeyName

// What a pane draws instead of rows: the empty hero, the reading spiral and the failure block. All
// three sit over the same slot and are gated on the same listing state, so they live together and
// ui/Pane.qml carries one child rather than three.
Item {
    id: root

    required property var pane
    property bool trashOpen: false

    readonly property alias emptyItem: emptyState
    readonly property alias messageItem: paneMessage

    // Every Columns hero centres on the pane while it clears the listed columns and their rules, else on the free area right of them.
    readonly property var columns: root.pane.viewMode === "columns" ? root.pane.columnsArea : null
    readonly property real heroWidth: emptyState.visible ? emptyState.naturalWidth
        : loadingState.visible ? loadingState.naturalWidth : paneMessage.naturalWidth
    readonly property bool overActive: !!root.columns && root.columns.activeX > 0
        && root.pane.listSlot.width / 2 - root.heroWidth / 2 < root.columns.activeX
    readonly property real heroX: root.pane.listSlot.x + (root.overActive ? root.columns.activeX : 0)
    readonly property real heroSpan: root.overActive ? root.pane.listSlot.width - root.columns.activeX : root.pane.listSlot.width

    Flea.EmptyState {
        id: emptyState
        x: root.heroX
        y: root.pane.listSlot.y
        width: root.heroSpan
        height: root.pane.listSlot.height
        visible: !root.trashOpen && root.pane.listingState === "empty"
        caption: Search.live(root.pane) ? Search.nothingMatches(root.pane.swap.answeredQuery) : ""
        mark: "search"
        hint: Search.live(root.pane) ? "Press Escape to clear."
            : ViewState.keyHints ? KeyName.hinted("Press %1 for a new folder.", "newFolder") : ""
    }

    Flea.LoadingState {
        id: loadingState
        x: root.heroX
        y: root.pane.listSlot.y
        width: root.heroSpan
        height: root.pane.listSlot.height
        // A listing past its wait is the Error state, so the crawl yields to the sentence.
        visible: !root.trashOpen && root.pane.listingState === "loading"
        heldOff: root.pane.swap.fellBack
        holdOffMs: Swap.markHoldOff(root.pane.searchMode)
        onShownChanged: root.pane.swap.markShown(shown)
    }

    // While a listing is out a press or wheel notch over the header, filter or rows lands on nothing; the handlers gate it, not enabled.
    MouseArea {
        x: root.pane.listSlot.x
        y: root.pane.header.y
        width: root.pane.listSlot.width
        height: root.pane.listSlot.y + root.pane.listSlot.height - root.pane.header.y
        enabled: !root.trashOpen
        acceptedButtons: Qt.AllButtons
        onPressed: function (mouse) {
            mouse.accepted = root.pane.listInFlight
            if (mouse.accepted) root.pane.message(Swap.LOADING, false)
        }
        onWheel: function (wheel) { wheel.accepted = root.pane.listInFlight }
        // Passive, above the rows, so it sees a row's press and its release even after the row is rebuilt; a platform drag takes the release, so ui/FileDrag.qml liftEnded clears the hold.
        PointHandler {
            acceptedButtons: Qt.AllButtons
            onActiveChanged: root.pane.pointerHeld = active
        }
    }

    Flea.StateMessage {
        id: paneMessage
        active: !root.trashOpen
        x: root.heroX + Theme.spacing.rowPaddingX
        y: root.pane.listSlot.y
        width: root.heroSpan - 2 * Theme.spacing.rowPaddingX
        height: root.pane.listSlot.height
        message: root.pane.stateMessage
        listingState: root.pane.listingState
        lockedMode: root.pane.lockedMode
        total: root.pane.total
        // The rows the filter has tested: the held window until the folder's answer lands (issue 315).
        loaded: root.pane.filterTested
        filterQuery: root.pane.filterQuery
        shown: root.pane.filterQuery.length > 0 ? root.pane.shownTotal : -1
    }
}
