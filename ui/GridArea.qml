import QtQuick
import "." as Flea
import "js/DirSizes.js" as DirSizes
import "js/Filter.js" as Filter
import "js/Focus.js" as Focus
import "js/Tap.js" as Tap
import "js/Thumbs.js" as Thumbs

// The grid view. Same rows, same marks, same thumbnails as the list; only the geometry differs, so
// the viewport maths is the list's own with a tile row standing in for a text row.
GridView {
    id: root

    property var pane: null
    property var menu: null

    property real zoomTravel: 0
    readonly property int wheelNotch: 120
    readonly property int touchpadStep: 48

    function zoomWheel(wheel) {
        if (!ViewState.ctrlZoom) return false
        root.zoomTravel += wheel.pixelDelta.y !== 0 ? wheel.pixelDelta.y / root.touchpadStep
                                                   : wheel.angleDelta.y / root.wheelNotch
        var steps = root.zoomTravel > 0 ? Math.floor(root.zoomTravel) : Math.ceil(root.zoomTravel)
        if (steps !== 0) {
            root.zoomTravel -= steps
            var sizes = ["small", "medium", "large", "xlarge"]
            var next = Math.max(0, Math.min(sizes.length - 1, sizes.indexOf(ViewState.thumbnailSize) + steps))
            ViewState.changeSetting("preview.thumbSize", sizes[next])
        }
        return true
    }

    signal thumbsApplied(var work)
    signal dirSizesApplied(var ask)
    signal dirSizesCancelled()

    // How many tiles fit across, which is what a cursor step down has to move by.
    readonly property int columns: Math.max(1, Math.floor(root.width / Math.max(Theme.grid.minCellWidth, ViewState.thumbnailPixels + 2 * Theme.spacing.rowPaddingX)))
    readonly property int tileRows: Math.max(1, Math.ceil(root.pane.shownTotal / root.columns))
    // Mark, one gap, two caption lines, and the padding above and below.
    readonly property int cellHeightPx: ViewState.thumbnailPixels + Theme.spacing.gap
                                        + Math.ceil(Theme.grid.captionHeight)
                                        + 2 * Theme.spacing.rowPaddingX
                                        + renameExtraHeight
    readonly property real renameExtraHeight: {
        if (!root.pane || root.pane.renamingIndex < 0) return 0
        var cell = root.itemAtIndex(Filter.viewOf(root.pane.shown, root.pane.renamingIndex))
        return cell ? cell.renameExtraHeight : 0
    }
    readonly property int visibleTileRows: Math.max(1, Math.ceil(root.height / root.cellHeightPx))
    onColumnsChanged: if (root.visible) settle.restart()
    onVisibleTileRowsChanged: if (root.visible) settle.restart()

    focus: true
    // Whichever view is up owns the keyboard, and Focus.handleKey is the one route all three take.
    Keys.onPressed: function (event) { event.accepted = Focus.handleKey(event, root.pane, root.pane.sidebar) }
    model: pane.shownTotal
    clip: true
    cellWidth: Math.floor(root.width / root.columns)
    cellHeight: root.cellHeightPx
    cacheBuffer: root.cellHeightPx * 2
    boundsBehavior: Flickable.StopAtBounds
    reuseItems: true

    Flea.FastScrollHandler {
        parent: root
        flickable: root
        ctrlWheelAction: function (wheel) { return root.zoomWheel(wheel) }
    }

    Flea.SelectionBand {
        parent: root
        pane: root.pane
        flickable: root
        columns: root.columns
        cellWidth: root.cellWidth
        cellHeight: root.cellHeight
    }

    Flea.FileDrag {
        id: dragSession
        pane: root.pane
    }

    delegate: Flea.GridTile {
        id: cell
        required property int index
        // A filtered tile position still acts on the backend row whose identity it displays.
        readonly property int listingIndex: Filter.at(root.pane.shown, index)
        width: root.cellWidth
        height: root.cellHeight
        row: root.pane.rowFor(listingIndex)
        cursor: listingIndex === root.pane.cursorIndex
        hovered: hover.hovered
        selected: root.pane.isSelected(listingIndex)
        dropTarget: dragSession.dropIndex >= 0 && listingIndex === dragSession.dropIndex
        dropCopying: dragSession.dragCopy
        thumb: Thumbs.allowed(row, ViewState.thumbnailMode) ? Thumbs.fileFor(root.pane.thumbState, listingIndex) : ""
        renaming: listingIndex >= 0 && listingIndex === root.pane.renamingIndex
        renamePane: root.pane
        onRenameCommitted: function(newName) { root.pane.commitRename(newName) }
        onRenameAbandoned: root.pane.renamingIndex = -1

        HoverHandler {
            id: hover
            enabled: root.pane.selectionBand === null
        }

        TapHandler {
            id: tap
            enabled: !cell.renaming
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onTapped: function (eventPoint, button) {
                if (cell.listingIndex < 0) return
                if (button === Qt.RightButton)
                    Tap.tappedMenu(cell.listingIndex, eventPoint, root.pane, root.menu)
                else
                    Tap.tapped(cell.listingIndex, tap.tapCount, tap.point.modifiers, root.pane)
            }
        }

        Flea.RowDrag {
            session: dragSession
            listingIndex: cell.listingIndex
            row: cell.row
        }
    }

    footer: Item {
        width: root.width
        height: note.text.length > 0 ? Theme.chromeHeight : 0
        Text {
            id: note
            anchors.fill: parent
            anchors.leftMargin: Theme.spacing.rowPaddingX
            anchors.rightMargin: Theme.spacing.rowPaddingX
            verticalAlignment: Text.AlignVCenter
            text: Filter.note(root.pane.shown, root.pane.rows.length, root.pane.filterQuery)
            color: Theme.color.muted
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            elide: Text.ElideRight
            textFormat: Text.PlainText
        }
    }

    // The space past the last tile is the directory's own, the same rule ui/List.qml carries: a
    // tile's right click belongs to its delegate, and indexAt is what tells the two apart.
    TapHandler {
        acceptedButtons: Qt.RightButton
        onTapped: function (eventPoint) {
            if (root.indexAt(root.contentX + eventPoint.position.x, root.contentY + eventPoint.position.y) < 0)
                root.menu.openBackground(eventPoint.scenePosition)
        }
    }

    onContentYChanged: {
        root.menu.close()
        if (DirSizes.hasPending(root.pane.dirSizeState)) {
            root.pane.backend.dirsizecancel()
            root.dirSizesCancelled()
        }
        coalesce.start()
        settle.restart()
    }

    // The same drift check the list runs: a grid scrolled past the held window would otherwise draw
    // empty tiles for rows the backend has never been asked for.
    Timer {
        id: coalesce
        interval: root.pane.coalesceMs
        repeat: false
        onTriggered: root.requestIfDrifted()
    }

    Timer {
        id: settle
        interval: root.pane.settleMs
        repeat: false
        onTriggered: { root.requestThumbs(); root.requestDirSizes() }
    }

    function restartSettle() { settle.restart() }
    function restartCoalesce() { coalesce.restart() }
    function primeSettle() { settle.interval = root.pane.firstSettleMs }

    function requestIfDrifted() {
        if (root.pane.total === 0 || root.pane.shown !== null)
            return
        var range = root.visibleRange()
        if (root.pane.rows.length === 0) {
            root.requestAround(range.first)
            return
        }
        var heldEnd = root.pane.held + root.pane.rows.length
        if (range.first - root.pane.held < root.pane.refetchMargin && root.pane.held > 0) {
            root.requestAround(range.first)
        } else if (heldEnd - range.last < root.pane.refetchMargin && heldEnd < root.pane.total) {
            root.requestAround(range.first)
        }
    }

    function requestAround(firstVisible) {
        var start = Math.max(0, firstVisible - root.pane.buffer)
        root.pane.backend.window(start, root.pane.windowSize)
    }

    // Thumbs.viewport takes geometry and no thumb-specific state, so a tile row is handed to it the
    // same way a text row is; the answer is then multiplied out into item indices.
    function visibleRange() {
        var view = Thumbs.viewport(root.contentY, root.cellHeightPx, root.visibleTileRows, root.tileRows)
        return {
            first: view.first * root.columns,
            last: Math.min(root.pane.shownTotal - 1, (view.last + 1) * root.columns - 1)
        }
    }

    Connections {
        target: root.pane
        function onFilterQueryChanged() {
            if (!root.visible) return
            var work = Filter.cut({ask: [], drop: []}, root.pane.shown, root.pane.thumbState)
            work.drop = work.drop.filter(function(index) { return index !== root.pane.previewIndex })
            if (work.drop.length > 0) {
                root.pane.backend.thumbcancel(work.drop)
                root.thumbsApplied(work)
            }
            if (DirSizes.hasPending(root.pane.dirSizeState)) {
                root.pane.backend.dirsizecancel()
                root.dirSizesCancelled()
            }
            settle.restart()
        }
    }

    Connections {
        target: ViewState
        function onThumbnailModeChanged() { if (root.visible) settle.restart() }
        function onThumbnailPixelsChanged() { if (root.visible) settle.restart() }
    }

    function requestThumbs() {
        if (!root.visible || root.pane.listInFlight)
            return
        var range = root.visibleRange()
        var span = Filter.span(root.pane.shown, range.first, range.last)
        var work = Filter.cut(Thumbs.plan(root.pane.thumbState, root.pane.rows, root.pane.held, span.first, span.last, ViewState.thumbnailMode), root.pane.shown, root.pane.thumbState)
        work.drop = work.drop.filter(function (index) { return index !== root.pane.previewIndex })
        root.pane.backend.thumbcancel(work.drop)
        root.pane.backend.thumb(work.ask)
        root.thumbsApplied(work)
    }

    function requestDirSizes() {
        if (!root.visible || root.pane.shownTotal === 0 || root.pane.listInFlight)
            return
        var range = root.visibleRange()
        var span = Filter.span(root.pane.shown, range.first, range.last)
        var ask = Filter.keep(DirSizes.plan(root.pane.dirSizeState, root.pane.rows, root.pane.held, span.first, span.last), root.pane.shown)
        if (ask.length > 0)
            root.pane.backend.dirsize(ask)
        root.dirSizesApplied(ask)
    }
}
