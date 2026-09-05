import QtQuick
import "." as Flea
import "js/DirSizes.js" as DirSizes
import "js/Tap.js" as Tap
import "js/Thumbs.js" as Thumbs

// The grid view. Same rows, same marks, same thumbnails as the list; only the geometry differs, so
// the viewport maths is the list's own with a tile row standing in for a text row.
GridView {
    id: root

    property var pane: null
    property var menu: null

    signal thumbsApplied(var work)
    signal dirSizesApplied(var ask)
    signal dirSizesCancelled()

    // How many tiles fit across, which is what a cursor step down has to move by.
    readonly property int columns: Math.max(1, Math.floor(root.width / Theme.grid.minCellWidth))
    readonly property int tileRows: Math.max(1, Math.ceil(root.pane.total / root.columns))
    readonly property int tileWidth: Math.max(1, Math.floor(root.width / root.columns))
    // Square thumb filling the cell width, less the tile's own gap, then a gap and the caption.
    readonly property int thumbPx: Math.max(1, root.tileWidth - 2 * Theme.spacing.gap)
    // Two caption lines: the name wraps to maximumLineCount 2, and one line left the second
    // painting through the tile below.
    readonly property int captionPx: Math.round(Theme.font.caption * 1.6) * 2
    readonly property int cellHeightPx: root.thumbPx + Theme.spacing.gap
                                        + root.captionPx
                                        + Theme.spacing.gap
    readonly property int visibleTileRows: Math.max(1, Math.ceil(root.height / root.cellHeightPx))

    focus: true
    model: pane.total
    clip: true
    cellWidth: root.tileWidth
    cellHeight: root.cellHeightPx
    cacheBuffer: root.cellHeightPx * 2
    boundsBehavior: Flickable.StopAtBounds
    reuseItems: true

    delegate: Flea.GridTile {
        required property int index
        width: root.cellWidth
        height: root.cellHeight
        row: root.pane.rowFor(index)
        cursor: index === root.pane.cursorIndex
        hovered: hover.hovered
        selected: root.pane.isSelected(index)
        thumb: Thumbs.fileFor(root.pane.thumbState, index)
        slotSize: root.thumbPx

        HoverHandler {
            id: hover
        }

        TapHandler {
            id: tap
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onTapped: function (eventPoint, button) {
                if (button === Qt.RightButton)
                    Tap.tappedMenu(index, eventPoint, root.pane, root.menu)
                else
                    Tap.tapped(index, tap.tapCount, tap.point.modifiers, root.pane)
            }
        }
    }

    onContentYChanged: {
        root.menu.close()
        if (DirSizes.hasPending(root.pane.dirSizeState)) {
            root.pane.backend.dirsizecancel()
            root.dirSizesCancelled()
        }
        coalesce.restart()
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
        if (root.pane.total === 0)
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
            last: Math.min(root.pane.total - 1, (view.last + 1) * root.columns - 1)
        }
    }

    function requestThumbs() {
        if (root.pane.total === 0 || root.pane.listInFlight)
            return
        var range = root.visibleRange()
        var work = Thumbs.plan(root.pane.thumbState, root.pane.rows, root.pane.held, range.first, range.last)
        root.pane.backend.thumbcancel(work.drop)
        root.pane.backend.thumb(work.ask)
        root.thumbsApplied(work)
    }

    function requestDirSizes() {
        if (root.pane.total === 0 || root.pane.listInFlight)
            return
        var range = root.visibleRange()
        var ask = DirSizes.plan(root.pane.dirSizeState, root.pane.rows, root.pane.held, range.first, range.last)
        if (ask.length > 0)
            root.pane.backend.dirsize(ask)
        root.dirSizesApplied(ask)
    }
}
