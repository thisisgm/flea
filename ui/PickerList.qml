import QtQuick
import qs.Commons
import "." as Flea
import "js/Filter.js" as Filter
import "js/Match.js" as Match
import "js/Picker.js" as Picker

// The picker's listing: ui/Row.qml drawn behind a check box, and the keys that move through it. The
// window's own ui/List.qml is not reused, because every line of it that is not layout is a drag, a
// rename, a thumbnail or a context menu, and a chooser has none of those.
ListView {
    id: root

    property var picker: null
    property var backend: null

    readonly property int visibleRows: Math.max(1, Math.ceil(root.height / Theme.rowHeight))
    // Wide enough that a row's box is a check and not a chip; the board's own is one pixel over bodySmall.
    readonly property int checkSize: Theme.font.bodySmall

    model: root.picker.shownTotal
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    highlightMoveDuration: 0
    reuseItems: true
    cacheBuffer: Theme.rowHeight * 4

    delegate: Item {
        id: cell
        required property int index
        // index is where the row is drawn, listingIndex is the row the backend numbers; under a
        // filter chip the two differ, and ui/js/Filter.js is what converts between them.
        readonly property int listingIndex: Filter.at(root.picker.shown, index)
        readonly property var row: root.picker.rowFor(cell.listingIndex)
        readonly property string rowPath: cell.row ? Picker.rowPath(root.picker.path, cell.row.n) : ""
        // A Recent row is named by its whole path under the listing base, and SendPicker.html draws
        // the file's own name; the row handed to Row.qml is the same row under that name.
        readonly property var shownRow: cell.row && root.picker.recent
            ? Object.assign({}, cell.row, { n: Match.base(cell.row.n) })
            : cell.row
        // A file request marks files and a folder request marks folders; the other kind is a way
        // through the tree and never an answer, so it carries no box at all.
        readonly property bool markable: cell.row !== null && cell.row.d === root.picker.folderMode
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
            border.width: Theme.spacing.hairline * 2
            border.color: cell.isMarked ? Theme.color.accent : Theme.color.muted

            Flea.Glyph {
                anchors.fill: parent
                visible: cell.isMarked
                name: "check"
                maxSize: root.checkSize
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
                if (onBox || (tap.tapCount === 2 && cell.markable))
                    root.picker.toggleMark(cell.listingIndex)
                else if (tap.tapCount === 2 && cell.row && cell.row.d)
                    root.picker.open(cell.rowPath)
            }
        }
    }

    // Both ends are view positions, because a filter chip makes the listing rows between them a set.
    function moveCursor(delta) {
        if (root.picker.shownTotal === 0)
            return
        var was = Filter.viewOf(root.picker.shown, root.picker.cursorIndex)
        var to = Math.max(0, Math.min(root.picker.shownTotal - 1, was + delta))
        root.picker.cursorIndex = Filter.at(root.picker.shown, to)
        root.positionViewAtIndex(to, ListView.Contain)
    }

    Keys.onPressed: function (event) {
        event.accepted = true
        if (event.key === Qt.Key_Escape) {
            root.picker.cancel()
        } else if (event.key === Qt.Key_Down || event.key === Qt.Key_J) {
            root.moveCursor(1)
        } else if (event.key === Qt.Key_Up || event.key === Qt.Key_K) {
            root.moveCursor(-1)
        } else if (event.key === Qt.Key_PageDown) {
            root.moveCursor(root.visibleRows)
        } else if (event.key === Qt.Key_PageUp) {
            root.moveCursor(-root.visibleRows)
        } else if (event.key === Qt.Key_Home) {
            root.moveCursor(-root.picker.shownTotal)
        } else if (event.key === Qt.Key_End) {
            root.moveCursor(root.picker.shownTotal)
        } else if (event.key === Qt.Key_Space) {
            root.picker.toggleMark(root.picker.cursorIndex)
        } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_L) {
            root.picker.activate(root.picker.cursorIndex)
        } else if (event.key === Qt.Key_Backspace || event.key === Qt.Key_H) {
            root.picker.goUp()
        } else if (event.key === Qt.Key_Left && (event.modifiers & Qt.AltModifier)) {
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
        // A chip narrows rows already held, so it can never scroll past them and asks for nothing.
        if (root.picker.total === 0 || root.picker.shown !== null)
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
