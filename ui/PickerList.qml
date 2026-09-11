import QtQuick
import qs.Commons
import "." as Flea
import "js/Match.js" as Match
import "js/Picker.js" as Picker
import "js/Keymap.js" as Keymap

// The picker's listing: ui/Row.qml drawn behind a check box, and the keys that move through it. The
// window's own ui/List.qml is not reused, because every line of it that is not layout is a drag, a
// rename, a thumbnail or a context menu, and a chooser has none of those.
ListView {
    id: root

    property var picker: null
    property var backend: null

    readonly property int visibleRows: Math.max(1, Math.ceil(root.height / Theme.rowHeight))
    // The board's content-box dimensions exclude the border; QML Rectangle dimensions include it.
    readonly property int checkInnerSize: Theme.font.bodySmall + Theme.spacing.hairline
    readonly property int checkBorderWidth: Theme.spacing.hairline * 2
    readonly property int checkSize: root.checkInnerSize + root.checkBorderWidth * 2

    model: root.picker.shownTotal
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    highlightMoveDuration: 0
    reuseItems: true
    cacheBuffer: Theme.rowHeight * 4
    activeFocusOnTab: true
    Keys.onTabPressed: function(event) { root.picker.stepFocus(root, (event.modifiers & Qt.ShiftModifier) !== 0) }
    Keys.onBacktabPressed: root.picker.stepFocus(root, true)
    property bool firstArmed: false
    Flea.FastScrollHandler { flickable: root }

    delegate: Item {
        id: cell
        required property int index
        readonly property int listingIndex: index
        readonly property var row: root.picker.rowFor(cell.listingIndex)
        readonly property string rowPath: cell.row ? Picker.rowPath(root.picker.path, cell.row.n) : ""
        // A Recent row is named by its whole path under the listing base, and SendPicker.html draws
        // the file's own name; the row handed to Row.qml is the same row under that name.
        readonly property var shownRow: cell.row && root.picker.recent
            ? Object.assign({}, cell.row, { n: Match.base(cell.row.n) })
            : cell.row
        // A file request marks files and a folder request marks folders; the other kind is a way
        // through the tree and never an answer, so it carries no box at all.
        readonly property bool markable: cell.row !== null && Picker.directory(cell.row) === root.picker.folderMode
        readonly property bool isMarked: cell.markable && Picker.marked(root.picker.marks, cell.rowPath)

        width: root.width
        height: Theme.rowHeight

        // The board's marked row: the accent wash and the accent bar, under the row's own text. The
        // window's selected fill is a foreground grey, and SendPicker.html marks in the accent, so
        // ui/Row.qml is left unselected here and the picker paints its own mark behind it.
        Rectangle {
            visible: cell.isMarked
            anchors.fill: parent
            color: Style.selectedAccentFill
        }

        Rectangle {
            visible: cell.isMarked
            width: Theme.spacing.hairline * 2
            height: parent.height
            color: Theme.color.accent
        }

        Flea.Row {
            anchors.fill: parent
            leadingSlot: root.checkSize + Theme.spacing.gap
            compactDate: true
            foregroundMetadata: true
            hiddenCols: Picker.HIDDEN_COLS
            row: cell.shownRow
            cursor: cell.listingIndex === root.picker.cursorIndex
            hovered: hover.hovered
            kindNames: root.picker.kindNames
        }

        Rectangle {
            id: box
            visible: cell.markable
            anchors.left: parent.left
            anchors.leftMargin: Theme.spacing.rowPaddingX
            anchors.verticalCenter: parent.verticalCenter
            width: root.checkSize
            height: root.checkSize
            color: "transparent"
            border.width: root.checkBorderWidth
            border.color: cell.isMarked ? Theme.color.accent : Theme.color.muted

            Flea.Glyph {
                anchors.fill: parent
                visible: cell.isMarked
                name: "check"
                maxSize: root.checkInnerSize - root.checkBorderWidth * 2
                color: Theme.color.accent
            }
        }

        HoverHandler {
            id: hover
        }

        TapHandler {
            id: tap
            acceptedButtons: Qt.LeftButton
            // ui/js/Tap.js's rule, one tap selects and the second opens, with the chooser's one
            // difference: the second tap on a file marks it and never sends, because a double click
            // that hands a file to the caller is a send nobody asked for.
            onTapped: function (eventPoint, button) {
                root.picker.cursorIndex = cell.listingIndex
                root.forceActiveFocus()
                var onBox = box.visible && eventPoint.position.x <= box.x + box.width + Theme.spacing.gap
                if (onBox)
                    root.picker.toggleMark(cell.listingIndex)
                else if (tap.tapCount === 2 && Picker.directory(cell.row))
                    root.picker.open(cell.rowPath)
                else if (tap.tapCount === 2 && cell.markable)
                    root.picker.toggleMark(cell.listingIndex)
            }
        }
    }

    function moveCursor(delta) {
        if (root.picker.shownTotal === 0)
            return
        var was = root.picker.cursorIndex
        var to = Math.max(0, Math.min(root.picker.shownTotal - 1, was + delta))
        root.picker.cursorIndex = to
        root.positionViewAtIndex(to, ListView.Contain)
    }

    Keys.onPressed: function (event) {
        var action = Keymap.lookup(event.key, event.text, event.modifiers, "listing")
        event.accepted = true
        if (action === "cursorFirstArm") {
            if (root.firstArmed) root.moveCursor(-root.picker.shownTotal)
            root.firstArmed = !root.firstArmed
            return
        }
        root.firstArmed = false
        if (event.key === Qt.Key_Escape) {
            root.picker.cancel()
        } else if (action === "cursorDown") {
            root.moveCursor(1)
        } else if (action === "cursorUp") {
            root.moveCursor(-1)
        } else if (action === "pageDown") {
            root.moveCursor(root.visibleRows)
        } else if (action === "pageUp") {
            root.moveCursor(-root.visibleRows)
        } else if (action === "cursorFirst") {
            root.moveCursor(-root.picker.shownTotal)
        } else if (action === "cursorLast") {
            root.moveCursor(root.picker.shownTotal)
        } else if (event.key === Qt.Key_Space && event.modifiers === Qt.NoModifier) {
            root.picker.toggleMark(root.picker.cursorIndex)
        } else if (action === "open" || action === "pageForward") {
            root.picker.activate(root.picker.cursorIndex)
        } else if (action === "parent") {
            root.picker.goUp()
        } else if (action === "historyBack") {
            root.picker.goBack()
        } else {
            event.accepted = false
        }
    }

    // The listing is a window around the viewport, not the directory, so scrolling refetches. Same
    // shape as ui/List.qml's own drift check, minus the thumbnail and directory-size planners.
    onContentYChanged: coalesce.restart()

    Timer {
        id: coalesce
        interval: 16
        onTriggered: root.requestIfDrifted()
    }

    function requestIfDrifted() {
        if (root.picker.backendUnavailable || root.picker.total === 0 || root.picker.pendingListings > 0)
            return
        var firstVisible = Math.floor(root.contentY / Theme.rowHeight)
        var heldEnd = root.picker.held + root.picker.rows.length
        if (root.picker.rows.length === 0
                || (firstVisible < root.picker.held && root.picker.held > 0)
                || (firstVisible + root.visibleRows > heldEnd && heldEnd < root.picker.total)) {
            var start = Math.max(0, firstVisible - root.picker.windowSize / 4)
            root.backend.window(Math.floor(start), root.picker.windowSize)
        }
    }
}
