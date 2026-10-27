import QtQuick
import "." as Flea
import "js/ClipMarks.js" as ClipMarks
import "js/DirSizes.js" as DirSizes
import "js/ExtThumbs.js" as ExtThumbs
import "js/Filter.js" as Filter
import "js/FilterWire.js" as FilterWire
import "js/Scroll.js" as Scroll
import "js/ScrollOff.js" as ScrollOff
import "js/Tap.js" as Tap
import "js/Thumbs.js" as Thumbs

// The listing's render, scroll and settle-triggered refetch, split out of Pane.qml; reaches Pane's state through the pane reference and the context menu through menu, both handed in at instantiation.
ListView {
    id: root

    property var pane: null
    readonly property bool fileDragActive: dragSession.Drag.active
    property var menu: null

    // Parked rows built ahead of the next landing; a row parks invisible and the landing shows it again.
    readonly property Flea.PrebuildDriver prebuild: Flea.PrebuildDriver {
        view: root
        model: stable
        pane: root.pane
        limit: stable.warmRows
        held: root.hiddenHeld
        park: function (cell) { cell.live = false; cell.visible = false }
        adopt: function (cell) { cell.live = true; cell.visible = true; cursorMark.place(cell) }
    }

    // Shared once, read plainly per row, so no delegate builds its own width or array.
    property real rowWidth: Scroll.contentWidth(root.width, Theme.spacing.rowPaddingX)
    // Dual mode hides mode and kind on top of the stored set; one array, never one per row.
    property var rowHiddenCols: root.pane && root.pane.dualMode ? ["mode", "kind"].concat(ViewState.hiddenCols) : ViewState.hiddenCols
    // Shared name budgets remove the per-row floor; Row keeps its local default for PickerList and drop targets.
    readonly property bool listDual: root.pane && root.pane.dualMode ? true : false
    readonly property real listMarkSlot: root.listDual ? Theme.markSize : Theme.iconSize
    readonly property real listSizeWidth: root.listDual ? Theme.dualColumn.size : Theme.column.size
    readonly property real listDateWidth: root.listDual ? Theme.dualColumn.date : Theme.column.date
    // The same set Row.cols resolves, from the same width and hidden array, so the two cannot drift.
    readonly property var listCols: root.pane && root.pane.recentMode.length > 0 ? Theme.columns(root.rowWidth, root.rowHiddenCols, root.listDateWidth, true, root.listDual) : root.listDual ? Theme.dualColumns(root.rowWidth, root.rowHiddenCols) : Theme.columns(root.rowWidth, root.rowHiddenCols, root.listDateWidth)
    readonly property bool listModeShown: root.listCols ? !!root.listCols.mode : false
    readonly property bool listSizeShown: root.listCols ? !!root.listCols.size : false
    readonly property bool listDateShown: root.listCols ? !!root.listCols.date : false
    readonly property bool listKindShown: root.listCols ? !!root.listCols.kind : false
    // Matches Theme.clipMarkSize; the reserve keeps the glyph readable.
    readonly property int listClipPx: Theme.clipMarkSize
    // Exact drawn geometry: the row less padding, mark, gap, columns, gaps and name-to-mode gap, in pixels then floored.
    readonly property real nameSlotPlainPx: Math.max(0, root.rowWidth - 2 * Theme.spacing.rowPaddingX - root.listMarkSlot - Theme.spacing.gap - (root.listModeShown ? Theme.column.mode : 0) - (root.listSizeShown ? root.listSizeWidth : 0) - (root.listDateShown ? root.listDateWidth : 0) - (root.listKindShown ? Theme.column.kind : 0) - (root.listKindShown ? Theme.spacing.gap : 0) - ((root.listDateShown && !root.listDual) ? Theme.spacing.gap : 0) - ((root.listSizeShown && !root.listDual) ? Theme.spacing.gap : 0) - (root.listModeShown ? Theme.spacing.gap : 0))
    readonly property real nameSlotClipPx: Math.max(0, root.nameSlotPlainPx - Theme.spacing.gap - root.listClipPx)
    readonly property int nameBudgetPlain: Theme.bodyAdvance > 0 && root.nameSlotPlainPx > 0 ? Math.floor(root.nameSlotPlainPx / Theme.bodyAdvance) : -1
    readonly property int nameBudgetClip: Theme.bodyAdvance > 0 && root.nameSlotClipPx > 0 ? Math.floor(root.nameSlotClipPx / Theme.bodyAdvance) : -1
    property bool clipEmpty: ClipMarks.isEmpty(root.pane ? root.pane.clipboard : null)
    // The short-circuit below is the only release while empty, so a cut of a large directory frees its lookup here.
    onClipEmptyChanged: { if (root.clipEmpty) ClipMarks.release() }

    // Pane owns cursorIndex, thumbState and dirSizeState; List only computes what changed and hands it back.
    // Both ends of cursorClamped are view positions, not listing rows: under a filter they differ.
    signal cursorClamped(int first, int last)
    signal thumbsApplied(var work)
    signal dirSizesApplied(var ask)
    signal dirSizesCancelled()

    focus: true
    keyNavigationEnabled: false
    // Hidden holds no delegates; an edit the List held when it hid keeps its own per RenameField.qml.
    model: Flea.StableModel { id: stable; view: root; total: (root.visible || root.renameHeld) ? root.pane.shownTotal : 0
                              warm: root.visible && root.pane.searchMode.length > 0 && root.pane.swap.searchWarm === true; warmRows: Math.floor((root.height + root.cacheBuffer) / Theme.fileRowHeight) + 1 }
    clip: true
    cacheBuffer: Theme.fileRowHeight * pane.cacheRows
    boundsBehavior: Flickable.StopAtBounds
    highlightMoveDuration: 0
    // Every property the delegate draws binds to index and cursor is refiled by listingIndex, so a row leaving the buffer is re-bound rather than rebuilt.
    reuseItems: true
    // The one row drawn as the cursor; a cursor move touches two delegates, not every live one.
    Flea.CursorMark { id: cursorMark; row: root.pane.cursorIndex }

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
        // Parked while pooled: each listing read returns its own last value until onReused rebinds it before the next frame; a row holding the open editor stays live, since a prebuild create pools and reuses the shown rows and pooling would destroy the editor.
        property bool live: true
        ListView.onPooled: if (!cell.renaming) cell.live = false
        ListView.onReused: {
            // The driver's create can take a pooled row past the shown ones; it parks as a built one does.
            if (cell.index >= root.count) { root.prebuild.parkCell(cell); return }
            cell.live = true
            root.prebuild.adoptCell(cell)
        }
        // Built ahead of a landing past the shown rows: parked invisible until the landing reuses it.
        property bool prebuilt: false
        Component.onDestruction: root.prebuild.gone(cell)
        // index is where the row is drawn; listingIndex is the row the backend numbers, and under a
        // filter the two are different. Everything that leaves this delegate takes the listing one.
        readonly property int listingIndex: cell.live ? Filter.at(root.pane.shown, index) : -1
        // Filed by listing row in the view's cursor mark, which sets cursor; see ui/CursorMark.qml.
        property int markRow: -1
        Component.onCompleted: if (!root.prebuild.built(cell)) cursorMark.place(cell)
        onListingIndexChanged: cursorMark.place(cell)
        // The scroll lane stays clear so rows never reflow under the bar.
        width: root.rowWidth
        paintWidth: root.width
        row: cell.live ? root.pane.rowFor(listingIndex) : cell.row
        paneFocused: root.pane.paneFocused
        dualMode: root.pane.dualMode
        hiddenCols: root.rowHiddenCols
        // One Columns.set per List state, shared by every delegate; Row keeps its local default for PickerList and drop targets.
        assignedCols: root.listCols
        hovered: hover.hovered
        thumb: cell.live ? root.thumbFor(listingIndex) : cell.thumb
        selected: cell.live && root.pane.isSelected(listingIndex)
        // Empty short-circuits before the library, so no row calls it while the clipboard is empty.
        clipMark: !cell.live ? cell.clipMark : root.clipEmpty ? "" : ClipMarks.markForRow(root.pane, cell.row ? cell.row.n : "", root.pane.clipboard)
        // Shared budgets: one floor per List state, not per row; drop rows keep local measured geometry in Row.
        assignedNameBudget: cell.clipMark.length > 0 ? root.nameBudgetClip : root.nameBudgetPlain
        // The highlight content builds with the filter strip, so no keystroke pays for it; mirrors the strip's own up rule.
        markArmed: root.pane.filterTyping || root.pane.filterQuery.length > 0
        // A search's location layout builds hidden after the strip's first frame, so no answer pays for it.
        locateArmed: root.pane.searchMode.length > 0 && root.pane.swap.searchWarm === true
        // A filter window's row names its listing row as r and indexes that window's own Kind dictionary (issue 315).
        kindNames: !cell.live ? cell.kindNames : cell.row && cell.row.r !== undefined ? root.pane.filterKinds : root.pane.kindNames
        dirSize: cell.live ? root.dirSizeFor(listingIndex) : cell.dirSize
        // A filter paints its run the same way a search does; filtering below is what keeps the
        // ordinary columns, because these rows are still this directory's own and not walk results.
        searchQuery: !cell.live ? cell.searchQuery : root.pane.searchMode.length > 0 ? root.pane.swap.answeredQuery : root.pane.filterQuery
        filtering: cell.live ? root.pane.shown !== null : cell.filtering
        // Sidebar040: Recent splits its path into Name and the fixed Location column.
        recenting: cell.live ? root.pane.recentMode.length > 0 : cell.recenting
        renaming: listingIndex >= 0 && listingIndex === root.pane.renamingIndex
        renamePane: root.pane
        // -1 is also what Filter.at answers for a stale delegate, so an idle list must never light one.
        dropTarget: dragSession.dropIndex >= 0 && listingIndex === dragSession.dropIndex
        dropCopying: dragSession.dragCopy
        dropLinking: dragSession.dragLink

        onRenameCommitted: function (newName) { root.pane.commitRename(newName) }
        onRenameAbandoned: root.pane.renamingIndex = -1

        HoverHandler {
            id: hover
            // Disabled while pooled, so a reused row starts unhovered as a new one does.
            enabled: cell.live && root.pane.selectionBand === null
        }

        TapHandler {
            id: tap
            enabled: !cell.renaming
            acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
            // A press held past the interval must not fire while the button is
            // still down on a row about to be dragged; the release still arms.
            onPressedChanged: if (pressed) root.pane.pressSlowClick()
            onTapped: function (eventPoint, button) {
                if (button === Qt.MiddleButton)
                    Tap.tappedTab(root.pane.rowFor(listingIndex), root.pane.path, root.pane)
                else if (button === Qt.RightButton)
                    Tap.tappedMenu(listingIndex, eventPoint, root.pane, root.menu)
                else {
                    var wasSole = root.pane.slowClickWasSole(listingIndex)
                    Tap.tapped(listingIndex, tap.tapCount, tap.point.modifiers, root.pane)
                    Tap.slowTap(root.pane, tap.tapCount === 1 && Tap.onName(cell.nameItem(), cell, eventPoint.position, false),
                                listingIndex, tap.point.modifiers, dragSession.Drag.active, wasSole)
                }
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
        width: root.rowWidth
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
        // Hidden geometry and a restore in flight move no shared state.
        if (!root.visible || root.hiddenHeld)
            return
        // The cursor follows the bounded view, silent while that view did not move.
        var at = Scroll.bounded(root.contentY, root.originY, root.contentHeight, root.height)
        // Qt shifts originY when expanded delegates collapse; row offsets start at that origin.
        var first = Math.floor((at - root.originY) / Theme.fileRowHeight)
        var last = Math.min(root.pane.shownTotal - 1, first + root.pane.visibleRows - 1)
        if (root.pane.renamingIndex >= 0) {
            var range = root.visibleRange()
            first = range.first
            last = range.last
        }
        if (last >= first && root.pane.selectionBand === null && at !== root._clampAt) {
            root._clampAt = at
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
        var fallback = Thumbs.viewport(root.contentY - root.originY, Theme.fileRowHeight, root.pane.visibleRows, root.pane.shownTotal)
        // The retained error caption expands one row; query actual delegates while it is present.
        if (root.pane.renamingIndex >= 0) {
            var first = root.indexAt(0, root.contentY)
            var last = root.indexAt(0, root.contentY + root.height - 1)
            first = first < 0 ? fallback.first : first
            return {first: first, last: last < 0 ? Math.min(root.count - 1, first + root.pane.visibleRows) : last}
        }
        return fallback
    }

    // Returning parks the view on the shared cursor once the reset rows land.
    property bool hiddenHeld: false
    // True while the List's own edit is open, so a hide keeps it in any binding order; a rename in Grid or Columns leaves it false.
    property bool renameHeld: false
    // The bounded view last emitted; a bounce holds it, so the cursor stays put.
    property var _clampAt
    // Last parked contentY and loop turns spent; only a value stable across turns ends the hold.
    property real restoreY: -1
    property int restoreTicks: 0
    onVisibleChanged: {
        if (!root.visible) {
            coalesce.stop()
            settle.stop()
            root.hiddenHeld = true
            root.restoreY = -1
            root.restoreTicks = 0
            return
        }
        root.renameHeld = root.pane ? root.pane.renamingIndex >= 0 : false
        if (root.hiddenHeld)
            root.restoreCursorView()
    }
    onCountChanged: {
        if (root.hiddenHeld && root.visible && root.count > 0)
            root.restoreCursorView()
        root.prebuild.landed()
    }
    onCurrentIndexChanged: root.prebuild.kick()
    function restoreCursorView() {
        // A queued turn arriving after the hold ended is stale.
        if (!root.hiddenHeld || !root.visible)
            return
        if (!root.pane) {
            root.hiddenHeld = false
            return
        }
        // The model reset may still be landing; a later turn retries.
        if (root.count === 0 && root.pane.shownTotal > 0) {
            root.deferRestore()
            return
        }
        // An empty listing and a filtered-away cursor have no view to park, so the hold ends here.
        var view = Filter.viewOf(root.pane.shown, root.pane.cursorIndex)
        if (root.count === 0 || view < 0) {
            root.hiddenHeld = false
            return
        }
        root.forceLayout()
        root.positionViewAtIndex(view, ListView.Contain)
        // Only a value stable across turns ends the hold; an optimistic position never releases it.
        if (root.coversCursor(view) && root.contentY === root.restoreY) {
            root.hiddenHeld = false
            root.restoreY = -1
            root.restoreTicks = 0
            settle.restart()
            return
        }
        root.restoreY = root.contentY
        root.deferRestore()
    }
    // The parked check the return asserts through: the cursor row is inside the viewport.
    function coversCursor(view) {
        var top = root.originY + view * Theme.fileRowHeight
        return root.contentY <= top + Theme.fileRowHeight && root.contentY + root.height >= top
    }
    // One more loop turn, with a deadlock guard that ends the hold rather than keeping it.
    function deferRestore() {
        if (!root.visible)
            return
        if (root.restoreTicks >= 60) {
            root.hiddenHeld = false
            root.restoreY = -1
            root.restoreTicks = 0
            settle.restart()
            return
        }
        root.restoreTicks += 1
        Qt.callLater(root.restoreCursorView)
    }

    // The renaming row's view index, the same mapping restoreCursorView uses.
    readonly property int renameViewIndex: root.pane && root.pane.renamingIndex >= 0 ? Filter.viewOf(root.pane.shown, root.pane.renamingIndex) : -1
    // The renaming delegate's error height, read once here rather than bound per row; none while hidden, when Grid or Columns owns the editor.
    readonly property real renameErrorHeight: {
        if (!root.visible || !root.pane || root.pane.renamingIndex < 0 || root.pane.renameError.length === 0) return 0
        var ed = root.pane.renameEditor()
        var f = ed ? ed.editorField : null
        return f ? f.errorHeight : 0
    }
    // Set when an error line appears or grows, and spent by the view's next contentHeight change.
    property bool containRenameError: false
    onRenameErrorHeightChanged: root.containRenameError = root.renameErrorHeight > 0
    // The error line's growth lands in contentHeight after layout, so the row is contained once then, never on a later scroll's estimate.
    onContentHeightChanged: if (root.containRenameError && root.renameViewIndex >= 0) { root.containRenameError = false; root.positionViewAtIndex(root.renameViewIndex, ListView.Contain) }

    // Pane's own open() and its Connections.onRows reach these two through the wrapper functions below.
    Timer {
        id: coalesce
        interval: root.pane.coalesceMs
        repeat: false
        onTriggered: root.requestIfDrifted()
    }

    // The cursor keeps three rows of context above and below while scrolling; every cursor move routes through here.
    function showCursor(view, context) {
        var to = ScrollOff.cursorY(root, view, root.pane.shownTotal, Theme.fileRowHeight, context)
        if (to !== null)
            root.contentY = to
    }

    // Resize and filter changes can change the visible work without moving contentY.
    Connections {
        target: root.pane
        function onVisibleRowsChanged() { if (root.visible) settle.restart() }
        function onRenamingIndexChanged() { root.renameHeld = root.visible && root.pane.renamingIndex >= 0 }
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
        // A hidden-then-shown Size column asks for the visible rows at that moment.
        function onHiddenColsChanged() { if (root.visible) settle.restart() }
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
        // A filtered viewport covers a set and not a run, so the plan walks the rows drawn and no row between them (issue 315).
        var work = Filter.planThumbs(root.pane, view.first, view.last, ViewState.thumbnailMode)
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
        // A hidden Size column or a network or phone folder draws no size, so it asks for no walk.
        if (!DirSizes.wantsSizes("list", ViewState.hiddenCols, root.pane.storageClass, root.pane.storageKnown))
            return
        // Thumbs.viewport() is reused: it takes no thumb-specific state, only geometry.
        var view = root.visibleRange()
        var ask = Filter.planDirSizes(root.pane, view.first, view.last)
        if (ask.length > 0) {
            root.pane.backend.dirsize(ask)
            settle.interval = root.pane.settleMs
        }
        root.dirSizesApplied(ask)
    }

    // Handle an empty held window explicitly before applying held-edge arithmetic.
    function requestIfDrifted() {
        // No window while hidden, nor while a listing is out, whose windows are the directory asked for.
        if (!root.visible || root.pane.total === 0 || root.pane.listInFlight)
            return
        var firstVisible = Math.floor((root.contentY - root.originY) / Theme.fileRowHeight)
        var lastVisible = firstVisible + root.pane.visibleRows
        if (root.pane.renamingIndex >= 0) {
            var range = root.visibleRange()
            firstVisible = range.first
            lastVisible = range.last + 1
        }
        // Under a filter the drawn rows are matches, so the window asked for is the filter's own, in view space (issue 315).
        if (root.pane.shown !== null) {
            FilterWire.drift(root.pane, firstVisible, lastVisible)
            return
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
