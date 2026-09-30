import QtQuick
import "." as Flea
import "js/DirSizes.js" as DirSizes
import "js/ExtThumbs.js" as ExtThumbs
import "js/Filter.js" as Filter
import "js/Tap.js" as Tap
import "js/Thumbs.js" as Thumbs

// The listing's render, scroll and settle-triggered refetch, split out of Pane.qml; reaches Pane's state through the pane reference and the context menu through menu, both handed in at instantiation.
ListView {
    id: root

    property var pane: null
    property var menu: null

    // Pane owns cursorIndex, thumbState and dirSizeState; List only computes what changed and hands it back.
    // Both ends of cursorClamped are view positions, not listing rows: under a filter they differ.
    signal cursorClamped(int first, int last)
    signal thumbsApplied(var work)
    signal dirSizesApplied(var ask)
    signal dirSizesCancelled()

    focus: true
    model: pane.shownTotal
    clip: true
    cacheBuffer: Theme.fileRowHeight * pane.cacheRows
    boundsBehavior: Flickable.StopAtBounds
    highlightMoveDuration: 0
    // Every property the delegate draws is a binding on index, so a row leaving the buffer is re-bound rather than rebuilt.
    reuseItems: true

    Flea.FastScrollHandler {
        parent: root
        flickable: root
    }

    readonly property alias scrollBar: verticalScroll
    Flea.ViewportScrollBar {
        id: verticalScroll
        parent: root
        anchors { top: parent.top; right: parent.right }
        flickable: root
    }

    Flea.SelectionBand {
        parent: root
        pane: root.pane
        flickable: root
    }

    Flea.FileDrag {
        id: dragSession
        pane: root.pane
    }

    delegate: Flea.Row {
        id: cell
        required property int index
        // index is where the row is drawn; listingIndex is the row the backend numbers, and under a
        // filter the two are different. Everything that leaves this delegate takes the listing one.
        readonly property int listingIndex: Filter.at(root.pane.shown, index)
        width: root.width
        row: root.pane.rowFor(listingIndex)
        cursor: listingIndex === root.pane.cursorIndex
        paneFocused: root.pane.paneFocused
        dualMode: root.pane.dualMode
        hiddenCols: root.pane.dualMode ? ["mode", "kind"].concat(ViewState.hiddenCols) : ViewState.hiddenCols
        hovered: hover.hovered
        thumb: root.thumbFor(listingIndex)
        selected: root.pane.isSelected(listingIndex)
        kindNames: root.pane.kindNames
        dirSize: root.dirSizeFor(listingIndex)
        // A filter paints its run the same way a search does; filtering below is what keeps the
        // ordinary columns, because these rows are still this directory's own and not walk results.
        searchQuery: root.pane.searchMode.length > 0 ? root.pane.searchQuery : root.pane.filterQuery
        filtering: root.pane.shown !== null
        renaming: listingIndex === root.pane.renamingIndex
        renamePane: root.pane
        // -1 is also what Filter.at answers for a stale delegate, so an idle list must never light one.
        dropTarget: dragSession.dropIndex >= 0 && listingIndex === dragSession.dropIndex
        dropCopying: dragSession.dragCopy

        onRenameCommitted: function (newName) { root.pane.commitRename(newName) }
        onRenameAbandoned: root.pane.renamingIndex = -1

        HoverHandler {
            id: hover
            enabled: root.pane.selectionBand === null
        }

        TapHandler {
            id: tap
            enabled: !cell.renaming
            acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
            onTapped: function (eventPoint, button) {
                if (button === Qt.MiddleButton)
                    Tap.tappedTab(root.pane.rowFor(listingIndex), root.pane.path, root.pane)
                else if (button === Qt.RightButton)
                    Tap.tappedMenu(listingIndex, eventPoint, root.pane, root.menu)
                else
                    Tap.tapped(listingIndex, tap.tapCount, tap.point.modifiers, root.pane)
            }
        }

        Flea.RowDrag {
            session: dragSession
            listingIndex: cell.listingIndex
            row: cell.row
        }
    }


    // The listing keeps one row of bare ground at its end. SearchFilter rule 1 took the filter's
    // sentence off this slot, and the ground under it is not the sentence: it is where a band starts
    // and where the background menu is raised, and a listing whose last row sits flush on the bottom
    // edge offers neither once it is scrolled to the end.
    footer: Item {
        width: root.width
        height: Theme.chromeHeight
    }

    // Empty space under the last row belongs to the directory, not to a row, so it raises the
    // background menu. Tap.onBackground says the point missed every delegate, which is what leaves a
    // right click on a row to that row's own handler above.
    TapHandler {
        acceptedButtons: Qt.RightButton
        onTapped: function (eventPoint) {
            if (Tap.onBackground(root, eventPoint))
                root.menu.openBackground(eventPoint.scenePosition)
        }
    }

    onContentYChanged: {
        // The wheel moves the view and not the cursor, so the cursor follows the viewport here.
        var first = Math.floor(root.contentY / Theme.fileRowHeight)
        var last = Math.min(root.pane.shownTotal - 1, first + root.pane.visibleRows - 1)
        if (root.pane.renamingIndex >= 0) {
            var range = root.visibleRange()
            first = range.first
            last = range.last
        }
        if (last >= first && root.pane.selectionBand === null) {
            root.cursorClamped(first, last)
        }
        root.menu.close()
        // Gated on hasPending (see DirSizes.js) rather than diffed at settle like thumbcancel, see docs/protocol.md "dirsizecancel".
        if (DirSizes.hasPending(root.pane.dirSizeState)) {
            root.pane.backend.dirsizecancel()
            root.dirSizesCancelled()
        }
        coalesce.start()
        settle.restart()
    }

    function visibleRange() {
        var fallback = Thumbs.viewport(root.contentY, Theme.fileRowHeight, root.pane.visibleRows, root.pane.shownTotal)
        // The retained error caption expands one row; query actual delegates while it is present.
        if (root.pane.renamingIndex >= 0) {
            var first = root.indexAt(0, root.contentY)
            var last = root.indexAt(0, root.contentY + root.height - 1)
            first = first < 0 ? fallback.first : first
            return {first: first, last: last < 0 ? Math.min(root.count - 1, first + root.pane.visibleRows) : last}
        }
        return fallback
    }

    // Pane's own open() and its Connections.onRows reach these two through the wrapper functions below.
    Timer {
        id: coalesce
        interval: root.pane.coalesceMs
        repeat: false
        onTriggered: root.requestIfDrifted()
    }

    // Resize and filter changes can change the visible work without moving contentY.
    Connections {
        target: root.pane
        function onVisibleRowsChanged() { settle.restart() }
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
    }

    Timer {
        id: settle
        interval: root.pane.firstSettleMs
        repeat: false
        onTriggered: { root.requestThumbs(); root.requestDirSizes() }
    }

    // Pane.open() primes the next settle to the short first-screen interval before any row arrives.
    function primeSettle() { settle.interval = root.pane.firstSettleMs }
    function restartCoalesce() { coalesce.restart() }
    function restartSettle() { settle.restart() }

    // Only the visible rows, only once each, and only after the list has stopped moving.
    // Unknown storage holds until fsinfo names it, so the first screen never decodes as local.
    function requestThumbs() {
        if (!root.visible || root.pane.listInFlight || !root.pane.storageKnown)
            return
        var view = root.visibleRange()
        // A filtered viewport covers a set and not a run, so the run it spans is what the planner
        // gets and Filter.cut takes back every row inside that run the filter is hiding.
        var span = Filter.span(root.pane.shown, view.first, view.last)
        var work = Filter.cut(Thumbs.plan(root.pane.thumbState, root.pane.rows, root.pane.held, span.first, span.last, ViewState.thumbnailMode), root.pane.shown, root.pane.thumbState)
        work.drop = work.drop.filter(function (index) { return index !== root.pane.previewIndex })
        root.pane.backend.thumbcancel(work.drop)
        // An off class still asks; the backend answers from the cache alone, see ui/js/ExtThumbs.js.
        work.cacheOnly = ExtThumbs.cacheOnly(root.pane.storageClass, ViewState.preview)
        root.pane.backend.thumb(work.ask, work.cacheOnly)
        // The short first settle latches to the fling debounce only once a request has actually gone out.
        if (work.ask.length > 0)
            settle.interval = root.pane.settleMs
        root.thumbsApplied(work)
    }

    function thumbFor(index) {
        return Thumbs.allowed(root.pane.rowFor(index), ViewState.thumbnailMode) ? Thumbs.fileFor(root.pane.thumbState, index) : ""
    }

    function dirSizeFor(index) {
        return DirSizes.sizeFor(root.pane.dirSizeState, index)
    }

    // Same idiom as requestThumbs, minus a cancel: onContentYChanged already sent it, see above.
    function requestDirSizes() {
        if (!root.visible || root.pane.shownTotal === 0 || root.pane.listInFlight)
            return
        // Thumbs.viewport() is reused: it takes no thumb-specific state, only geometry.
        var view = root.visibleRange()
        var span = Filter.span(root.pane.shown, view.first, view.last)
        var ask = Filter.keep(DirSizes.plan(root.pane.dirSizeState, root.pane.rows, root.pane.held, span.first, span.last, ViewState.thumbnailMode), root.pane.shown)
        if (ask.length > 0) {
            root.pane.backend.dirsize(ask)
            settle.interval = root.pane.settleMs
        }
        root.dirSizesApplied(ask)
    }

    // Handle an empty held window explicitly before applying held-edge arithmetic.
    function requestIfDrifted() {
        // No window while a filter narrows rows already held, nor while a listing is out, whose windows are the directory asked for.
        if (root.pane.total === 0 || root.pane.shown !== null || root.pane.listInFlight)
            return
        var firstVisible = Math.floor(root.contentY / Theme.fileRowHeight)
        var lastVisible = firstVisible + root.pane.visibleRows
        if (root.pane.renamingIndex >= 0) {
            var range = root.visibleRange()
            firstVisible = range.first
            lastVisible = range.last + 1
        }
        if (root.pane.rows.length === 0) {
            root.requestAround(firstVisible)
            return
        }
        var heldEnd = root.pane.held + root.pane.rows.length
        if (firstVisible - root.pane.held < root.pane.refetchMargin && root.pane.held > 0) {
            root.requestAround(firstVisible)
        } else if (heldEnd - lastVisible < root.pane.refetchMargin && heldEnd < root.pane.total) {
            root.requestAround(firstVisible)
        }
    }

    function requestAround(firstVisible) {
        var start = Math.max(0, firstVisible - root.pane.buffer)
        root.pane.backend.window(start, root.pane.windowSize)
    }
}
