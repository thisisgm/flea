import QtQuick
import qs.Commons
import "." as Flea
import "js/Filter.js" as Filter
import "js/Match.js" as Match
import "js/Picker.js" as Picker
import "js/PickerKeys.js" as PickerKeys
import "js/PickerMarks.js" as Marks

// The picker's listing: ui/Row.qml drawn behind a check box, and the keys that move through it. The
// window's own ui/List.qml is not reused, because every line of it that is not layout is a drag, a
// rename, a thumbnail or a context menu, and a chooser has none of those.
ListView {
    id: root

    property var picker: null
    property var backend: null
    // The location strip ":" and Ctrl+L hand the keyboard to, ui/PickerEntry.qml in the window.
    property var entry: null

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
        readonly property bool isMarked: cell.markable && Marks.marked(root.picker.marks, cell.rowPath)

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
            // that hands a file to the caller is a send nobody asked for. Ctrl and Shift are Finder's
            // two marking modifiers, neither ever opens, and only the first tap of one counts, so a
            // held double click marks once instead of toggling itself back off.
            onTapped: function (eventPoint, button) {
                root.picker.cursorIndex = cell.listingIndex
                root.picker.focusList()
                var mods = tap.point.modifiers
                if (mods & Qt.ControlModifier) {
                    if (tap.tapCount === 1)
                        root.picker.toggleMark(cell.listingIndex)
                    return
                }
                if (mods & Qt.ShiftModifier) {
                    if (tap.tapCount === 1)
                        root.markRange(cell.listingIndex)
                    return
                }
                var onBox = box.visible && eventPoint.position.x <= box.x + box.width + Theme.spacing.gap
                if (onBox || (tap.tapCount === 2 && cell.markable))
                    root.picker.toggleMark(cell.listingIndex)
                else if (tap.tapCount === 2 && cell.row && cell.row.d)
                    root.picker.open(cell.rowPath)
            }
        }
    }

    // Shift+click. The rows drawn from the anchor to this one, or this one alone before any toggle,
    // each read from the held window: a row scrolled out of it is not on screen and was never part
    // of what the person saw as the range. The anchor stays where it was, as the cursor moves.
    function markRange(index) {
        var picker = root.picker
        var from = picker.markAnchor >= 0 ? picker.markAnchor : index
        var onScreen = picker.shown !== null ? picker.shown : picker.rows.map(function (_, i) { return picker.held + i })
        var drawn = Filter.between(onScreen, from, index)
        if (from > index)
            drawn.reverse()
        var rows = []
        for (var i = 0; i < drawn.length; i++) {
            var row = picker.rowFor(drawn[i])
            if (row && row.d === picker.folderMode)
                rows.push({ path: Picker.rowPath(picker.path, row.n), bytes: row.s })
        }
        picker.marks = Marks.markRange(picker.marks, rows, picker.req.multiple)
        picker.cursorIndex = index
    }

    // The fleapicker seam's rowCentre: a drawn row's painted box reduced to the point a test clicks,
    // the same read ui/Ipc.qml makes through ui/shell.qml's centreOf.
    function rowCentre(index) {
        var item = root.itemAtIndex(Filter.viewOf(root.picker.shown, index))
        if (!item)
            return ""
        var rect = root.picker.itemRect(item)
        return Math.round(rect.x + rect.width / 2) + " " + Math.round(rect.y + rect.height / 2)
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

    // ui/js/PickerKeys.js: the picker's own verbs, then keys.toml through its allowlist.
    Keys.onPressed: function (event) { PickerKeys.handle(event, root.picker, root) }

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
