import QtQuick
import qs.Commons
import "." as Flea
import "js/Match.js" as Match
import "js/Picker.js" as Picker
import "js/Keymap.js" as Keymap
import "js/Scroll.js" as Scroll
import "js/Sort.js" as Sort

// The picker's listing: ui/Row.qml drawn behind a check box, and the keys that move through it. The
// window's own ui/List.qml is not reused, because every line of it that is not layout is a drag, a
// rename, a thumbnail or a context menu, and a chooser has none of those.
ListView {
    id: root

    property var picker: null
    property var backend: null

    // Whether the listing shows the directory's dotfiles. The backend never sends what a request
    // did not ask for, so the window re-reads the standing directory when this flips; `.` and the
    // preset's toggleHidden chord are the two ways in, and the flag survives every walk.
    property bool showHidden: false

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
    // The previous tap path, so a rebuilt list never sends a row the first tap missed.
    property string lastTapPath: ""
    Flea.FastScrollHandler { flickable: root }
    Flea.ViewportScrollBar {
        parent: root
        anchors { top: parent.top; right: parent.right }
        flickable: root
    }

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

        // The scroll lane stays clear, the same rule the window's own list follows.
        width: Scroll.contentWidth(root.width, Theme.spacing.rowPaddingX)
        height: Theme.rowHeight

        // The board's marked row: the accent wash and the accent bar, under the row's own text. The
        // window's selected fill is a foreground grey, and SendPicker.html marks in the accent, so
        // ui/Row.qml is left unselected here and the picker paints its own mark behind it.
        Rectangle {
            visible: cell.isMarked
            width: root.width
            height: parent.height
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
            paintWidth: root.width
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
            // One tap moves the cursor, a second tap on the same path opens or sends, a box tap toggles.
            onTapped: function (eventPoint, button) {
                root.picker.cursorIndex = cell.listingIndex
                root.forceActiveFocus()
                var onBox = box.visible && eventPoint.position.x <= box.x + box.width + Theme.spacing.gap
                var second = tap.tapCount === 2
                var firstPath = root.lastTapPath
                root.lastTapPath = cell.rowPath
                if (onBox)
                    root.picker.toggleMark(cell.listingIndex)
                else if (second && Picker.sameTap(firstPath, cell.rowPath))
                    root.picker.doubleActivate(cell.listingIndex, cell.rowPath, firstPath)
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
        // Confirmation belongs to the chooser, even when the listing preset binds Enter to Rename.
        var confirm = (event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
            && (event.modifiers & ~Qt.KeypadModifier) === Qt.NoModifier
        var action = confirm ? "open" : Keymap.lookup(event.key, event.text, event.modifiers, "listing")
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
        } else if (action === "sortNext") {
            root.picker.requestSort(Sort.nextOrder(Picker.SORT_ORDERS, root.picker.sortBy))
        } else if (action === "sortReverse") {
            root.picker.requestSort(Sort.reverseOrder(root.picker.sortBy, root.picker.sortDesc))
        } else if (action === "toggleHidden") {
            root.showHidden = !root.showHidden
            if (root.picker.path.length > 0)
                root.picker.openWithoutHistory(root.picker.path)
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
