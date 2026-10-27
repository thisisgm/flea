import QtQuick
import "." as Flea
import "js/ClipMarks.js" as ClipMarks
import "js/Density.js" as Density
import "js/DirSizes.js" as DirSizes
import "js/Drag.js" as DragOps
import "js/DragOut.js" as DragOut
import "js/ExtThumbs.js" as ExtThumbs
import "js/Filter.js" as Filter
import "js/FilterWire.js" as FilterWire
import "js/Focus.js" as Focus
import "js/GridGeometry.js" as GridGeometry
import "js/Scroll.js" as Scroll
import "js/Tap.js" as Tap
import "js/Thumbs.js" as Thumbs

// Grid shares the list's rows, marks, thumbnails and viewport plan, scaled to tile rows.
GridView {
    id: root

    property var pane: null
    readonly property bool fileDragActive: dragSession.Drag.active
    property var menu: null

    property real zoomTravel: 0
    readonly property int wheelNotch: 120
    readonly property int touchpadStep: 48

    function zoomWheel(wheel) {
        if (!ViewState.ctrlZoom) return false
        var phase = wheel.phase !== undefined ? wheel.phase : Qt.NoScrollPhase
        root.zoomTravel += Scroll.zoomTravel(phase, wheel.pixelDelta.y, wheel.angleDelta.y, root.touchpadStep, root.wheelNotch)
        var steps = root.zoomTravel > 0 ? Math.floor(root.zoomTravel) : Math.ceil(root.zoomTravel)
        if (steps !== 0) {
            root.zoomTravel -= steps
            var sizes = ["small", "medium", "large", "xlarge", "huge", "largest"]
            var next = Math.max(0, Math.min(sizes.length - 1, sizes.indexOf(ViewState.thumbnailSize) + steps))
            ViewState.changeSetting("preview.thumbSize", sizes[next])
        }
        return true
    }

    signal thumbsApplied(var work)
    signal dirSizesApplied(var ask)
    signal dirSizesCancelled()

    // Tile columns leave one gap, both edge insets and the scroll lane.
    readonly property int columns: GridGeometry.columnsFor(root.width, Theme.grid.minCellWidth,
        ViewState.thumbnailPixels, Theme.spacing.rowPaddingX, Theme.spacing.gap, Theme.spacing.rowPaddingX)
    readonly property int tileRows: Math.max(1, Math.ceil(root.pane.shownTotal / root.columns))
    // Mark, one gap, two caption lines, and the density's pad above and below.
    readonly property int cellHeightPx: GridGeometry.cellHeightFor(ViewState.thumbnailPixels, Theme.spacing.gap,
                                        Math.ceil(Theme.grid.captionHeight),
                                        Density.gridPadY(Theme.spacing.rowPaddingX, ViewState.density))
                                        + renameExtraHeight
    property Item renameEditor: null
    property real renameExtraHeight: 0
    property var renameRetirement: null

    // Grid pooling can leave itemAtIndex pointing at a superseded same-row tile.
    function currentRenameTile(expectedPane) {
        var editor = root.renameEditor
        var host = editor ? editor.editorHost : null
        return editor && editor.visible && root.visible && !root.hiddenHeld && host && host.visible
            && editor.pane === expectedPane && editor.viewport === root && editor.ownsEdit()
            && host === editor.layoutHost() && host.renaming && host.editorField === editor ? host : null
    }

    onPaneChanged: { root.renameRetirement = null; root.queueRenameLayout(); root.takeHeld() }
    // Editor replacement and destruction must not reenter Grid layout through its cell height.
    onRenameEditorChanged: {
        if (root.renameEditor) root.renameRetirement = null
        root.queueRenameLayout()
    }
    function queueRenameLayout() { Qt.callLater(root.settleRenameEditor) }
    function retireRenameEditor(editor, retirement) {
        if (root.renameEditor !== editor) return
        root.renameRetirement = retirement
        root.renameEditor = null
    }
    function settleRenameEditor() {
        var retirement = root.renameRetirement
        var sameEdit = retirement && !root.renameEditor && root.pane && root.pane === retirement.pane
            && root === retirement.viewport && root.pane.renamingIndex === retirement.index
        var row = sameEdit ? root.pane.rowFor(retirement.index) : null
        // A pending write can outlive its Loader. Keep only its values until that exact edit returns.
        var keep = sameEdit && row && row.n.split("/").pop() === retirement.name
            && root.pane.renamePending && root.pane.renameRequest === retirement.request
        if (!keep) root.renameRetirement = null
        if (sameEdit && !root.pane.renamePending)
            root.pane.renamingIndex = -1
        var editor = root.renameEditor
        if (editor && root.pane && root.pane.renamingIndex >= 0 && editor.extraHeight < 0) return
        var height = root.pane && root.pane.renamingIndex >= 0 && editor ? editor.extraHeight : 0
        if (height === root.renameExtraHeight) return
        if (editor) editor.queueContainment()
        root.renameExtraHeight = height
    }
    readonly property int visibleTileRows: Math.max(1, Math.ceil(root.height / root.cellHeightPx))
    onColumnsChanged: if (root.visible) { settle.restart(); coalesce.restart() }
    onVisibleTileRowsChanged: if (root.visible) { settle.restart(); coalesce.restart() }

    focus: true
    // Whichever view is up owns the keyboard, and Focus.handleKey is the one route all three take.
    Keys.onPressed: function (event) {
        root.prebuild.kick()
        root.holdCache()
        event.accepted = Focus.handleKey(event, root.pane, root.pane.sidebar)
    }
    // Written, not bound: a count change puts the view on row 0 as a reset does, which would drop a binding.
    readonly property int cursorView: Filter.viewOf(root.pane.shown, root.pane.renamingIndex >= 0 ? root.pane.renamingIndex : root.pane.cursorIndex)
    onCursorViewChanged: { root.prebuild.kick(); root.askLanding(); root.currentIndex = root.cursorView }
    // Hidden holds no delegates; an edit in flight keeps its own per RenameField.qml.
    model: Flea.StableModel { view: root; total: (root.visible || root.pane.renamingIndex >= 0) ? root.pane.shownTotal : 0; onResized: root.modelResized(); holdCurrent: root.pane.renamingIndex >= 0
                              warm: root.visible && root.pane.searchMode.length > 0 && root.pane.swap.searchWarm === true; warmRows: root.columns * (Math.floor(Math.max(0, root.height - root.topMargin + root.cacheFull) / root.cellHeight) + 2) }
    clip: true
    // Qt's own tracking scrolls a flush-parked end to the footer's end on a cursor move; revealCursor's Contain leaves a whole tile alone.
    highlightFollowsCurrentItem: false
    // Qt emits currentIndexChanged before it frees the old current item; positioning inside it leaves that item behind as a second delegate for its row.
    onCurrentIndexChanged: { root.prebuild.kick(); root.holdCache(); Qt.callLater(root.revealCursor) }
    function revealCursor() {
        // StableModel.resize parks currentIndex on row 0 mid-change, so onResized places the view; a far landing waits for its window, which onRowsChanged lands.
        if (root.visible && !root.hiddenHeld && !root.landingWait && root.count > 0 && root.cursorView >= 0 && root.currentIndex === root.cursorView) {
            root.positionViewAtIndex(root.cursorView, GridView.Contain)
            root.keepLastRowClear(root.cursorView)
        }
    }
    // One gap of bare ground along the left, the top and below the last row; the hairline inset stays.
    leftMargin: Theme.spacing.gap
    topMargin: Theme.spacing.gap
    bottomMargin: Theme.spacing.gap
    cellWidth: GridGeometry.cellWidthFor(root.width, root.columns, Theme.spacing.gap, Theme.spacing.rowPaddingX)
    cellHeight: root.cellHeightPx
    // An emptied view drops its cache rows; they come back in cacheStepsMargin frame steps, each built in its own turn, never by incubation an emptied view would cancel.
    // A switch builds the viewport's tiles before its first frame and the rows past them after it, as every landing does.
    readonly property int cacheFull: root.cellHeightPx * GridGeometry.cacheRows
    property int cacheSteps: 0
    readonly property int cacheStepsMargin: 4
    property int cacheQuiet: 0
    // The frames a key holds growth quiet, so the key's own frame builds nothing else.
    readonly property int cacheQuietFrames: 2
    cacheBuffer: root.cacheSteps > root.cacheStepsMargin ? root.cacheFull : 0
    displayMarginEnd: root.cacheSteps > root.cacheStepsMargin ? 0 : root.cacheFull * root.cacheSteps / root.cacheStepsMargin
    function regrowCache() {
        if (root.model.count === 0)
            root.cacheSteps = 0
        cacheGrow.stop()
        if (root.model.count > 0 && root.cacheSteps <= root.cacheStepsMargin)
            cacheGrow.start()
    }
    function holdCache() {
        if (cacheGrow.running) root.cacheQuiet = root.cacheQuietFrames
    }
    FrameAnimation {
        id: cacheGrow
        running: false
        onTriggered: {
            if (root.cacheQuiet > 0) root.cacheQuiet -= 1
            if (root.cacheQuiet > 0 || root.pane.listInFlight || root.moving) return
            root.cacheSteps += 1
            root.forceLayout()
            if (root.cacheSteps > root.cacheStepsMargin) cacheGrow.stop()
        }
    }
    boundsBehavior: Flickable.StopAtBounds
    reuseItems: true
    // Parked tiles for the next landing: a tile parks unparented and the landing's layout parents it again.
    readonly property Flea.PrebuildDriver prebuild: Flea.PrebuildDriver {
        view: root
        model: root.model
        pane: root.pane
        limit: root.landingTiles
        held: root.hiddenHeld
        rowsPerTick: 4
        park: function (cell) { cell.pooled = true; cell.parent = null }
        adopt: function (cell) { cell.pooled = false; cursorMark.place(cell) }
    }
    // The tiles a full landing from the top lays out; the partial row warmRows bounds stays out (Qt 6.11.2 qquickgridview.cpp:507).
    readonly property int landingTiles: {
        var c = root.columns, h = root.cellHeight, end = root.height - root.topMargin + root.cacheFull, n = 0
        while (c > 0 && h > 0 && n < root.model.warmRows && Math.floor(n / c) * h <= end + h * (c - n % c) / (c + 1)) n++
        return n
    }
    // The one tile drawn as the cursor; a cursor move touches two delegates, not every live one.
    Flea.CursorMark { id: cursorMark; row: root.pane.cursorIndex }

    Flea.FastScrollHandler {
        parent: root
        flickable: root
        ctrlWheelAction: function (wheel) { return root.zoomWheel(wheel) }
        onWheeled: root.endLanding(false)
    }
    // movementStarted is the view's own drag or flick, never a positionViewAtIndex or contentY write, so the landing's moves do not end its wait.
    onMovementStarted: root.endLanding(false)

    readonly property alias scrollBar: verticalScroll
    Flea.ViewportScrollBar {
        id: verticalScroll
        parent: root
        anchors { top: parent.top; right: parent.right }
        flickable: root
        ctrlWheelAction: function (wheel) { return root.zoomWheel(wheel) }
        onUserScrolled: root.endLanding(false)
    }

    Flea.SelectionBand {
        parent: root
        pane: root.pane
        flickable: root
        columns: root.columns
        cellWidth: root.cellWidth
        cellHeight: root.cellHeight
        rowAt: root.tileAt
    }

    Flea.FileDrag {
        id: dragSession
        pane: root.pane
    }

    delegate: Flea.GridTile {
        id: cell
        required property int index

        // Acts on the backend row it displays; while pooled (GridTile.live) each listing read keeps its last value until onReused.
        readonly property int listingIndex: cell.live ? Filter.at(root.pane.shown, index) : -1
        // Filed by listing row in the view's cursor mark, which sets cursor; see ui/CursorMark.qml.
        property int markRow: -1
        // Built ahead of a landing past the shown rows: parked and unparented until the landing adopts it.
        property bool prebuilt: false
        // Where hover, a press, a tap or a drag reaches this tile: its icon box and painted caption.
        readonly property QtObject pressMask: QtObject { function contains(point: point): bool { return root.hitsTile(cell, point) } }
        readonly property Item rowDrag: rowDragItem
        Component.onCompleted: if (!root.prebuild.built(cell)) cursorMark.place(cell)
        Component.onDestruction: root.prebuild.gone(cell)
        onParentChanged: if (cell.prebuilt && cell.parent === root.contentItem) root.prebuild.adoptCell(cell)
        onListingIndexChanged: cursorMark.place(cell)
        width: root.cellWidth
        height: root.cellHeight
        row: cell.live ? GridGeometry.heldRow(cell.rowMemo, root.heldRows, listingIndex) : cell.row
        // One shared hover index for every tile, so no tile builds its own handler.
        hovered: cell.live && root.hoveredIndex === cell.index
        hoverShown: root.pane.selectionBand === null
        selected: cell.live && root.pane.isSelected(listingIndex)
        dropTarget: dragSession.dropIndex >= 0 && listingIndex === dragSession.dropIndex
        dropCopying: dragSession.dragCopy
        dropLinking: dragSession.dragLink
        thumb: !cell.live ? cell.thumb : Thumbs.allowed(row, ViewState.thumbnailMode) ? Thumbs.fileFor(root.pane.thumbState, listingIndex) : ""
        // List's own query and arming (ui/List.qml), so a tile marks what a row marks and builds its mark with the strip.
        markQuery: root.pane.searchMode.length > 0 ? root.pane.swap.answeredQuery : root.pane.filterQuery
        markArmed: root.pane.filterTyping || root.pane.filterQuery.length > 0 || root.pane.searchMode.length > 0
        renaming: listingIndex >= 0 && listingIndex === root.pane.renamingIndex
        renamePane: root.pane
        // The clipboard mark is looked up only for tiles a delegate holds.
        clipMark: cell.live ? ClipMarks.markForRow(root.pane, cell.row ? cell.row.n : "", root.pane.clipboard) : cell.clipMark
        onRenameCommitted: function(newName) { root.pane.commitRename(newName) }
        onRenameAbandoned: root.pane.renamingIndex = -1

        Item {
            anchors.fill: parent
            containmentMask: cell.pressMask
            TapHandler {
                id: tap
                enabled: !cell.renaming
                acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
                // A press held past the interval must not fire while the button is
                // still down on a row about to be dragged; the release still arms.
                onPressedChanged: if (pressed) root.pane.pressSlowClick()
                onTapped: function (eventPoint, button) {
                    if (cell.listingIndex < 0) return
                    if (button === Qt.MiddleButton)
                        Tap.tappedTab(root.pane.rowFor(cell.listingIndex), root.pane.path, root.pane)
                    else if (button === Qt.RightButton)
                        Tap.tappedMenu(cell.listingIndex, eventPoint, root.pane, root.menu)
                    else {
                        var wasSole = root.pane.slowClickWasSole(cell.listingIndex)
                        Tap.tapped(cell.listingIndex, tap.tapCount, tap.point.modifiers, root.pane)
                        // The slow click renames on the pane's timer; a double click opens through tapped() above instead.
                        if (tap.tapCount === 2) root.pane.cancelSlowClick()
                        else if (tap.tapCount === 1 && Tap.onName(cell.captionItem, cell, eventPoint.position, true)) root.pane.armSlowClick(cell.listingIndex, tap.point.modifiers, dragSession.Drag.active, wasSole)
                        else root.pane.cancelSlowClick()
                    }
                }
            }
        }

        Flea.RowDrag {
            id: rowDragItem
            containmentMask: cell.pressMask
            session: dragSession
            listingIndex: cell.listingIndex
            row: cell.row
            // Drops answer from the one shared area below, so no tile builds its own.
            dropsLocal: false
        }
    }

    // One hover index for every tile: a per-tile HoverHandler costs a delegate each in the landing turn.
    property int hoveredIndex: -1
    // The tile under a view point, or -1 on a cell's bare margin or past the tiles: only the icon box and the caption hover.
    function hoverAt(x, y) {
        root.hoveredIndex = root.tileAt(x + root.contentX, y + root.contentY)
    }
    // A Flickable puts its handlers in the content item, so the probe's point is read from the scene, which a scroll leaves in place.
    function hoverScene(scene) {
        var p = root.mapFromItem(null, scene.x, scene.y)
        root.hoverAt(p.x, p.y)
    }
    HoverHandler {
        id: hoverProbe
        enabled: root.visible && !root.hiddenHeld
        onHoveredChanged: { if (!hovered) root.hoveredIndex = -1; else root.hoverScene(point.scenePosition) }
        onPointChanged: if (hovered) root.hoverScene(point.scenePosition)
    }
    // One drop area for every folder tile: a per-tile DropArea costs a delegate each in the landing turn.
    property var dropTile: null
    property int dropListingIndex: -1
    // What the tile under the drag answered; one refusal on entry must not end the area's events, so the area itself stays accepted and each move re-says this.
    property bool dropAccepted: false
    // A folder tile's drop row under a view point, or null on a file, past its icon and caption, or on bare ground; the index lookup never meets another item of the content, and a pooled or stale tile never answers.
    function tileDropAt(x, y) {
        var p = root.mapToItem(root.contentItem, x, y)
        var tile = root.itemAtIndex(root.indexAt(p.x, p.y))
        if (!tile || tile.visible === false || !tile.live || tile.listingIndex === undefined || tile.listingIndex < 0
                || !tile.row || tile.row.d !== true || !root.hitsTile(tile, tile.mapFromItem(root, x, y)))
            return null
        return tile.rowDrag
    }
    // The hovered tile's own exit, or its record on the session when a re-list destroyed it already.
    function dropLeft() {
        var tile = root.dropTile
        root.dropTile = null
        root.dropListingIndex = -1
        if (tile) {
            tile.leaveAt()
            return
        }
        dragSession.dropIndex = -1
        dragSession.dropDest = ""
        dragSession.dropDestDev = 0
        dragSession.leaveTarget()
        if (dragSession.dragRows.length === 0) {
            dragSession.dragCopy = false
            dragSession.dragLink = false
        }
        dragSession.hoverEnded()
        DragOut.keys.leave()
    }
    function dropEnter(next, drag) {
        root.dropTile = next
        root.dropListingIndex = next ? next.listingIndex : -1
        if (next) next.enterAt(drag, next.mapFromItem(root, drag.x, drag.y))
        else drag.accepted = false
        root.dropAccepted = drag.accepted
    }
    DropArea {
        parent: root
        anchors.fill: parent
        // Bare ground and file tiles are not this area's, so the pane's floor (ui/PaneWire.qml) takes them as it does under List.
        containmentMask: QtObject { function contains(point: point): bool { return root.tileDropAt(point.x, point.y) !== null } }
        enabled: root.pane !== null && !root.pane.listInFlight
        keys: [DragOps.ROWS_MIME, "text/uri-list", "text/plain"]
        onEntered: function (drag) {
            root.dropEnter(root.tileDropAt(drag.x, drag.y), drag)
            drag.accepted = true
        }
        onPositionChanged: function (drag) {
            var next = root.tileDropAt(drag.x, drag.y)
            if (next !== root.dropTile || (next && next.listingIndex !== root.dropListingIndex)) {
                if (root.dropTile || root.dropListingIndex >= 0) root.dropLeft()
                root.dropEnter(next, drag)
            } else {
                if (next) next.moveAt(next.mapFromItem(root, drag.x, drag.y))
                drag.accepted = root.dropAccepted
            }
        }
        onExited: root.dropLeft()
        onDropped: function (drop) {
            var target = root.tileDropAt(drop.x, drop.y)
            if (target !== root.dropTile) root.dropLeft()
            root.dropTile = null
            root.dropListingIndex = -1
            if (target) target.dropAt(drop)
        }
    }

    // One tile row of bare ground at the end, the same inset ui/List.qml keeps and for the same two
    // reasons: a band has to start somewhere and the background menu has to be raisable at the end.
    footer: Item {
        width: Math.max(0, root.width - Theme.spacing.rowPaddingX)
        height: Theme.chromeHeight
    }

    // The space past the last tile is the directory's own, the same rule ui/List.qml carries: a
    // tile's right click belongs to its delegate, and tileAt is what tells the two apart.
    TapHandler {
        acceptedButtons: Qt.RightButton
        onTapped: function (eventPoint) {
            if (root.tileAt(eventPoint.position.x, eventPoint.position.y) < 0)
                root.menu.openBackground(eventPoint.scenePosition)
        }
    }

    // A tile is its icon box and its painted caption, or the whole editor while it renames; the rest of a cell is bare ground.
    function hitsTile(tile, point) {
        var icon = tile.thumbItem.mapFromItem(tile, point.x, point.y)
        return tile.editing || (icon.x >= 0 && icon.x < tile.thumbItem.width && icon.y >= 0 && icon.y < tile.thumbItem.height)
            || Tap.onName(tile.captionItem, tile, point, true)
    }
    // The tile index under a content point, -1 on a cell's bare margin or past the last tile.
    function tileAt(x, y) {
        var index = root.indexAt(x, y), tile = root.itemAtIndex(index)
        return tile && root.hitsTile(tile, Qt.point(x - tile.x, y - tile.y)) ? index : -1
    }

    onContentYChanged: {
        // Hidden geometry and a restore in flight move no shared state.
        if (!root.visible || root.hiddenHeld)
            return
        // A scroll under a resting pointer moves the hovered tile without a point change.
        if (hoverProbe.hovered) root.hoverScene(hoverProbe.point.scenePosition)
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

    Component.onCompleted: root.takeHeld()
    // The held rows as the tiles read them, taken once per rows write: held and rows land as two writes, and a tile bound to both would read the old rows against the new start.
    property var heldRows: ({ start: 0, rows: [], listing: 0 })
    // more is the filter window's row for a matched tile the held window does not hold (issue 315): a tile's row binding reads that window through it, so the window landing re-reads the tile.
    function takeHeld() {
        if (!root.pane) return
        root.heldRows = { start: root.pane.held, rows: root.pane.rows, listing: root.pane.backend ? root.pane.backend.heldListing : 0,
            more: function (index) { return root.pane.shown === null ? null : FilterWire.rowAt(root.pane, index) } }
    }
    // The held window in tile rows, where Pane's windowSize and buffer count list rows: the list's context in pixels, rebuilt in tiles.
    readonly property var windowPlan: GridGeometry.windowFor(root.columns, root.visibleTileRows, root.pane.buffer, Theme.fileRowHeight, root.cellHeightPx, root.pane.refetchMargin)
    // A cursor landing past the held rows asks for its window now, ahead of the move Pane.showRow makes right after, so the first layout binds real rows.
    function askLanding() {
        if (root.cursorView < 0 || root.hiddenHeld) return
        var need = root.landingNeed()
        // Set before the ask: a reply that beats this call lands the view in onRowsChanged.
        root.landingWait = need !== null && root.owes()
        root.landingStart = need === null ? -1 : need.start
        root.askWindow(need)
    }
    // The window a far cursor landing owes, null for a step, a landing the held rows reach, or a view that may not ask.
    function landingNeed() {
        return GridGeometry.landingAsk(root.windowPlan, root.cursorView, root.visibleRange().first, root.pane.total, root.pane.held, root.pane.rows.length)
    }
    // Owed while the cursor's own row is not held; a landing inside the held rows has rows to bind and moves at once.
    function owes() { return root.canAsk() && root.pane.rowFor(root.pane.cursorIndex) === null }
    function windowOwed() { return root.owes() && root.landingNeed() !== null }
    // The view move of a cursor landing, which Pane.showRow makes right after the ask: a far one waits for its rows, as one pass binds them where two would clear then bind.
    function showCursor(view, context) {
        if (!root.landingWait) root.positionViewAtIndex(view, GridView.Contain)
    }
    property bool landingWait: false
    // Where the landing ask's window starts, so its own reply is told from another ask's.
    property int landingStart: -1
    // The wait ends on the ask's outcome, rows covering the cursor or not, or its refusal; a wheel, drag, flick or bar press ends it for an ask never answered.
    function landIfReady() {
        if (!root.landingWait || (root.windowOwed() && root.pane.held !== root.landingStart)) return
        root.landingWait = false
        root.revealCursor()
    }
    // A refused or unanswered window leaves the view where it is and frees the drift asks; the operator owns the view from here.
    function endLanding(reveal) {
        if (!root.landingWait) return
        root.landingWait = false
        if (reveal) root.revealCursor()
        else coalesce.restart()
    }
    // No window while hidden, while a filter narrows rows already held, nor while a listing is out, whose windows are the directory asked for.
    function canAsk() { return root.visible && root.pane.total !== 0 && root.pane.shown === null && !root.pane.listInFlight }
    function askWindow(ask) {
        if (ask !== null && root.canAsk())
            root.pane.backend.window(ask.start, ask.count)
    }

    function requestIfDrifted() {
        // The view is parked on the old rows until the landing window arrives; a drift ask from there would fetch the window it left.
        if (root.landingWait) return
        var range = root.visibleRange()
        // Under a filter the tiles are matches, so the window asked for is the filter's own, in view space (issue 315).
        if (root.pane.shown !== null) {
            if (root.visible && !root.pane.listInFlight) FilterWire.drift(root.pane, range.first, range.last)
            return
        }
        root.askWindow(GridGeometry.driftAsk(root.windowPlan, range.first, range.last, root.pane.held, root.pane.rows.length, root.pane.total))
    }

    // Thumbs.viewport takes geometry and no thumb-specific state, so a tile row is handed to it the
    // same way a text row is; the answer is then multiplied out into item indices.
    function visibleRange() {
        var view = Thumbs.viewport(root.viewTop(), root.cellHeightPx, root.visibleTileRows, root.tileRows)
        return {
            first: view.first * root.columns,
            last: Math.min(root.pane.shownTotal - 1, (view.last + 1) * root.columns - 1)
        }
    }

    // Returning parks the view on the shared cursor once the reset rows land.
    property bool hiddenHeld: false
    // Last parked contentY and loop turns spent; only a value stable across turns ends the hold.
    property real restoreY: -1
    property int restoreTicks: 0
    onVisibleChanged: {
        if (!root.visible) {
            coalesce.stop()
            settle.stop()
            root.landingWait = false
            root.hiddenHeld = true
            root.restoreY = -1
            root.restoreTicks = 0
            return
        }
        if (root.hiddenHeld)
            root.restoreCursorView()
    }
    // Count changes inside StableModel's layout, where a move refills every tile between the top and the cursor; resized follows it.
    function modelResized() { root.landingWait = false; root.placeCursor(); root.prebuild.landed(); root.regrowCache() }
    function placeCursor() {
        root.currentIndex = root.cursorView
        if (root.hiddenHeld && root.visible && root.count > 0)
            root.restoreCursorView()
        else root.revealCursor()
    }
    // Contain parks a last-row cursor flush on the view bottom, so keep the tiles one gap clear; a row already clear stays put.
    function keepLastRowClear(view) {
        if (root.contentHeight + root.topMargin + root.bottomMargin <= root.height || root.columns < 1 || view < 0) return
        if (Math.floor(view / root.columns) !== root.tileRows - 1) return
        root.contentY = Math.max(root.contentY, root.originY + root.contentHeight - Theme.chromeHeight + Theme.spacing.gap - root.height)
    }
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
        root.positionViewAtIndex(view, GridView.Contain)
        root.keepLastRowClear(view)
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
    // Restore gives up after this many turns, the bound the hidden return waits on.
    readonly property int restoreMaxTurns: 60
    // The parked check the return asserts through: the cursor tile overlaps the viewport.
    function coversCursor(view) {
        var row = Math.floor(view / Math.max(1, root.columns))
        var top = row * root.cellHeightPx
        return top < root.viewTop() + root.height && top + root.cellHeightPx > root.viewTop()
    }
    // The viewport's top in tile space: a column change re-flows the tiles and moves originY, so contentY alone reads the wrong row.
    function viewTop() { return root.contentY - root.originY }
    // One more loop turn, with a deadlock guard that ends the hold rather than keeping it.
    function deferRestore() {
        if (!root.visible)
            return
        if (root.restoreTicks >= root.restoreMaxTurns) {
            root.hiddenHeld = false
            root.restoreY = -1
            root.restoreTicks = 0
            settle.restart()
            return
        }
        root.restoreTicks += 1
        Qt.callLater(root.restoreCursorView)
    }

    Connections {
        target: root.pane.backend
        ignoreUnknownSignals: true
        function onFailed(where, input, message, mode) { if (where === "window") root.endLanding(true) }
    }

    Connections {
        target: root.pane
        function onRowsChanged() { root.takeHeld(); root.landIfReady() }
        function onRenamingIndexChanged() { root.renameRetirement = null; root.queueRenameLayout() }
        function onRenameRequestChanged() { root.queueRenameLayout() }
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
        if (!root.visible || root.pane.listInFlight || !root.pane.storageKnown)
            return
        var range = root.visibleRange()
        var work = Filter.planThumbs(root.pane, range.first, range.last, ViewState.thumbnailMode)
        work.drop = work.drop.filter(function (index) { return index !== root.pane.previewIndex })
        root.pane.backend.thumbcancel(work.drop)
        work.cacheOnly = ExtThumbs.cacheOnly(root.pane.storageClass, ViewState.preview)
        root.pane.backend.thumb(work.ask, work.cacheOnly)
        root.thumbsApplied(work)
    }

    function requestDirSizes() {
        if (!root.visible || root.pane.shownTotal === 0 || root.pane.listInFlight)
            return
        // A tile draws no size, so the grid never asks, whatever rows are visible.
        if (!DirSizes.wantsSizes("grid", ViewState.hiddenCols, root.pane.storageClass, root.pane.storageKnown))
            return
        var range = root.visibleRange()
        var ask = Filter.planDirSizes(root.pane, range.first, range.last)
        if (ask.length > 0)
            root.pane.backend.dirsize(ask)
        root.dirSizesApplied(ask)
    }
}
