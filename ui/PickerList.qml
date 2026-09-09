import QtQuick
import qs.Commons
import "." as Flea
import "js/Drag.js" as DragOps
import "js/Filter.js" as Filter
import "js/Match.js" as Match
import "js/Picker.js" as Picker
import "js/PickerKeys.js" as PickerKeys
import "js/PickerMarks.js" as Marks
import "js/Thumbs.js" as Thumbs

// The picker's listing: ui/Row.qml drawn behind a check box, the keys that move through it, and the
// drag that carries rows out to another application. The window's own ui/List.qml is not reused: it
// selects rows by index where the chooser marks by path, so the pieces are drawn again over the marks.
// The one part the chooser shares with it, the thumbnail settle, is repeated below.
ListView {
    id: root

    property var picker: null
    property var backend: null
    // The location strip ":" and Ctrl+L hand the keyboard to, ui/PickerEntry.qml in the window.
    property var entry: null

    // The listing rows a drag out carries, and what they put on the wire, built at the lift and
    // cleared with the gesture. There is no drop side: nothing lands in a chooser. ui/js/Drag.js decides.
    property var dragRows: []
    property var dragMime: ({})

    readonly property int visibleRows: Math.max(1, Math.ceil(root.height / Theme.rowHeight))
    // ui/Pane.qml's two settle intervals, see AGENTS.md "Thumbnail requests in the GUI".
    readonly property int firstSettleMs: 70
    readonly property int settleMs: 120
    // Wide enough that a row's box is a check and not a chip; the board's own is one pixel over bodySmall.
    readonly property int checkSize: Theme.font.bodySmall

    model: root.picker.shownTotal
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    highlightMoveDuration: 0
    reuseItems: true
    cacheBuffer: Theme.rowHeight * 4

    // The item Qt hangs the platform drag off, on the view and never in a delegate: a tab hover switch
    // re-lists mid-drag and releases the pressed row, and a QDrag parented there died inside its own
    // exec while the compositor still asked it for data (quickshell SIGSEGV, 2026-09-07). No drag image.
    Item {
        id: ghost
        Drag.dragType: Drag.Automatic
        // Copy alone, because supportedActions is the only one of these another application ever
        // sees: offering Qt.MoveAction told Chromium the drop was a move, which Google's uploader
        // refuses, and liftEnded removes nothing so it was a promise Flea cannot keep.
        Drag.supportedActions: Qt.CopyAction
        Drag.proposedAction: Qt.CopyAction
        Drag.mimeData: root.dragMime
        // The one end of the gesture: exec has returned, whatever became of the row that lifted it.
        Drag.onDragFinished: root.liftEnded(ghost)
    }

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
            thumb: root.thumbFor(cell.listingIndex)
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

        // A press that moves past the threshold lifts the row and cancels the tap above. The grab
        // ends nothing: the compositor owns the gesture once the platform drag has started, so the
        // end is the ghost's dragFinished.
        DragHandler {
            id: lift
            target: null
            // The list is a Flickable and would take the grab past its own threshold; without ApprovesTakeOverByItems it cannot.
            grabPermissions: PointerHandler.CanTakeOverFromItems | PointerHandler.CanTakeOverFromHandlersOfDifferentType | PointerHandler.ApprovesTakeOverByHandlersOfSameType
            onActiveChanged: if (active) root.liftBegan(cell.listingIndex, ghost)
        }
    }

    // The drag out: the marks when the pressed row is one of them, that row alone otherwise, the
    // rule ui/js/Drag.js carried gives the window. A drag out is always a copy, the one action the
    // ghost advertises, so the marker bakes copy and the footer's line carries no ctrl hint. The
    // note follows the payload: no uri-list on the wire means no other application can take it.
    function liftBegan(index, ghost) {
        root.dragRows = DragOps.carried(root.picker, index)
        root.dragMime = DragOps.mimeFor(root.picker, root.dragRows, true)
        root.picker.sticky(DragOps.line(root.dragRows.length, "", true)
                           + DragOps.reachNote(root.dragMime.hasOwnProperty("text/uri-list")))
        // Automatic starts the platform drag on this assignment and does not return until the drop,
        // so everything the gesture needs is already set above.
        ghost.Drag.active = true
    }

    // The platform drag has already finished inside liftBegan, so this only clears the gesture's own state.
    function liftEnded(ghost) {
        ghost.Drag.active = false
        root.dragRows = []
        root.dragMime = ({})
        root.picker.sticky("")
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

    // ui/List.qml's own line under the last row the query left standing: caption type, muted, and
    // gone with the query. A chip alone says nothing here; the pill row already shows what narrows.
    footer: Item {
        width: root.width
        height: note.text.length > 0 ? Theme.chromeHeight : 0

        Text {
            id: note
            anchors.fill: parent
            anchors.leftMargin: Theme.spacing.rowPaddingX
            anchors.rightMargin: Theme.spacing.rowPaddingX
            verticalAlignment: Text.AlignVCenter
            text: root.picker.filterQuery.length > 0
                ? Filter.note(root.picker.shown, root.picker.rows.length, root.picker.filterQuery) : ""
            color: Theme.color.muted
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            elide: Text.ElideRight
            textFormat: Text.PlainText
        }
    }

    // ui/js/PickerKeys.js: the picker's own verbs, then keys.toml through its allowlist.
    Keys.onPressed: function (event) { PickerKeys.handle(event, root.picker, root) }

    // A query drops the view back to its first row without moving contentY afterwards, so closing
    // it can leave the viewport over rows the held window no longer covers; the drift check runs
    // again on every change of the query rather than only on a scroll.
    Connections {
        target: root.picker
        function onFilterQueryChanged() { coalesce.restart() }
    }

    // The listing is a window around the viewport, not the directory, so scrolling refetches. Same
    // shape as ui/List.qml's own drift check, minus the directory-size planner.
    onContentYChanged: { coalesce.restart(); settle.restart() }
    onVisibleRowsChanged: settle.restart()

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

    // ui/List.qml's settle: every scroll, arriving window of rows and viewport change pushes it out,
    // so a fling issues no request until the list has stopped. A new listing starts it back at 70.
    Timer {
        id: settle
        interval: root.firstSettleMs
        onTriggered: root.requestThumbs()
    }

    Connections {
        target: root.backend
        function onRows(start, items, ms, kinds) { settle.restart() }
    }

    Connections {
        target: root.picker
        // A chip swap redraws the screen without a scroll, so it is a settle too.
        function onShownChanged() { settle.restart() }
        function onListingStateChanged() {
            if (root.picker.listingState === "loading")
                settle.interval = root.firstSettleMs
        }
    }

    // Only the visible rows, only once each, and only after the list has stopped moving.
    function requestThumbs() {
        var picker = root.picker
        if (picker.shownTotal === 0 || picker.listingState === "loading")
            return
        var view = Thumbs.viewport(root.contentY, Theme.rowHeight, root.visibleRows, picker.shownTotal)
        // A chip narrows the viewport to a set and not a run: the run it spans is what the planner
        // gets and Filter.cut takes back every row inside that run the chip is hiding.
        var span = Filter.span(picker.shown, view.first, view.last)
        var work = Filter.cut(Thumbs.plan(picker.thumbState, picker.rows, picker.held, span.first, span.last), picker.shown)
        root.backend.thumbcancel(work.drop)
        root.backend.thumb(work.ask)
        if (work.ask.length > 0)
            settle.interval = root.settleMs
        picker.thumbState = Thumbs.applied(picker.thumbState, work)
    }

    function thumbFor(index) {
        return Thumbs.fileFor(root.picker.thumbState, index)
    }
}
