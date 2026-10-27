import QtQuick
import qs.Commons
import "." as Flea
import "js/ClipMarks.js" as ClipMarks
import "js/Filter.js" as Filter
import "js/FilterWire.js" as FilterWire
import "js/ExtThumbs.js" as ExtThumbs
import "js/Scroll.js" as Scroll
import "js/ScrollOff.js" as ScrollOff
import "js/Tap.js" as Tap
import "js/Thumbs.js" as Thumbs
import "js/DirSizes.js" as DirSizes
import "js/DragOut.js" as DragOut
import "js/DropDev.js" as DropDev

// One Miller column: a scrolling list of ColumnRows over either a peeked directory or the pane's
// own listing window. It owns no state; the area above it decides which row is which.
Item {
    id: root

    // [{n, d, i}], from a peek or from the pane's own held window.
    property var rows: []
    property bool loadingRows: false
    signal loadingShown(bool shown)
    // The row this column's cursor is on, as an absolute index; -1 when this column has no cursor.
    property int selectedIndex: -1
    // The one row drawn as this column's cursor; a cursor move touches two delegates, not every live one.
    Flea.CursorMark { id: cursorMark; row: root.selectedIndex }
    // The row named here is the one the trail passes through: lifted like a hover, never accented.
    property string liftedName: ""
    // The pane whose listing this column draws, or null for a peek. Only that one column has a
    // selection to paint, and pane.isSelected reads selectionVersion, so the delegate follows it.
    property var pane: null
    // The window pane plus this peek column's directory, device and writability, for a drag from its rows; null on the pane's own column.
    property var peekSource: null
    // A column that is not the active one reads back.
    property bool dim: false
    // The mode of a denied peek, or -1 when this column's directory was read and its rows are true.
    property int lockedMode: -1
    // Whether zero rows here means empty: false while a peek is still out, because a pending peek answers zero rows too, and false for the pane's own listing, whose empty answer is the hero ui/shell.qml lays over the area.
    property bool drawsEmpty: false
    // True when this column draws its size cell, the same flag its rows read as showSize.
    readonly property bool showsSize: root.pane !== null && root.pane !== undefined
    // Parked rows the active column built ahead of the next landing; only the pane's own column has a pane, so only it prebuilds.
    readonly property Flea.PrebuildDriver prebuild: Flea.PrebuildDriver {
        view: root.viewport
        model: stable
        pane: root.pane
        limit: stable.warmRows
        park: function (cell) { cell.live = false; cell.visible = false }
        adopt: function (cell) { cell.live = true; cell.visible = true; cursorMark.place(cell) }
    }
    // One character budget for a row without the chevron, so a column lays its names out once.
    readonly property int nameBudgetPlain: Theme.bodyAdvance > 0 ? Math.max(0, Math.floor((Scroll.contentWidth(root.width, Theme.spacing.rowPaddingX) - Theme.spacing.rowPaddingX - Theme.iconSize - Theme.spacing.gap - Theme.spacing.rowPaddingX - (root.showsSize ? Theme.column.size + 2 * Theme.spacing.gap : 0)) / Theme.bodyAdvance)) : -1
    // The same terms plus the chevron slot, for the chosen directory alone.
    readonly property int nameBudgetChevron: Theme.bodyAdvance > 0 ? Math.max(0, Math.floor((Scroll.contentWidth(root.width, Theme.spacing.rowPaddingX) - Theme.spacing.rowPaddingX - Theme.iconSize - Theme.spacing.gap - Theme.spacing.rowPaddingX - Theme.font.caption - (root.showsSize ? Theme.column.size + 2 * Theme.spacing.gap : 0)) / Theme.bodyAdvance)) : -1
    // True for every shown pane but the rightmost one; the line sits on the pane edge and changes no width.
    property bool showDivider: false
    // Empty short-circuits before the library, the same rule ui/List.qml carries.
    property bool clipEmpty: ClipMarks.isEmpty(root.pane ? root.pane.clipboard : null)
    // The short-circuit above is the only release while empty, so a cut of a large directory frees its lookup here.
    onClipEmptyChanged: { if (root.clipEmpty) ClipMarks.release() }

    // isDir says which of the two things a neighbour column's row is: a directory the pane opens as
    // its own listing, or a file it hands to the opener. See keys.toml's [[pointer]] table.
    signal activated(string name, bool isDir)
    signal picked(int index, int tapCount, int modifiers, bool onName)
    // A row press, so the columns view can stop the slow-click timer before a
    // hold past the interval fires while the button is still down.
    signal rowPressed(int index)
    // The row under a right click. Only the column carrying the pane's own listing answers it, because a peeked column's rows are another directory's and every menu action addresses the pane's cursor.
    signal menuRequested(int index, var eventPoint)
    // A right click that landed on no row, which only the pane's own column can answer for the same
    // reason: the background menu acts on the directory being shown and a peek is not that directory.
    signal backgroundMenuRequested(var eventPoint)
    // A right click on a peek's row: the peek's directory becomes the listing with this row as the cursor, and the menu opens there.
    signal neighbourMenuRequested(string name)
    // A right click on a peek's empty space: the peek's drawn directory becomes the listing, and the background menu opens there once its rows land.
    signal neighbourBackgroundRequested(var eventPoint)
    // A middle click on a directory row, in any of the three columns: ui/js/Tap.js tappedTab opens it in
    // a new tab, and ui/ColumnsArea.qml supplies which directory this column is showing.
    signal tabRequested(var row)
    // The thumbnail plan for this column's viewport, computed here and written by the pane, the grid's own contract.
    signal thumbsApplied(var work)
    signal dirSizesApplied(var ask)

    // The listArea contract ui/ColumnsArea.qml drives the middle column through; the view is private.
    function positionViewAtIndex(index, mode) { view.positionViewAtIndex(index, mode) }
    function itemAtIndex(index) { return view.itemAtIndex(index) }
    function contentY() { return view.contentY }
    // The scrolling list itself, for the anchor that puts the view back after a re-list; see ui/js/AnchorHold.js.
    readonly property alias viewport: view
    // A platform drag in flight, so a slow click never renames off one; ui/ColumnsArea.qml reads it.
    readonly property bool fileDragActive: dragSession.Drag.active
    // Cursor keeps three rows context above and below; wheel path follows viewport margin-free; click context 0 never moves list.
    function showCursor(viewIndex, context) {
        // The column's own rowTotal falls back to its rows with no pane, the same fallback its model uses.
        var to = ScrollOff.cursorY(view, viewIndex, root.rowTotal, Theme.fileRowHeight, context)
        if (to !== null)
            view.contentY = to
    }
    readonly property alias scrollBar: verticalScroll
    function restartSettle() { settle.restart() }
    function restartCoalesce() { coalesce.restart() }
    function primeSettle() { settle.interval = root.pane.firstSettleMs }

    function visibleRange() {
        var visibleRows = Math.max(1, Math.ceil(view.height / Theme.fileRowHeight))
        var fallback = Thumbs.viewport(view.contentY - view.originY, Theme.fileRowHeight, visibleRows, view.count)
        if (root.pane && root.pane.renamingIndex >= 0) {
            var first = view.indexAt(0, view.contentY)
            var last = view.indexAt(0, view.contentY + view.height - 1)
            first = first < 0 ? fallback.first : first
            return {first: first, last: last < 0 ? Math.min(view.count - 1, first + visibleRows) : last}
        }
        return fallback
    }

    // The visible folder rows of a peek column that name no device, which a drag entering the pane asks the backend for.
    function dropAsks() {
        if (!root.visible || !root.peekSource) return []
        var range = root.visibleRange()
        return DropDev.folderAsks(root.rows, root.peekSource.dir, range.first, range.last, root.peekSource.pane)
    }

    // The active column uses List/Grid's integer model and refills only around its viewport.
    function requestIfDrifted() {
        if (root.pane === null || !root.visible || root.pane.listInFlight || root.pane.total === 0)
            return
        var range = root.visibleRange()
        // Under a filter the rows are matches, so the window asked for is the filter's own, in view space (issue 315).
        if (root.pane.shown !== null) {
            FilterWire.drift(root.pane, range.first, range.last)
            return
        }
        var heldEnd = root.pane.held + root.pane.rows.length
        if (root.pane.rows.length === 0
                || (range.first - root.pane.held < root.pane.refetchMargin && root.pane.held > 0)
                || (heldEnd - range.last < root.pane.refetchMargin && heldEnd < root.pane.total))
            root.pane.backend.window(Math.max(0, range.first - root.pane.buffer), root.pane.windowSize)
    }

    // The viewport's rows and no more, rule 1: the same plan the list and the grid run, over this column's own scroll position.
    function requestThumbs() {
        // visible is effective visibility, so the column kept alive under another view plans nothing against the shared state.
        if (root.pane === null || !root.visible || root.pane.total === 0 || root.pane.listInFlight || !root.pane.storageKnown)
            return
        var range = root.visibleRange()
        var work = Filter.planThumbs(root.pane, range.first, range.last, ViewState.thumbnailMode)
        // Only the loaded preview owns an off-viewport request; manual cursor movement asks nothing extra.
        work.drop = work.drop.filter(function (index) { return index !== root.pane.previewIndex })
        root.pane.backend.thumbcancel(work.drop)
        work.cacheOnly = ExtThumbs.cacheOnly(root.pane.storageClass, ViewState.preview)
        root.pane.backend.thumb(work.ask, work.cacheOnly)
        if (work.ask.length > 0) settle.interval = root.pane.settleMs
        root.thumbsApplied(work)
    }

    // The same viewport plan the list runs, over this column's own scroll position. Only the active
    // column can ask: dirsize resolves against st.listing, which a peeked row has no index in.
    function requestDirSizes() {
        if (root.pane === null || !root.visible || root.pane.total === 0 || root.pane.listInFlight)
            return
        // The active column always draws its size, so only the class gate applies here.
        if (!DirSizes.wantsSizes("columns", [], root.pane.storageClass, root.pane.storageKnown))
            return
        var range = root.visibleRange()
        var ask = Filter.planDirSizes(root.pane, range.first, range.last)
        if (ask.length > 0) {
            root.pane.backend.dirsize(ask)
            settle.interval = root.pane.settleMs
        }
        root.dirSizesApplied(ask)
    }

    Connections {
        target: ViewState
        function onThumbnailModeChanged() { if (root.visible) settle.restart() }
    }

    Timer {
        id: coalesce
        interval: root.pane ? root.pane.coalesceMs : 0
        repeat: false
        onTriggered: root.requestIfDrifted()
    }

    Timer {
        id: settle
        interval: root.pane ? root.pane.settleMs : 120
        repeat: false
        onTriggered: { root.requestThumbs(); root.requestDirSizes() }
    }
    onRowsChanged: if (root.pane !== null) settle.restart()
    // A column with no pane is a peek the user is not working in, so it keeps fewer unseen rows than the pane's cacheRows: 4 (ui/Pane.qml).
    readonly property int bareCacheRows: 2
    // No pane yet: the column's own rows are the listing, the same fallback its model uses.
    readonly property int rowTotal: root.pane ? root.pane.shownTotal : root.rows.length
    // The count a hidden column keeps its rows for, -1 once another reached it; the model compares counts, so no handler order builds a row.
    property int keptTotal: -1
    // Where a dropped column stands: the top a new count leaves, then each cursor move contained as showCursor does.
    property real heldY: 0
    Component.onCompleted: root.keptTotal = root.rowTotal
    onRowTotalChanged: { root.keptTotal = root.visible ? root.rowTotal : -1; root.heldY = 0 }
    onVisibleChanged: {
        // A dropped column builds its rows on the frame that shows it, standing where a kept one would.
        if (root.visible && root.keptTotal !== root.rowTotal) {
            root.keptTotal = root.rowTotal
            verticalScroll.settling = true
            view.contentY = view.originY + root.heldY
            verticalScroll.settling = false
        }
        if (root.visible && root.pane !== null) { coalesce.restart(); settle.restart() }
    }
    onHeightChanged: if (root.visible && root.pane !== null) { coalesce.restart(); settle.restart() }

    Flea.FileDrag {
        id: dragSession
        pane: root.pane
        peekSource: root.peekSource
    }

    // Only the active column owns this directory; neighboring peek floors cannot target its listing.
    Flea.DropInto {
        anchors.fill: parent
        enabled: root.pane !== null && !root.pane.trash.opened && root.pane.searchMode === ""
        pane: root.pane
        dest: root.pane ? root.pane.dropPath : ""
        refuseLoading: DragOut.refuseLoading(root.pane && root.pane.listInFlight, false, false)
        destDev: root.pane && root.pane.backend && !root.pane.listInFlight ? root.pane.backend.dirDev : 0
    }

    ListView {
        id: view
        anchors.fill: parent

        model: Flea.StableModel { id: stable; view: view; total: root.visible || root.keptTotal === root.rowTotal ? root.rowTotal : 0; keptRows: true
                                  warm: root.pane !== null && view.visible && root.pane.searchMode.length > 0 && root.pane.swap.searchWarm === true; warmRows: Math.floor((view.height + view.cacheBuffer) / Theme.fileRowHeight) + 1 }
        clip: true
        // The pane's cache rows, as ui/List.qml takes them: Qt's default 320 px builds about 12 unseen rows into every landing.
        cacheBuffer: Theme.fileRowHeight * (root.pane ? root.pane.cacheRows : root.bareCacheRows)
        boundsBehavior: Flickable.StopAtBounds
        onContentYChanged: if (root.pane !== null) { coalesce.start(); settle.restart() }
        onCountChanged: root.prebuild.landed()
        // The error line's growth lands in contentHeight after layout, so the row is contained once then, never on a later scroll's estimate.
        onContentHeightChanged: if (root.containRenameError) { root.containRenameError = false; view.positionViewAtIndex(root.renameViewIndex, ListView.Contain) }
        reuseItems: true

        // G7 needs an empty press target below the final row even when a long column fills the viewport.
        footer: Item {
            width: Scroll.contentWidth(view.width, Theme.spacing.rowPaddingX)
            height: root.pane ? Theme.spacing.rowPaddingY : 0
        }

        Flea.FastScrollHandler {
            parent: view
            flickable: view
        }

        Flea.ViewportScrollBar {
            id: verticalScroll
            parent: view
            anchors { top: parent.top; right: parent.right }
            flickable: view
        }

        Flea.SelectionBand {
            parent: view
            pane: root.pane
            flickable: view
        }

        // Empty space below the last row, the same rule ui/List.qml carries: the pane's own column answers at once while a peek navigates to its drawn directory first and opens its menu once the rows land.
        TapHandler {
            acceptedButtons: Qt.RightButton
            onTapped: function (eventPoint) {
                if (!Tap.onBackground(view, eventPoint))
                    return
                if (root.pane !== null)
                    root.backgroundMenuRequested(eventPoint)
                else
                    root.neighbourBackgroundRequested(eventPoint)
            }
        }

        delegate: Flea.ColumnRow {
            id: cell
            required property int index
            // Parked while pooled (ColumnRow.live): a parked value reads itself back, so parking re-runs nothing that draws.
            readonly property int listingIndex: !cell.live ? -1 : root.pane ? Filter.at(root.pane.shown, index) : index
            // Filed by listing row in the column's cursor mark, which sets cursor; see ui/CursorMark.qml.
            property int markRow: -1
            // Built ahead of a landing past the shown rows: parked hidden until the landing adopts it.
            property bool prebuilt: false
            Component.onCompleted: if (!root.prebuild.built(cell)) cursorMark.place(cell)
            Component.onDestruction: root.prebuild.gone(cell)
            ListView.onReused: root.prebuild.adoptCell(cell)
            onListingIndexChanged: cursorMark.place(cell)
            // Each column keeps the scroll lane clear, the same rule ui/List.qml follows.
            width: Scroll.contentWidth(view.width, Theme.spacing.rowPaddingX)
            paintWidth: view.width
            // A shrunk listing subscripts out of range under a delegate not yet released, and QML
            // hands that back as undefined; every row reader in the tree tests against a real null.
            row: !cell.live ? cell.row : root.pane ? root.pane.rowFor(listingIndex) : root.rows[index] !== undefined ? root.rows[index] : null
            thumb: !cell.live ? cell.thumb : root.pane !== null && Thumbs.allowed(row, ViewState.thumbnailMode) ? root.pane.thumbFor(listingIndex) : ""
            showSize: root.showsSize
            dirSize: !cell.live ? cell.dirSize : root.pane !== null ? DirSizes.sizeFor(root.pane.dirSizeState, listingIndex) : null
            pane: root.pane
            // The clipboard mark is looked up only on the pane's own column, and never while it is empty.
            clipMark: !cell.live ? cell.clipMark : root.clipEmpty ? "" : (root.pane !== null ? ClipMarks.markForRow(root.pane, cell.row ? cell.row.n : "", root.pane.clipboard) : "")
            // The list and the grid both mark a selection member apart from the cursor; so does this.
            selected: cell.live && root.pane !== null && root.pane.isSelected(listingIndex)
            dropTarget: dragSession.dropIndex >= 0 && listingIndex === dragSession.dropIndex
            dropCopying: dragSession.dragCopy
            dropLinking: dragSession.dragLink
            // Read off the normalised row above: subscripting rows again hands a shrunk listing's undefined to a bool.
            lifted: !cell.live ? cell.lifted : root.liftedName.length > 0 && row !== null && row.n === root.liftedName
            dim: cell.live ? root.dim && !lifted : cell.dim
            // The renaming row is always the cursor row; it hides its own name under the editor, as ui/Row.qml does.
            renaming: cell.cursor && root.renaming
            errorGrowth: cell.cursor && root.renameErrorHeight > 0 ? Math.max(root.renameErrorHeight, renameLoader.y + renameLoader.height - root.renameTop - Theme.fileRowHeight) : 0
            // The column's own budget, so no row measures its own text to elide it.
            nameBudget: cell.showChevron ? root.nameBudgetChevron : root.nameBudgetPlain

            TapHandler {
                id: tap
                acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
                // A press held past the interval must not fire while the button
                // is still down; ColumnsArea stops the pane's timer from onRowPressed.
                onPressedChanged: if (pressed) root.rowPressed(cell.listingIndex)
                onTapped: function (eventPoint, button) {
                    if (root.pane !== null) {
                        if (cell.listingIndex < 0 || !cell.row) return
                        if (button === Qt.MiddleButton)
                            root.tabRequested(cell.row)
                        else if (button === Qt.RightButton)
                            root.menuRequested(cell.listingIndex, eventPoint)
                        else
                            root.picked(cell.listingIndex, tap.tapCount, tap.point.modifiers, Tap.onName(cell.nameItem(), cell, eventPoint.position, false))
                        return
                    }
                    if (button === Qt.RightButton && root.rows[index]) {
                        root.neighbourMenuRequested(root.rows[index].n)
                        return
                    }
                    var verb = Tap.tappedColumn(root.rows[index], button, tap.tapCount)
                    if (verb === "openTab")
                        root.tabRequested(root.rows[index])
                    else if (verb.length > 0)
                        root.activated(root.rows[index].n, verb === "reveal")
                }
            }

            Flea.RowDrag {
                session: dragSession
                listingIndex: cell.listingIndex
                row: cell.row
            }
        }
    }

    // A denied peek answers zero rows, the exact count an empty directory answers, so a locked column draws States.dc.html's Locked tile rather than reading as an empty one.
    Flea.StateMessage {
        anchors.fill: parent
        listingState: root.lockedMode >= 0 ? "locked" : "ready"
        lockedMode: root.lockedMode
        total: root.rows.length
    }

    // The column boundary, one per pane and never one per row; an inactive Loader builds no pane so draws none.
    Flea.Divider {
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        visible: root.showDivider
    }

    // corner: one editor for this column, never one inside each row. A delegate binding that follows
    // the pane's renamingIndex costs this column its keys, measured on the box: the comma that opens
    // Settings stopped reaching ui/js/Focus.js, which case_overlays catches. An overlay follows no
    // delegate, so the keys are safe by construction and only the active column ever draws one.
    readonly property int renameViewIndex: root.pane !== null && root.pane.renamingIndex >= 0
                                           ? Filter.viewOf(root.pane.shown, root.pane.renamingIndex) : -1
    readonly property bool renaming: root.renameViewIndex >= 0
    // What ui/Pane.qml's renameEditor() hands ui/Ipc.qml, the shape a list delegate hands it.
    readonly property Item editorField: renameLoader.item as Item
    readonly property string editorText: renameLoader.item ? renameLoader.item.current : ""
    function commitEditor() { return renameLoader.item ? renameLoader.item.commit() : false }

    // The span ColumnRow draws its name in, one gap after the mark to one gap before its size cell, and nothing beyond it.
    readonly property real renameLeft: Theme.spacing.rowPaddingX + Theme.iconSize + Theme.spacing.gap
    // Read when the editor is built, as its name is, because rowFor answers only once the row is held.
    function renamingFolder() {
        var row = root.pane ? root.pane.rowFor(root.pane.renamingIndex) : null
        return !!row && row.d === true
    }
    // The renaming row is the cursor row, so a folder carries its chevron, and the size cell sits left of it as ColumnRow anchors both.
    readonly property real renameRight: Theme.spacing.rowPaddingX + (renameLoader.item !== null && root.renamingFolder() ? Theme.font.caption : 0)
                                        + (root.showsSize ? 2 * Theme.spacing.gap + Theme.column.size : 0)
    readonly property real renameWidth: Math.max(0, Scroll.contentWidth(view.width, Theme.spacing.rowPaddingX) - root.renameLeft - root.renameRight)
    readonly property real renameTop: root.renameViewIndex * Theme.fileRowHeight
    // The editor owns its height, one line box; the column centres it in the row on whole pixels.
    readonly property real renameY: root.renameTop + Math.round((Theme.fileRowHeight - (renameLoader.item ? renameLoader.item.fieldHeight : 0)) / 2)
    readonly property real renameErrorHeight: renameLoader.item ? renameLoader.item.errorHeight : 0
    // Set when an error line appears or changes height, and spent by the view's next contentHeight change.
    property bool containRenameError: false
    onRenameErrorHeightChanged: root.containRenameError = root.renameErrorHeight > 0

    Loader {
        id: renameLoader
        parent: view.contentItem
        // Loaded only while a rename is open: a RenameField built beside every row reports its own
        // hide at creation, and that hide is an abandon.
        active: root.renaming
        // The editor leaves focus in this ListView, which eats Up and Down, so this pane's columns area takes it back in any view.
        onActiveChanged: if (!active && view.focus && root.pane && root.pane.columnsArea) root.pane.columnsArea.focus = true
        x: root.renameLeft
        y: root.renameY
        width: root.renameWidth
        z: 2
        sourceComponent: Flea.RenameField {
            height: implicitHeight
            pane: root.pane
            errorSpan: Math.max(0, root.renameWidth + root.renameRight - Theme.spacing.rowPaddingX)
            name: root.pane && root.pane.rowFor(root.pane.renamingIndex)
                  ? String(root.pane.rowFor(root.pane.renamingIndex).n).split("/").pop() : ""
            onCommitted: function (newName) { root.pane.commitRename(newName) }
            onAbandoned: root.pane.renamingIndex = -1
        }
    }

    // For ui/Ipc.qml's columnChildEmpty readers: the tile's state and its mark's box.
    readonly property Item emptyItem: emptyTile
    Loader {
        anchors.fill: parent
        active: root.loadingRows
        sourceComponent: Flea.LoadingState {
            heldOff: true
            onShownChanged: root.loadingShown(shown)
            Component.onCompleted: if (shown) root.loadingShown(true)
        }
    }
    // The same hero the list draws, per the operator: a peeked empty directory animates like the pane's own.
    Flea.EmptyState {
        id: emptyTile
        anchors.fill: parent
        visible: root.drawsEmpty && root.rows.length === 0 && root.lockedMode < 0
    }

    // Cursor can move off screen through keyboard, so column follows it with context.
    // Selection ensures whole row (context 0); showRow spent margin, second margin would move clicked row.
    onSelectedIndexChanged: {
        root.prebuild.kick()
        if (root.selectedIndex < 0)
            return
        var at = root.pane ? Filter.viewOf(root.pane.shown, root.selectedIndex) : root.selectedIndex
        if (root.visible || root.keptTotal === root.rowTotal)
            root.showCursor(at, 0)
        else
            root.heldY = ScrollOff.containY(at * Theme.fileRowHeight, Theme.fileRowHeight, root.heldY, view.height)
    }
}
