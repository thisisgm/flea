import QtQuick
import qs.Commons
import "." as Flea
import "js/Columns.js" as Columns
import "js/ColumnMenu.js" as ColumnMenu
import "js/ExtThumbs.js" as ExtThumbs
import "js/Focus.js" as Focus
import "js/Nav.js" as Nav
import "js/Swap.js" as Swap
import "js/Tap.js" as Tap

// Miller columns peek their neighbours; the active listing shares List's cursor, selection and row operations.
Item {
    id: root

    property var pane: null
    property var menu: null
    // The active column's thumbnail plan, relayed for ui/Pane.qml to write, the grid's own contract.
    signal thumbsApplied(var work)
    signal dirSizesApplied(var ask)
    // Whichever view is up owns the keyboard, and Focus.handleKey is the one route all three take.
    Keys.onPressed: function (event) { event.accepted = Focus.handleKey(event, root.pane, root.pane.sidebar) }

    // peekKey -> answered rows (peeked) and outstanding asks (pending), cleared whenever the pane moves.
    property var peeked: ({})
    property var pending: ({})
    // peekKey -> the mode of a peek that came back denied, which answers zero rows as an empty one does.
    property var denials: ({})
    property int peekVersion: 0
    // The column directories drawn at the last refresh, which every watching peek names so the backend unwatches the rest.
    readonly property var keep: ({ drawn: [] })

    readonly property string parentPath: Nav.parentOf(root.pane.path)
    // Extra columns are ancestors, oldest first, each with the parent column's own peek.
    readonly property string grandparentPath: Nav.parentOf(root.parentPath)
    readonly property string greatGrandparentPath: Nav.parentOf(root.grandparentPath)
    // The meta the preview column names: pixels, line count, symlink target, for the cursor row only.
    property var cursorMeta: null
    readonly property var cursorRow: root.pane.rowFor(root.pane.cursorIndex)
    readonly property bool cursorIsDir: root.cursorRow !== null && root.cursorRow.d === true
    readonly property string childPath: root.cursorIsDir
        ? root.pane.join(root.pane.path, root.cursorRow.n) : ""

    // What the third column shows: the cursor row as of the last swap, so a picture is taken before it changes.
    property bool shownHasRow: false
    property bool shownIsDir: false
    property string shownChildPath: ""
    // The swap's answer for the third column: the folder's rows in, or the file's preview whole.
    readonly property bool thirdReady: root.shownIsDir ? root.answered(root.shownChildPath)
        : (!root.shownHasRow || !ViewState.previewColumn || preview.ready)
    // True while an unanswered folder waits under the live picture: the file work's readiness and cap stay out of the swap.
    readonly property bool folderWaiting: Columns.folderDataHold(root.cursorIsDir, root.answered(root.childPath))

    // The raw stored value, so cappedLimit reads a missing key or a hand-edited false or "" as the shipped default instead of the 0 an int property coerces.
    readonly property var columnsLimit: ViewState.state.columnsLimit
    readonly property int columnCount: Columns.columnCountForWidth(root.width, root.columnsLimit)
    // One columnWidth per shown column from the window width (ColumnsWidth board #167 and #69); the third column takes the remainder at activeX, and resizing re-lays widths in one frame with no re-read.
    readonly property int columnWidth: Math.max(1, Math.floor(root.width / Math.max(1, root.columnCount)))
    // Ancestors shown oldest first: 2 hides the parent, 5 adds the great-grandparent.
    readonly property bool showGreatGrandparent: root.columnCount >= 5
    readonly property bool showGrandparent: root.columnCount >= 4
    readonly property bool showParent: root.columnCount >= 3
    // The climb stops at /, so a shown slot with no distinct ancestor stays blank and keeps its width.
    readonly property bool parentShown: Columns.ancestorShown(root.pane.path, 1)
    readonly property bool grandparentShown: Columns.ancestorShown(root.pane.path, 2)
    readonly property bool greatGrandparentShown: Columns.ancestorShown(root.pane.path, 3)
    readonly property int shownBefore: (root.showGreatGrandparent ? 1 : 0) + (root.showGrandparent ? 1 : 0) + (root.showParent ? 1 : 0) + 1
    readonly property int activeX: (root.shownBefore - 1) * root.columnWidth
    // The preview column counts as shown when the child folder or the file preview draws, else the active pane is rightmost.
    readonly property bool thirdShown: root.shownIsDir || (root.shownHasRow && !root.shownIsDir && ViewState.previewColumn)

    // The key a column's rows are kept under: the path and the order that sorted them.
    function peekKey(path) {
        return Columns.peekKey(path, root.pane.showHidden, ViewState.state.hiddenLast === true)
    }

    // A read of peeked that a binding re-evaluates when a peek lands; peekVersion is the trigger.
    function rowsFor(path) {
        var key = root.peekKey(path)
        return root.peekVersion >= 0 && path.length > 0 && root.peeked[key] ? root.peeked[key] : []
    }

    // -1 for a path that was not denied, so mode 0, the denial whose stat failed too, stays its own answer.
    function deniedMode(path) {
        var key = root.peekKey(path)
        return root.peekVersion >= 0 && root.denials[key] !== undefined ? root.denials[key] : -1
    }

    // A peek still out answers zero rows the way an empty directory does, so a column holds its empty tile back until the reply has actually landed for that path.
    function answered(path) {
        var key = root.peekKey(path)
        return root.peekVersion >= 0 && path.length > 0 && root.peeked[key] !== undefined
    }

    // again re-asks a column already held, the rows staying until the reply replaces them; rearm asks even with the first ask out, because the backend unwatched the column when it left the drawn set.
    function ask(path, again, rearm) {
        var key = root.peekKey(path), sent = Columns.sentKey(key, root.pane.windowSize)
        if (path.length > 0 && (again === true || !root.peeked[key]) && (rearm === true || !Columns.hasAsk(root.pending, sent))) {
            root.pending = Columns.trackAsk(root.pending, sent)
            root.pane.backend.peek(path, root.pane.windowSize, root.pane.showHidden, undefined, root.keep.drawn)
        }
    }

    // Hidden view asks nothing; the gates are computed fresh, so a handler mid-notify cannot read a stale sibling binding.
    function refreshNeighbours() { if (!root.visible) return
        Columns.keepAsks(root.keep, Columns.neighbourAsks(root.pane.path, root.width, root.columnsLimit), root.childPath).forEach(function (one) { root.ask(one.path, one.again, one.again) })
        root.askMeta()
        root.askThumb()
    }

    // One row, only when the preview column is actually the surface showing: the same no-sweep rule
    // thumb and dirsize already follow.
    function askMeta() { preview.followSelection() }

    // A stale queued show never draws an unanswered folder; the cap's own pending show passes force.
    function showCursorRow(force) {
        if (force !== true && Columns.folderDataHold(root.cursorIsDir, root.answered(root.childPath))) return
        root.shownHasRow = root.cursorRow !== null
        root.shownIsDir = root.cursorIsDir
        root.shownChildPath = root.childPath
    }

    // An unanswered folder shows its pending state at the cap; a live picture ends before that wait starts.
    Timer {
        id: folderFallback
        interval: Swap.HOLD_MS
        repeat: false
        onTriggered: if (root.cursorIsDir && !root.answered(root.childPath)) { thirdSwap.cancel(); root.showCursorRow(true) }
    }

    // Only a file load takes a hold and an unanswered folder waits by data; a manual load, a hidden preview column or a held frame lands at once, or the cap would never arm and the frozen picture would block the pointer.
    function moveThird() {
        if (root.cursorIsDir === root.shownIsDir && root.childPath === root.shownChildPath
                && (root.cursorRow !== null) === root.shownHasRow)
            return
        var idle = !thirdSwap.capturing && !thirdSwap.holding
        // Unknown is never held: the class has not named its verdict yet, so a file waits
        // for it the way a load does and onStorageKnownChanged decides when it lands.
        var held = ExtThumbs.manualHold(root.pane.storageClass, ViewState.preview)
        // A null row is never a file load: the rows have not landed yet, so a hold here would show the folder with no cap.
        var fileLoad = Columns.isFileRow(root.cursorRow) && ViewState.previewColumn && ViewState.previewAutomatic && !held
        // An unanswered folder waits by data under the live picture, which the landing or the cap releases with the folder in one pass.
        if (Columns.folderDataHold(root.cursorIsDir, root.answered(root.childPath))) {
            thirdSwap.stopCap()
            folderFallback.restart()
            return
        }
        folderFallback.stop()
        if (!fileLoad) {
            // A hold already live owns the picture, so the no-load change joins it under
            // the cap; landing it at once would freeze the column and block the pointer.
            if (!idle) {
                thirdSwap.hold(root.showCursorRow, root.swapKey())
                thirdSwap.start(false)
                if (!root.cursorIsDir) preview.followSelection()
            } else {
                root.showCursorRow()
                if (!root.cursorIsDir) preview.followSelection()
            }
            return
        }
        thirdSwap.hold(root.showCursorRow, root.swapKey())
        // A file's work starts with its settle, which runs from the key.
        preview.armSettle()
    }

    function swapKey() { return root.pane.path + "\n" + root.pane.cursorIndex }

    // The third column first, so a hold is in place before the preview clears under it.
    function followCursor() {
        root.moveThird()
        root.askMeta()
    }

    function swapState() { return thirdSwap.describe() }

    // The listArea contract every caller of the pane's own navigation uses: the listing's column plans its own viewport's thumbnails, the way the list and the grid do.
    function primeSettle() { active.primeSettle() }
    function restartCoalesce() { active.restartCoalesce() }
    function restartSettle() { active.restartSettle() }
    function positionViewAtIndex(index, mode) { active.positionViewAtIndex(index, mode) }
    // The one column whose rows are the pane's own, for ui/Ipc.qml: a neighbour column's background navigates to its drawn directory first, so no peek lands a background right click at once.
    function activeColumn() { return active }
    readonly property var scrollBar: active.scrollBar
    readonly property bool fileDragActive: active.fileDragActive || parentColumn.fileDragActive || childColumn.fileDragActive
        || !!(grandparentLoader.item && grandparentLoader.item.fileDragActive)
        || !!(greatGrandparentLoader.item && greatGrandparentLoader.item.fileDragActive)
    // All active views accept a view position; the pane maps filtered listing indices before calling.
    function itemAtIndex(index) { return active.itemAtIndex(index) }
    function activeContentY() { return active.contentY() }

    // A neighbour column's row, which the pane has no cursor on: a directory becomes the pane's own
    // listing, which is this view's reveal, and a file goes to the opener.
    // The peek's directory becomes the listing; Nav.applyPendingSelect puts the cursor on the row and opens the menu once the rows land.
    function menuOnNeighbour(base, name) {
        root.pane.pendingSelect = root.pane.join(base, name)
        root.pane.pendingMenu = true
        root.pane.open(base)
    }

    // A neighbour column's empty space delegates to the tested routing: busy refuses before any intent, the shown folder opens where it stands, and any other drawn target navigates with its menu waiting on the rows.
    function menuOnNeighbourBackground(base, eventPoint) {
        ColumnMenu.routeBackground(root.pane, base, eventPoint ? eventPoint.scenePosition : null, root.menu)
    }

    // For ui/Ipc.qml: the peek columns' rows and the child's empty tile; the two eldest answer null while their Loader is unbuilt.
    function parentItemAt(index) { return (root.showParent && root.parentShown) ? parentColumn.itemAtIndex(index) : null }
    function grandparentItemAt(index) { var col = grandparentLoader.item; return (root.showGrandparent && root.grandparentShown && col) ? col.itemAtIndex(index) : null }
    function greatGrandparentItemAt(index) { var col = greatGrandparentLoader.item; return (root.showGreatGrandparent && root.greatGrandparentShown && col) ? col.itemAtIndex(index) : null }
    function childItemAt(index) { return childColumn.itemAtIndex(index) }
    function childEmptyItem() { return childColumn.emptyItem }
    function frameItem() { return preview.frameItem }
    function pictureItem() { return preview.pictureItem }
    function playerLoaded() { return preview.playerLoaded() }
    readonly property int previewIndex: preview.visible ? preview.loadedIndex : -1
    function thumbShown() { return preview.thumbShown }
    function frameReady() { return preview.frameStatus === Image.Ready }
    function textLines() { return preview.textLines() }
    function markdownText() { return preview.markdownText() }
    function linesItem() { return preview.linesItem }
    function archiveItem() { return preview.archiveItem }
    function archiveNames() { return preview.archiveNames() }
    function failureText() { return preview.failureText() }

    function activateNeighbour(base, name, isDir) {
        var target = root.pane.join(base, name)
        if (isDir)
            Nav.openPlace(root.pane, target)
        else
            root.pane.openFile(target)
    }

    function askThumb() { active.restartSettle() }
    function loadSelection() { preview.loadSelection() }
    function focusPreview() { if (preview.visible) preview.forceActiveFocus() }

    // "Kind=MPEG-4 video|Duration=1:12|...", so a test reads the preview column's own table.
    function factsLine() {
        if (root.cursorIsDir)
            return ""
        var f = preview.factRows
        var out = []
        for (var i = 0; i < f.length; i++) out.push(f[i].label + "=" + f[i].value)
        return out.join("|")
    }

    // Empty while the cursor is on a directory: the third column is that directory's own rows then,
    // and reporting a hidden preview's state would read as if one were showing.
    function previewStateName() { return root.cursorIsDir ? "" : preview.previewState }
    function mediaPlaying() { return preview.mediaPlaying() }
    function mediaPosition() { return preview.mediaPosition() }
    function mediaStrip() { return preview.mediaStripItem() }
    function pdfPage() { return preview.pdfPage() }
    readonly property alias previewColumn: preview
    function pdfPages() { return preview.pdfPages }
    function pdfChevron(dir) { return preview.pdfChevron(dir) }
    function pdfLoaded() { return preview.pdfLoaded() }

    // The kind string the backend already sent for this row, never one the column re-derives.
    function kindName(index) {
        var row = root.pane.rowFor(index)
        return row && root.pane.kindNames[row.k] !== undefined ? root.pane.kindNames[row.k] : ""
    }

    function selectedRowObjects() {
        var out = []
        var idx = root.pane.selectedIndices()
        for (var i = 0; i < idx.length; i++) {
            var row = root.pane.rowFor(idx[i])
            if (row)
                out.push(row)
        }
        return out
    }

    onParentPathChanged: root.refreshNeighbours()
    // Toggling Hidden files re-asks the shown ancestors under the new key, or every column goes blank.
    readonly property string peekOrder: (root.pane.showHidden === true ? "1" : "0") + (ViewState.state.hiddenLast === true ? "1" : "0")
    onPeekOrderChanged: if (visible) root.refreshNeighbours()
    // The rows/cursor move can early-return on values this binding had not settled yet, so its own change re-moves with fresh ones.
    onChildPathChanged: { root.refreshNeighbours(); root.moveThird() }
    // Widening over a step shows an ancestor never asked for; ask() stays a no-op for the rest.
    onColumnCountChanged: root.refreshNeighbours()
    Connections {
        target: root.pane
        function onCursorIndexChanged() { root.followCursor() }
        // A meta asked before the new listing landed is answered with silence, because the row index
        // is outside the listing the backend still holds. The rows arriving re-asks through
        // followCursor, and preview.followSelection keeps a held or loaded identity rather than
        // clearing it, so a landing that already has its facts does not race its own reply.
        function onRowsChanged() { root.followCursor() }
    }
    Component.onCompleted: { root.showCursorRow(); root.refreshNeighbours() }
    // The view is built with the pane and only shown later, so neither path nor cursor has changed
    // by the time it first appears; becoming visible is the trigger that asks for everything.
    onVisibleChanged: if (visible) root.refreshNeighbours()

    Connections {
        target: root.pane.backend

        // The backend watches each column's directory and says so with the line the listed folder gets; the pane's own path is PaneWire's.
        function onChanged(path) { if (root.peeked[root.peekKey(path)] !== undefined) root.ask(path, true) }

        // hidden, hiddenLast and first are the request's own, echoed; first keeps a 1 or 512 repair peek out of a column waiting on the window size.
        function onPeeked(path, hidden, total, rows, readFailed, mode, hiddenLast, first) {
            var key = Columns.peekKey(path, hidden, hiddenLast), sent = Columns.sentKey(key, first)
            if (!Columns.hasAsk(root.pending, sent)) return
            // A data-held empty folder lands whole, the settled frame the picture hold revealed.
            var heroSettles = Columns.shouldSettleHero(root.cursorIsDir, path, root.childPath, readFailed, rows.length)
            if (heroSettles) childColumn.emptyItem.animateEntrance = false
            root.pending = Columns.dropAsk(root.pending, sent)
            var next = root.peeked
            next[key] = rows
            root.peeked = next
            if (readFailed) {
                var locked = root.denials
                locked[key] = mode
                root.denials = locked
            }
            root.peekVersion += 1
            // A landed peek shows the waiting folder whole; a live picture ends before those rows do.
            if (Columns.showFolderOnPeek(root.childPath, root.shownChildPath, root.answered(root.childPath))) {
                thirdSwap.cancel()
                root.showCursorRow()
            }
            if (heroSettles) { childColumn.emptyItem.markItem.settle(); childColumn.emptyItem.animateEntrance = true }
            if (root.answered(root.childPath))
                folderFallback.stop()
        }
    }

    // A new listing invalidates every cached column: the same rule thumbnails and dirsizes follow.
    Connections {
        target: root.pane
        function onPathChanged() {
            root.peeked = ({})
            root.pending = ({})
            root.denials = ({})
            root.peekVersion += 1
            root.refreshNeighbours()
        }
    }

    Row {
        anchors.fill: parent

        // The great-grandparent, built only on a window wide enough for five columns, so the shipped 3 never builds it.
        Loader {
            id: greatGrandparentLoader
            active: root.showGreatGrandparent
            width: root.showGreatGrandparent ? root.columnWidth : 0
            height: parent.height
            sourceComponent: Flea.ColumnPane {
                anchors.fill: parent
                rows: root.greatGrandparentShown ? root.rowsFor(root.greatGrandparentPath) : []
                lockedMode: root.greatGrandparentShown ? root.deniedMode(root.greatGrandparentPath) : -1
                drawsEmpty: root.greatGrandparentShown && root.answered(root.greatGrandparentPath)
                liftedName: root.greatGrandparentShown ? Nav.leafOf(root.grandparentPath) : ""
                dim: true
                showDivider: true
                onActivated: function (name, isDir) { root.activateNeighbour(root.greatGrandparentPath, name, isDir) }
                onRowPressed: root.pane.pressSlowClick()
                onNeighbourMenuRequested: function (name) { root.menuOnNeighbour(root.greatGrandparentPath, name) }
                onNeighbourBackgroundRequested: function (eventPoint) {
                    if (root.showGreatGrandparent && root.greatGrandparentShown)
                        root.menuOnNeighbourBackground(root.greatGrandparentPath, eventPoint)
                }
                onTabRequested: function (row) { Tap.tappedTab(row, root.greatGrandparentPath, root.pane) }
            }
        }

        // The grandparent, built only on a window wide enough for four columns, for the same reason.
        Loader {
            id: grandparentLoader
            active: root.showGrandparent
            width: root.showGrandparent ? root.columnWidth : 0
            height: parent.height
            sourceComponent: Flea.ColumnPane {
                anchors.fill: parent
                rows: root.grandparentShown ? root.rowsFor(root.grandparentPath) : []
                lockedMode: root.grandparentShown ? root.deniedMode(root.grandparentPath) : -1
                drawsEmpty: root.grandparentShown && root.answered(root.grandparentPath)
                liftedName: root.grandparentShown ? Nav.leafOf(root.parentPath) : ""
                dim: true
                showDivider: true
                onActivated: function (name, isDir) { root.activateNeighbour(root.grandparentPath, name, isDir) }
                onRowPressed: root.pane.pressSlowClick()
                onNeighbourMenuRequested: function (name) { root.menuOnNeighbour(root.grandparentPath, name) }
                onNeighbourBackgroundRequested: function (eventPoint) {
                    if (root.showGrandparent && root.grandparentShown)
                        root.menuOnNeighbourBackground(root.grandparentPath, eventPoint)
                }
                onTabRequested: function (row) { Tap.tappedTab(row, root.grandparentPath, root.pane) }
            }
        }

        // The parent shows the current directory among its siblings; below 900 px it hides.
        Flea.ColumnPane {
            id: parentColumn
            visible: root.showParent
            width: root.showParent ? root.columnWidth : 0
            height: parent.height
            rows: (root.showParent && root.parentShown) ? root.rowsFor(root.parentPath) : []
            lockedMode: (root.showParent && root.parentShown) ? root.deniedMode(root.parentPath) : -1
            drawsEmpty: root.parentShown && root.answered(root.parentPath)
            liftedName: root.parentShown ? Nav.leafOf(root.pane.path) : ""
            dim: true
            showDivider: true
            onActivated: function (name, isDir) { root.activateNeighbour(root.parentPath, name, isDir) }
            onRowPressed: root.pane.pressSlowClick()
            onNeighbourMenuRequested: function (name) { root.menuOnNeighbour(root.parentPath, name) }
            onNeighbourBackgroundRequested: function (eventPoint) {
                if (root.showParent && root.parentShown)
                    root.menuOnNeighbourBackground(root.parentPath, eventPoint)
            }
            onTabRequested: function (row) { Tap.tappedTab(row, root.parentPath, root.pane) }
        }

        // The pane's own listing, which is why this column and only this one takes the accent.
        Flea.ColumnPane {
            id: active
            width: root.columnWidth
            height: parent.height
            rows: root.pane.rows
            selectedIndex: root.pane.cursorIndex
            // Only this column's rows are the pane's own, so only it can paint the pane's selection.
            pane: root.pane
            showDivider: root.thirdShown
            // The list's and the grid's own two routes, reached from the one column whose rows are the pane's listing, so a click means the same thing in all three views.
            onPicked: function (index, tapCount, modifiers, onName) {
                var wasSole = root.pane.slowClickWasSole(index)
                Tap.tappedMiddle(index, tapCount, modifiers, root.pane)
                // The slow click renames on the pane's timer; a double click opens through tappedMiddle() above instead.
                if (tapCount === 2) root.pane.cancelSlowClick()
                else if (tapCount === 1 && onName) root.pane.armSlowClick(index, modifiers, active.fileDragActive, wasSole)
                else root.pane.cancelSlowClick()
            }
            onMenuRequested: function (index, eventPoint) { Tap.tappedMenu(index, eventPoint, root.pane, root.menu) }
            onRowPressed: root.pane.pressSlowClick()
            onTabRequested: function (row) { Tap.tappedTab(row, root.pane.path, root.pane) }
            onBackgroundMenuRequested: function (eventPoint) { root.menu.openBackground(eventPoint.scenePosition) }
            onThumbsApplied: function (work) { root.thumbsApplied(work) }
            onDirSizesApplied: function (ask) { root.dirSizesApplied(ask) }
        }

        // The cursor row: what is inside it when it is a directory, what it is when it is a file.
        Flea.PreviewSwap {
            id: thirdSwap
            width: root.width - root.shownBefore * root.columnWidth
            height: parent.height
            burstEnds: true
            ready: root.thirdReady
            folderHold: root.folderWaiting
            ground: Glass.planeAlpha < 1 ? "transparent" : Theme.color.background

            Flea.ColumnPane {
                id: childColumn
                anchors.fill: parent
                visible: root.shownIsDir
                rows: root.rowsFor(root.shownChildPath)
                lockedMode: root.deniedMode(root.shownChildPath)
                drawsEmpty: root.answered(root.shownChildPath)
                onActivated: function (name, isDir) { root.activateNeighbour(root.shownChildPath, name, isDir) }
                onRowPressed: root.pane.pressSlowClick()
                onNeighbourMenuRequested: function (name) { root.menuOnNeighbour(root.shownChildPath, name) }
                onNeighbourBackgroundRequested: function (eventPoint) {
                    if (root.shownIsDir && root.shownChildPath.length > 0)
                        root.menuOnNeighbourBackground(root.shownChildPath, eventPoint)
                }
                onTabRequested: function (row) { Tap.tappedTab(row, root.shownChildPath, root.pane) }
            }

            Flea.SelectionPreview {
                id: preview
                anchors.fill: parent
                visible: root.shownHasRow && !root.shownIsDir && ViewState.previewColumn
                pane: root.pane
                swap: thirdSwap
                onThumbsApplied: function (work) { root.thumbsApplied(work) }
            }
        }
    }

    // For ui/Ipc.qml: the names a drawn neighbour column holds, "|" joined, and "" when that column is not drawn.
    function drawnNames(slot) {
        var drawn = slot === "child" ? root.shownIsDir : (slot === "parent" && root.showParent && root.parentShown)
        return drawn ? (slot === "child" ? childColumn.rows : parentColumn.rows).map(function (row) { return row.n }).join("|") : ""
    }
    // For ui/Ipc.qml: the names the window last read for a folder, drawn or not.
    function peekNames(path) { return root.rowsFor(path).map(function (row) { return row.n }).join("|") }
}
