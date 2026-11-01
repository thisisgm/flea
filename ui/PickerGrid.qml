import QtQuick
import "." as Flea
import "js/Match.js" as Match
import "js/Picker.js" as Picker
import "js/Keymap.js" as Keymap
import "js/GridGeometry.js" as GridGeometry
import "js/Density.js" as Density
import "js/Thumbs.js" as Thumbs
import "js/ExtThumbs.js" as ExtThumbs
import "js/Sort.js" as Sort

// Issue #191 grid mode: the main window's tiles over the same rows, visible tiles only.
GridView {
    id: root

    property var picker: null
    // The listing worker, which holds the rows a thumb request names; see ui/PickerListing.qml.
    property var backend: null

    // The first screen outlasts the compositor's resize, the fling debounce after that.
    readonly property int firstSettleMs: 70
    readonly property int settleMs: 120

    // The same tile maths the main grid lays out by, so a tile here is a tile there.
    readonly property int columns: GridGeometry.columnsFor(root.width, Theme.grid.minCellWidth,
        ViewState.thumbnailPixels, Theme.spacing.rowPaddingX, Theme.spacing.gap, Theme.spacing.rowPaddingX)
    readonly property int cellHeightPx: GridGeometry.cellHeightFor(ViewState.thumbnailPixels, Theme.spacing.gap,
                                        Math.ceil(Theme.grid.captionHeight),
                                        Density.gridPadY(Theme.spacing.rowPaddingX, ViewState.density))
    readonly property int visibleTileRows: Math.max(1, Math.ceil(root.height / root.cellHeightPx))
    onColumnsChanged: if (root.visible) settle.restart()
    onVisibleTileRowsChanged: if (root.visible) settle.restart()

    // Hidden builds nothing: a hidden view is not a free view, AGENTS.md rule 6.
    model: root.visible ? root.picker.total : 0
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    highlightMoveDuration: 0
    reuseItems: true
    cacheBuffer: root.cellHeightPx * 2
    activeFocusOnTab: true
    Keys.onTabPressed: function(event) { root.picker.stepFocus(root, (event.modifiers & Qt.ShiftModifier) !== 0) }
    Keys.onBacktabPressed: root.picker.stepFocus(root, true)
    property bool firstArmed: false
    // The previous tap path, so a rebuilt grid never sends a tile the first tap missed.
    property string lastTapPath: ""
    leftMargin: Theme.spacing.gap
    topMargin: Theme.spacing.gap
    cellWidth: GridGeometry.cellWidthFor(root.width, root.columns, Theme.spacing.gap, Theme.spacing.rowPaddingX)
    cellHeight: root.cellHeightPx
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
        // A Recent row is named by its whole path, drawn by its own leaf; see ui/PickerList.qml.
        readonly property var shownRow: cell.row && root.picker.recent
            ? Object.assign({}, cell.row, { n: Match.base(cell.row.n) })
            : cell.row
        // A file request marks files and a folder request marks folders; the other kind is a way
        // through the tree and never an answer, so it carries no box at all.
        readonly property bool markable: root.picker.marksAllowed && cell.row !== null && Picker.directory(cell.row) === root.picker.folderMode
        readonly property bool isMarked: cell.markable && Picker.marked(root.picker.marks, cell.rowPath)

        width: root.cellWidth
        height: root.cellHeight

        Flea.GridTile {
            anchors.fill: parent
            row: cell.shownRow
            cursor: cell.listingIndex === root.picker.cursorIndex
            hovered: hover.hovered
            selected: cell.isMarked
            thumb: root.thumbFor(cell.listingIndex)
        }

        // The board's box, one gap in from the tile's top left; the cursor tile's mark is
        // foreground through GridTile itself, like the main grid.
        Flea.PickerBox {
            id: box
            visible: cell.markable
            x: Theme.spacing.gap
            y: Theme.spacing.gap
            marked: cell.isMarked
        }

        HoverHandler {
            id: hover
        }

        TapHandler {
            id: tap
            acceptedButtons: Qt.LeftButton
            // One tap moves the cursor; a same-path double tap opens, accepts or marks by mode; a box tap toggles.
            onTapped: function (eventPoint, button) {
                var was = root.picker.cursorIndex
                var extending = (tap.point.modifiers & Qt.ShiftModifier) !== 0 && root.picker.marksAllowed
                if (!extending) root.picker.endRange()
                root.picker.cursorIndex = cell.listingIndex
                root.forceActiveFocus()
                var onBox = box.visible && eventPoint.position.x <= box.x + box.width + Theme.spacing.gap
                    && eventPoint.position.y <= box.y + box.height + Theme.spacing.gap
                var second = tap.tapCount === 2
                var firstPath = root.lastTapPath
                root.lastTapPath = cell.rowPath
                if (extending)
                    root.picker.markRange(was, cell.listingIndex)
                else if (onBox)
                    root.picker.toggleMark(cell.listingIndex)
                else if (second && Picker.sameTap(firstPath, cell.rowPath))
                    root.picker.doubleActivate(cell.listingIndex, cell.rowPath, firstPath)
                root.picker.adoptSaveName(cell.listingIndex)
            }
        }
    }

    // One tile row of bare ground at the end, the same inset the main grid keeps.
    footer: Item {
        width: Math.max(0, root.width - Theme.spacing.rowPaddingX)
        height: Theme.chromeHeight
    }

    function moveCursor(delta, extending) {
        if (root.picker.total === 0)
            return
        var to = Picker.gridTarget(root.picker.cursorIndex, delta, root.columns, root.picker.total)
        var before = root.picker.cursorIndex
        if (!extending) root.picker.endRange()
        root.picker.cursorIndex = to
        if (extending) root.picker.markRange(before, to)
        root.positionViewAtIndex(to, GridView.Contain)
    }

    function jumpTo(index) {
        if (root.picker.total === 0)
            return
        root.picker.endRange()
        var to = Math.max(0, Math.min(root.picker.total - 1, index))
        root.picker.cursorIndex = to
        root.positionViewAtIndex(to, GridView.Contain)
    }

    function thumbFor(index) {
        return Thumbs.allowed(root.picker.rowFor(index), ViewState.thumbnailMode)
            ? Thumbs.fileFor(root.picker.thumbState, index) : ""
    }

    // One dispatch for every action the lookup names, so a key test pins the live path.
    function handleAction(action, key, modifiers) {
        if (action === "pathBar") {
            root.firstArmed = false
            root.picker.openPathBar()
            return true
        }
        if (action === "cursorFirstArm") {
            if (root.firstArmed) root.jumpTo(0)
            root.firstArmed = !root.firstArmed
            return true
        }
        root.firstArmed = false
        if (key === Qt.Key_Escape) {
            root.picker.cancel()
        } else if (action === "cursorDown") {
            root.moveCursor(root.columns)
        } else if (action === "cursorUp") {
            root.moveCursor(-root.columns)
        } else if (action === "extendDown") {
            root.moveCursor(root.columns, true)
        } else if (action === "extendUp") {
            root.moveCursor(-root.columns, true)
        } else if (action === "selectAll") {
            root.picker.selectAll()
        } else if (action === "pageDown") {
            root.jumpTo(Picker.pageTarget(root.picker.cursorIndex, root.visibleTileRows * root.columns, root.picker.total))
        } else if (action === "pageUp") {
            root.jumpTo(Picker.pageTarget(root.picker.cursorIndex, -root.visibleTileRows * root.columns, root.picker.total))
        } else if (action === "cursorFirst") {
            root.jumpTo(0)
        } else if (action === "cursorLast") {
            root.jumpTo(root.picker.total - 1)
        } else if (action === "viewList") {
            root.picker.setView("list")
        } else if (action === "viewGrid") {
            root.picker.setView("grid")
        } else if (key === Qt.Key_Space && modifiers === Qt.NoModifier) {
            root.picker.toggleMark(root.picker.cursorIndex)
        } else if (Picker.activates(action, key)) {
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
            root.picker.showHidden = !root.picker.showHidden
            if (root.picker.path.length > 0)
                root.picker.openWithoutHistory(root.picker.path)
        } else {
            return false
        }
        return true
    }

    Keys.onPressed: function (event) {
        // Bare arrows and h/l step to visual neighbours in the grid, the main grid's own rule;
        // the map below would read them as parent and browse-in instead.
        if (event.modifiers === Qt.NoModifier
                && (event.key === Qt.Key_Left || event.key === Qt.Key_Right
                    || event.text === "h" || event.text === "l")) {
            var across = (event.key === Qt.Key_Left || event.text === "h") ? -1 : 1
            event.accepted = true
            root.firstArmed = false
            root.moveCursor(across)
            return
        }
        var action = Keymap.lookup(event.key, event.text, event.modifiers, "listing")
        event.accepted = root.handleAction(action, event.key, event.modifiers)
    }

    // A scroll refetches the window and restarts the thumb settle for newly visible tiles.
    // Held rows are never refetched, so without the restart no settle asks for them.
    onContentYChanged: {
        coalesce.restart()
        settle.restart()
    }

    Timer {
        id: coalesce
        interval: root.picker.coalesceMs
        onTriggered: root.requestIfDrifted()
    }

    // Settle fires counted, so a probe waits on the first run rather than a delay.
    property int settleRuns: 0
    // Whether a settle fire is pending, so a probe quiesces before proving a restart.
    property alias settleRunning: settle.running
    Timer {
        id: settle
        interval: root.firstSettleMs
        onTriggered: { root.settleRuns += 1; root.requestThumbs() }
    }

    function primeSettle() { settle.interval = root.firstSettleMs }
    function restartSettle() { settle.restart() }

    // A reshow owns its window: move to the cursor, refetch there, restart thumbs.
    function reshow(index) {
        root.positionViewAtIndex(index, GridView.Contain)
        root.requestIfDrifted()
        root.restartSettle()
    }

    function requestIfDrifted() {
        if (!root.visible || root.picker.backendUnavailable || root.picker.total === 0 || root.picker.pendingListings > 0)
            return
        var range = Picker.tileRange(root.contentY, root.cellHeightPx, root.visibleTileRows, root.columns, root.picker.total)
        var heldEnd = root.picker.held + root.picker.rows.length
        if (root.picker.rows.length === 0
                || (range.first < root.picker.held && root.picker.held > 0)
                || (range.last >= heldEnd && heldEnd < root.picker.total)) {
            var start = Math.max(0, range.first - Math.floor(root.picker.windowSize * root.picker.windowLead))
            root.backend.window(Math.floor(start), root.picker.windowSize)
        }
    }

    // Only the visible tiles, only once each, and only after the grid has stopped moving.
    // Unknown storage holds until fsinfo names it, so the first screen never decodes as local.
    function requestThumbs() {
        if (!root.visible || root.picker.backendUnavailable || root.picker.total === 0
                || root.picker.pendingListings > 0 || root.picker.recent || !root.picker.storageKnown)
            return
        var range = Picker.tileRange(root.contentY, root.cellHeightPx, root.visibleTileRows, root.columns, root.picker.total)
        var work = Thumbs.plan(root.picker.thumbState, root.picker.rows, root.picker.held,
                               range.first, range.last, ViewState.thumbnailMode)
        root.backend.thumbcancel(work.drop)
        // An off class still asks; the worker answers from the cache alone, see ui/js/ExtThumbs.js.
        work.cacheOnly = ExtThumbs.cacheOnly(root.picker.storageClass, ViewState.preview)
        root.backend.thumb(work.ask, work.cacheOnly)
        if (work.ask.length > 0)
            settle.interval = root.settleMs
        root.thumbsApplied(work)
    }

    signal thumbsApplied(var work)

    Connections {
        target: root.picker
        function onRowsChanged() { if (root.visible) settle.restart() }
        function onTotalChanged() { if (root.visible) { root.primeSettle(); settle.restart() } }
    }

    Connections {
        target: ViewState
        function onThumbnailModeChanged() { if (root.visible) settle.restart() }
        function onThumbnailPixelsChanged() { if (root.visible) settle.restart() }
        function onPreviewChanged() { if (root.visible) settle.restart() }
    }
}
