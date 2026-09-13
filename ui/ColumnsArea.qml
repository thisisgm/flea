import QtQuick
import qs.Commons
import "." as Flea
import "js/Facts.js" as Facts
import "js/Focus.js" as Focus
import "js/Nav.js" as Nav
import "js/Thumbs.js" as Thumbs
import "js/Tap.js" as Tap

// The Miller three-pane. The parent and the child are read with peek, which never touches the pane's
// own listing; the middle column is that listing, so the cursor, the selection and every per-row
// facility keep working exactly as they do in the list view.
Item {
    id: root

    property var pane: null
    property var menu: null
    // The active column's thumbnail plan, relayed for ui/Pane.qml to write, the grid's own contract.
    signal thumbsApplied(var work)
    // Whichever view is up owns the keyboard, and Focus.handleKey is the one route all three take.
    Keys.onPressed: function (event) { event.accepted = Focus.handleKey(event, root.pane, root.pane.sidebar) }

    // path -> the rows a peek answered for it. Cleared whenever the pane moves, because a stale
    // column is worse than an empty one.
    property var peeked: ({})
    // path -> the mode of a peek that came back denied, which answers zero rows as an empty one does.
    property var denials: ({})
    property int peekVersion: 0

    readonly property string parentPath: Nav.parentOf(root.pane.path)
    // The meta the preview column names: pixels, line count, symlink target, for the cursor row only.
    property var cursorMeta: null
    readonly property var cursorRow: root.pane.rowFor(root.pane.cursorIndex)
    readonly property bool cursorIsDir: root.cursorRow !== null && root.cursorRow.d === true
    readonly property string childPath: root.cursorIsDir
        ? root.pane.join(root.pane.path, root.cursorRow.n) : ""

    // A third of the view for each of the two fixed columns; the third takes the remainder, so
    // a width that does not divide by three leaves no gap. shell.qml's empty hero takes it too.
    readonly property int columnWidth: Math.floor(root.width / 3)

    // A read of peeked that a binding re-evaluates when a peek lands; peekVersion is the trigger.
    function rowsFor(path) {
        return root.peekVersion >= 0 && path.length > 0 && root.peeked[path] ? root.peeked[path] : []
    }

    // -1 for a path that was not denied, so mode 0, the denial whose stat failed too, stays its own answer.
    function deniedMode(path) {
        return root.peekVersion >= 0 && root.denials[path] !== undefined ? root.denials[path] : -1
    }

    // A peek still out answers zero rows the way an empty directory does, so a column holds its empty tile back until the reply has actually landed for that path.
    function answered(path) {
        return root.peekVersion >= 0 && path.length > 0 && root.peeked[path] !== undefined
    }

    function ask(path) {
        if (path.length === 0 || root.peeked[path])
            return
        root.pane.backend.peek(path, root.pane.windowSize, root.pane.showHidden)
    }

    // Both neighbours are asked for on every move; ask() is a no-op for one already answered.
    function refreshNeighbours() {
        root.ask(root.parentPath)
        root.ask(root.childPath)
        root.askMeta()
        root.askThumb()
    }

    // One row, only when the preview column is actually the surface showing: the same no-sweep rule
    // thumb and dirsize already follow.
    function askMeta() { preview.followSelection() }

    // The listArea contract every caller of the pane's own navigation uses: the listing's column plans its own viewport's thumbnails, the way the list and the grid do.
    function primeSettle() { active.primeSettle() }
    function restartCoalesce() { active.restartCoalesce() }
    function restartSettle() { active.restartSettle() }
    function positionViewAtIndex(index, mode) { active.positionViewAtIndex(index, mode) }
    // The one column whose rows are the pane's own, for ui/Ipc.qml: the two beside it are peeks and
    // answer for another directory, so neither is where a background right click belongs.
    function activeColumn() { return active }
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

    // For ui/Ipc.qml: the peek columns' rows and the child column's empty tile, which pane.visibleItemFor cannot reach.
    function parentItemAt(index) { return parentColumn.itemAtIndex(index) }
    function childItemAt(index) { return childColumn.itemAtIndex(index) }
    function childEmptyItem() { return childColumn.emptyItem }
    function frameItem() { return preview.frameItem }
    function playerLoaded() { return preview.playerLoaded() }
    readonly property int previewIndex: preview.visible ? preview.loadedIndex : -1
    function thumbShown() { return preview.thumbShown }
    function frameReady() { return preview.frameStatus === Image.Ready }
    function textLines() { return preview.textLines() }
    function linesItem() { return preview.linesItem }
    function archiveItem() { return preview.archiveItem }
    function archiveNames() { return preview.archiveNames() }
    function failureText() { return preview.failureText() }

    function activateNeighbour(base, name, isDir) {
        var target = root.pane.join(base, name)
        if (isDir)
            root.pane.open(target)
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
    onChildPathChanged: root.refreshNeighbours()
    Connections {
        target: root.pane
        function onCursorIndexChanged() { root.askMeta() }
        // A meta asked before the new listing landed is answered with silence, because the row index
        // is outside the listing the backend still holds. The rows arriving is what re-asks it, and
        // only when nothing has answered yet, so the column cannot sit on Loading and a landing that
        // already has its facts does not ask a second time and race its own reply.
        function onRowsChanged() { if (root.cursorMeta === null) root.askMeta() }
    }
    Component.onCompleted: root.refreshNeighbours()
    // The view is built with the pane and only shown later, so neither path nor cursor has changed
    // by the time it first appears; becoming visible is the trigger that asks for everything.
    onVisibleChanged: if (visible) root.refreshNeighbours()

    Connections {
        target: root.pane.backend

        // hidden is the request's own flag, echoed; this view asks with the listing's and has only
        // ever one answer per path, so it reads the rows and lets the path bar do the correlating.
        function onPeeked(path, hidden, total, rows, readFailed, mode) {
            var next = root.peeked
            next[path] = rows
            root.peeked = next
            if (readFailed) {
                var locked = root.denials
                locked[path] = mode
                root.denials = locked
            }
            root.peekVersion += 1
        }
    }

    // A new listing invalidates every cached column: the same rule thumbnails and dirsizes follow.
    Connections {
        target: root.pane
        function onPathChanged() {
            root.peeked = ({})
            root.denials = ({})
            root.peekVersion += 1
            root.refreshNeighbours()
        }
    }

    Row {
        anchors.fill: parent

        // The parent, showing where the current directory sits among its own siblings. Its own row
        // for the current directory is the cursor trail: lifted like a hover, never accented.
        Flea.ColumnPane {
            id: parentColumn
            width: root.columnWidth
            height: parent.height
            rows: root.rowsFor(root.parentPath)
            lockedMode: root.deniedMode(root.parentPath)
            drawsEmpty: root.answered(root.parentPath)
            liftedName: Nav.leafOf(root.pane.path)
            dim: true
            onActivated: function (name, isDir) { root.activateNeighbour(root.parentPath, name, isDir) }
            onNeighbourMenuRequested: function (name) { root.menuOnNeighbour(root.parentPath, name) }
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
            // The list's and the grid's own two routes, reached from the one column whose rows are the pane's listing, so a click means the same thing in all three views.
            onPicked: function (index, tapCount, modifiers) { Tap.tappedMiddle(index, tapCount, modifiers, root.pane) }
            onMenuRequested: function (index, eventPoint) { Tap.tappedMenu(index, eventPoint, root.pane, root.menu) }
            onBackgroundMenuRequested: function (eventPoint) { root.menu.openBackground(eventPoint.scenePosition) }
            onThumbsApplied: function (work) { root.thumbsApplied(work) }
        }

        // The cursor row: what is inside it when it is a directory, what it is when it is a file.
        Item {
            width: root.width - 2 * root.columnWidth
            height: parent.height

            Flea.ColumnPane {
                id: childColumn
                anchors.fill: parent
                visible: root.cursorIsDir
                rows: root.rowsFor(root.childPath)
                lockedMode: root.deniedMode(root.childPath)
                drawsEmpty: root.answered(root.childPath)
                onActivated: function (name, isDir) { root.activateNeighbour(root.childPath, name, isDir) }
                onNeighbourMenuRequested: function (name) { root.menuOnNeighbour(root.childPath, name) }
            }

            Flea.SelectionPreview {
                id: preview
                anchors.fill: parent
                visible: root.cursorRow !== null && !root.cursorIsDir && ViewState.previewColumn
                pane: root.pane
                onThumbsApplied: function (work) { root.thumbsApplied(work) }
            }
        }
    }
}
