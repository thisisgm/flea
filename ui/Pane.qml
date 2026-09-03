import QtQuick
import Quickshell
import "." as Flea
import "js/DirSizes.js" as DirSizes
import "js/Filter.js" as Filter
import "js/Focus.js" as Focus
import "js/Search.js" as Search
import "js/Archive.js" as Archive
import "js/Nav.js" as Nav
import "js/Ops.js" as Ops
import "js/Selection.js" as Selection
import "js/Sort.js" as Sort
import "js/Thumbs.js" as Thumbs

FocusScope {
    id: root
    focus: true

    property var backend: null
    property string path: ""
    // Set once by shell.qml from FLEA_SELECT; applied to the first `rows` this pane receives, then forgotten.
    property string pendingSelect: ""
    property int total: 0
    property int cursorIndex: 0
    property string listingState: "loading"
    property string stateMessage: ""
    property int lockedMode: 0
    // Off by default: dotfiles stay out of every listing until the context menu or "." turns them on.
    property bool showHidden: false
    property string typed: ""
    // "" off, "typing" while the query line has the keyboard, "results" once a walk was asked for; ui/js/Search.js owns every transition.
    property string searchMode: ""
    // Where the search was started from, which a home-wide walk leaves behind; see ui/js/Search.js.
    property string searchFrom: ""
    property string searchQuery: ""
    property bool searchRunning: false
    property bool searchCancelled: false
    // The query narrowing the listing in place, and whether its line still has the keyboard;
    // ui/js/Filter.js owns every transition, the way ui/js/Search.js owns the walk's.
    property string filterQuery: ""
    property bool filterTyping: false
    property int searchScanned: 0
    property real searchMs: 0
    property bool listInFlight: false
    property bool listedSeen: false
    readonly property bool menuVisible: menu.opened
    // The keyboard-highlighted menu row, so a test can drive the menu without OCR.
    readonly property int menuCursor: menu.cursor
    // What the header case reads over IPC, the same alias idiom Row.qml uses for its icon.
    readonly property alias header: header
    readonly property alias sidebar: sidebar  // shell.qml reads addRequested/networkEntries through this
    // Which view Tab last handed the keyboard to; the rail's own cursor is read the same way.
    property string focusView: Focus.LIST
    readonly property alias railCursor: sidebar.cursorIndex
    // So a test can wait for the rail's async FileViews instead of sleeping and guessing.
    readonly property int railCount: sidebar.entries.length
    property var preview: null
    // shell.qml's ui/ShareBrowser.qml overlay, wired the same way as preview above.
    property var shareBrowser: null
    // shell.qml's ui/KeymapSheet.qml, which ? opens from either the list or the rail.
    property var keymapSheet: null

    signal opened(string path)
    signal message(string text, bool isError)
    // The status bar's sticky slot, which unlike message does not time out; empty clears it.
    signal sticky(string text)
    // The one popup, hosted in shell.qml beside the network dialog rather than inside the pane.
    signal convertRequested(string name)
    signal pathBarRequested()  // ":" and Ctrl+L; the bar is chrome, so shell.qml opens it as it does the popup above

    // The window covers [held, held + rows.length) and nothing outside it is in memory.
    property int held: 0
    property var rows: []
    // The current response's Kind dictionary; every row's own "k" is an index into this one array.
    property var kindNames: []
    // The listing rows the filter leaves standing, null when none is up: a view position goes in and
    // a listing row comes out, which is what every row-indexed cache and the selection still hold.
    readonly property var shown: Filter.shown(root.rows, root.held, root.filterQuery)
    readonly property int shownTotal: root.shown === null ? root.total : root.shown.length

    readonly property int minBuffer: 50
    readonly property int maxBuffer: 1000
    readonly property int defaultBuffer: 150
    property int bufferRows: defaultBuffer
    readonly property int buffer: Math.max(minBuffer, Math.min(maxBuffer, bufferRows))
    readonly property int visibleRows: Math.max(1, Math.ceil(list.height / Theme.rowHeight))
    readonly property int windowSize: visibleRows + 2 * buffer
    readonly property int refetchMargin: 25
    readonly property int cacheRows: 4
    readonly property int coalesceMs: 16
    readonly property int typeAheadClearMs: 900
    // A settle, not a stream: a fling must issue no request at all, see AGENTS.md "Thumbnail requests in the GUI".
    readonly property int settleMs: 120
    // The first screen's settle only has to outlast the compositor's resize, see AGENTS.md "Thumbnail requests in the GUI".
    readonly property int firstSettleMs: 70
    // Seven screens of history at this row height, so a policy bug costs memory slowly, not without limit.
    readonly property int thumbCap: 240
    property var thumbState: Thumbs.empty()
    // Same cap as thumbState, same reason; a directory row's size is only ever asked for by a viewport.
    property var dirSizeState: DirSizes.empty()
    // Input to rows is stamped inside the UI, because a harness that polls IPC across the interval times itself; see AGENTS.md "Testing".
    property real inputAt: 0
    property real rowsAt: 0

    // The cut or copied paths, absolute because a paste lands in a different directory; see ui/js/Ops.js.
    property var clipboard: Ops.emptyClipboard()
    // The mode of an askPaths round trip in flight, or null; nothing reaches the clipboard until it answers.
    property var clipPending: null
    // Which asker a pending paths reply belongs to, null meaning the clipboard, which is what every
    // reply meant before compress also had to resolve a selection wider than this pane holds.
    property var pathsPending: null
    // What the status bar's sticky slot is reporting, or an idle transfer; see ui/js/Ops.js.
    property var transfer: Ops.emptyTransfer()
    // The row that is its own editor right now, or -1; ui/List.qml's delegate reads it per row.
    property int renamingIndex: -1
    // Set by a pointer-committed rename so the reply reveals nothing; ui/js/Nav.js clears it.
    property bool renameKeepsPointerRow: false

    // Read once for the window: the chrome's path and the search strip's scope both shorten with it.
    readonly property string home: Quickshell.env("HOME") || ""

    // "list", "columns" or "grid"; the chrome's own buttons write it and the views read it.
    property string viewMode: "list"
    // Only the list view draws a filter, so leaving it takes the filter with it.
    onViewModeChanged: Filter.close(root)

    // Directories already visited, newest last, so the chrome's back arrow has somewhere to go.
    // Deliberately not a forward stack: the canvas draws one arrow, not two.
    property var history: []
    property var tabs: null
    readonly property bool canGoBack: root.history.length > 0
    readonly property bool canGoUp: root.path.length > 1

    // The filesystem line the status bar draws, refreshed once per directory rather than per row.
    property string fsName: ""
    property real fsFree: 0

    function goBack() { Nav.back(root) }

    // Rename lives in ui/js/Ops.js with the other write operations; ui/List.qml's editor commits through this.
    function commitRename(newName) { Ops.commitRename(root, newName) }

    // A set of row indices over the current listing, mutated in place; selectionVersion tells a reactive binding (List.qml's delegate, StatusBar's count) to re-read it. Task 8 declined ScriptModel plus ItemSelectionModel on measured memory, see AGENTS.md "The list model".
    property var selection: Selection.create()
    property int selectionVersion: 0
    property int selectionAnchor: 0
    function isSelected(index) { return root.selectionVersion >= 0 && root.selection.has(index) }
    function selectionCount() { return root.selectionVersion >= 0 ? root.selection.count() : 0 }
    function selectedIndices() { return root.selectionVersion >= 0 ? root.selection.indices() : [] }
    function toggleSelect() { root.selection.toggle(root.cursorIndex); root.selectionAnchor = root.cursorIndex; root.selectionVersion++ }
    function selectAll() { Filter.selectAll(root); root.selectionVersion++ }
    function clearSelection() { root.selection.clear(); root.selectionVersion++ }
    function extendSelection(delta) { Filter.extend(root, delta) }
    // Ctrl+click and shift+click, the mouse's twins of v and shift+j/k; see keys.toml's [[pointer]].
    function toggleSelectAt(index) { root.setCursor(index); root.toggleSelect() }
    function extendSelectionTo(index) { Filter.extendToRow(root, index) }
    // Named escapePressed, not escape, which collides with the JS global URI function; clears an active selection first, see keys.toml.
    function escapePressed() { if (root.selection.count() > 0) { root.clearSelection(); return }; root.message("", false) }

    function applyPendingSelect() { Nav.applyPendingSelect(root) }
    function refresh(selectPath) { Nav.refresh(root, selectPath) }

    function open(newPath) { Nav.open(root, newPath) }

    function openWithoutHistory(newPath) { Nav.openWithoutHistory(root, newPath) }

    // The toggle re-lists rather than filtering client-side: the model is a row count over the
    // backend's own listing, which never held the dotfiles to begin with when they were off.
    // Re-listing also clears the cursor and selection, the same as opening any other directory.
    function toggleHidden() {
        root.showHidden = !root.showHidden
        root.open(root.path)
    }

    function rowFor(index) {
        var offset = index - root.held
        if (offset < 0 || offset >= root.rows.length)
            return null
        return root.rows[offset]
    }

    // The list's own delegate, which is what every reader of a rendered row cell needs; only the
    // list draws those cells at all.
    function itemFor(index) {
        return list.itemAtIndex(index)
    }

    // The delegate actually on screen, whichever view is showing. A coordinate taken off a hidden
    // view is a coordinate nothing can be clicked at, which is what the columns view used to return.
    // index is a listing row and every view's itemAtIndex wants a view position, which is a different
    // number under a filter: passing the listing row answered null for a row plainly on screen, and
    // once the match count passed it, the delegate at that position under an unrelated row.
    function visibleItemFor(index) {
        return root.listArea.itemAtIndex(Filter.viewOf(root.shown, index))
    }

    // shell.qml's IPC thumbFile reader calls this; the lookup lives with the thumbnail machinery in ui/List.qml.
    function thumbFor(index) { return list.thumbFor(index) }

    // Lifted to Focus.act and Focus.railAct, see ui/js/Focus.js; each just names its own target.
    function act(action) { Focus.act(action, root) }
    function railAct(action) { Focus.railAct(action, root, sidebar) }

    // index is a listing row, which is what every caller outside ui/js/Filter.js holds; the clamp
    // and the scroll both happen in view space, because a filter can be narrowing what is drawn.
    function setCursor(index) { Filter.setCursor(root, index) }
    // ListView.Contain has no name inside a .pragma library, so the scroll itself stays here.
    function showRow(view) { root.listArea.positionViewAtIndex(view, ListView.Contain); root.listArea.restartCoalesce() }

    // ui/js/Tap.js's click-away commit; only ui/List.qml draws an editor, so only it is asked.
    function commitOpenRename() { if (root.renamingIndex >= 0) list.commitOpenRename() }

    // The live editor or null: a set renamingIndex is not evidence one exists, see ui/RenameField.qml.
    function renameEditor() { return root.viewMode === "list" ? list.renameEditor() : null }

    function openCursor() { Nav.openCursor(root, wire.opener) }

    // A path the caller already resolved, for the columns view's neighbour rows, which have no cursor.
    function openFile(path) { wire.opener.open(path) }

    function openParent() { Nav.parent(root) }

    function join(base, name) {
        return base === "/" ? "/" + name : base + "/" + name
    }

    function typeAhead(character) { Nav.typeAhead(root, character, typedClear) }

    Flea.PaneWire {
        id: wire
        pane: root
    }

    Flea.Sidebar {
        id: sidebar
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        focused: root.focusView === Focus.RAIL
        onOpened: function (path) { root.open(path) }
        onMessage: function (text, isError) { root.message(text, isError) }
        menu: menu
        // The rename TextField takes real Qt focus itself; this only hands it back once it is done.
        onRenameFinished: list.forceActiveFocus()
    }

    Flea.Header {
        id: header
        // Only the list view has columns to head, and neither the grid board nor the columns board
        // draws one; the strip collapses rather than hiding, so the view below starts at the top of
        // the pane instead of a gap. A search takes the strip whole, in any view.
        visible: root.viewMode === "list" || root.searchMode.length > 0
        height: visible ? implicitHeight : 0
        anchors.top: parent.top
        anchors.left: sidebar.right
        anchors.right: parent.right
        sortBy: root.backend.sortBy
        sortDesc: root.backend.sortDesc
        onSortRequested: function (key) { Sort.column(root, key) }
        searchMode: root.searchMode
        searchQuery: root.searchQuery
        searchScope: Search.scope(Search.scopeRoot(root.path, root.home), root.home)
        searchNote: Search.note(root.total, root.searchRunning, root.searchCancelled)
    }

    // The two views share the same slot, the same rows and the same cursor; only one is ever up, and
    // listArea points at whichever it is, so every caller of restartSettle stays view-agnostic and
    // shell.qml lays the empty-state overlay over the right one.
    readonly property var listArea: root.viewMode === "grid" ? grid
                                  : root.viewMode === "columns" ? columns : list
    // How far a cursor step down moves: one row in the list, one row of tiles in the grid.
    // The columns view's own preview, exposed so a test can assert its facts without OCR.
    readonly property var columnsArea: columns
    readonly property int cursorStride: root.viewMode === "grid" ? grid.columns : 1

    Flea.FilterStrip {
        id: filterStrip
        anchors.top: header.bottom
        anchors.left: sidebar.right
        anchors.right: parent.right
        pane: root
    }

    Flea.ColumnsArea {
        id: columns
        visible: root.viewMode === "columns"
        focus: root.viewMode === "columns"
        anchors.top: filterStrip.bottom
        anchors.left: sidebar.right
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        pane: root
        menu: menu
        Keys.onPressed: function (event) { event.accepted = Focus.handleKey(event, root, sidebar) }
    }

    Flea.GridArea {
        id: grid
        visible: root.viewMode === "grid"
        // Both views default to focus true, so the one that is not up has to give it back explicitly:
        // a hidden item holding focus swallows every key the visible one should have had.
        focus: root.viewMode === "grid"
        anchors.top: filterStrip.bottom
        anchors.left: sidebar.right
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        pane: root
        menu: menu
        onThumbsApplied: function (work) { root.thumbState = Thumbs.applied(root.thumbState, work) }
        onDirSizesApplied: function (ask) { root.dirSizeState = DirSizes.applied(root.dirSizeState, ask) }
        onDirSizesCancelled: root.dirSizeState = DirSizes.cancelled(root.dirSizeState)

        // The same seam the list carries: whichever view is up owns the keyboard, and Focus.handleKey
        // is the one route either of them takes.
        Keys.onPressed: function (event) { event.accepted = Focus.handleKey(event, root, sidebar) }
    }

    Flea.List {
        id: list
        visible: root.viewMode === "list"
        focus: root.viewMode === "list"
        anchors.top: filterStrip.bottom
        anchors.left: sidebar.right
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        pane: root
        menu: menu

        // List only computes the clamp and the thumbnail plan; cursorIndex and thumbState are Pane's own to write.
        onCursorClamped: function (first, last) { Filter.clampCursor(root, first, last) }
        onThumbsApplied: function (work) { root.thumbState = Thumbs.applied(root.thumbState, work) }
        onDirSizesApplied: function (ask) { root.dirSizeState = DirSizes.applied(root.dirSizeState, ask) }
        onDirSizesCancelled: root.dirSizeState = DirSizes.cancelled(root.dirSizeState)

        // The whole route lives in Focus.handleKey now, see its own comment; this is only the seam.
        Keys.onPressed: function (event) { event.accepted = Focus.handleKey(event, root, sidebar) }
    }

    // A directory has nothing Taildrop can send today, so the entry hides for one rather than
    // opening a submenu that always no-ops; reactive on both the cursor and the held window.
    readonly property var cursorRow: root.rowFor(root.cursorIndex)

    Flea.ContextMenu {
        id: menu
        showHidden: root.showHidden
        taildropPeers: (root.cursorRow && !root.cursorRow.d) ? wire.taildrop.peers : []
        archiveFormats: root.backend.archiveFormats
        canConvert: root.backend.canConvert
        rowIsArchive: root.cursorRow !== null && !root.cursorRow.d && Archive.isArchive(root.cursorRow.n)
        rowIsImage: root.cursorRow !== null && root.cursorRow.i === "image-x-generic"
        dropboxPath: sidebar.dropboxReady ? root.home + "/Dropbox" : ""
        // The separator is part of the test, or /home/gm/DropboxBackup would count as inside Dropbox.
        rowInDropbox: root.path === root.home + "/Dropbox"
                      || root.path.indexOf(root.home + "/Dropbox/") === 0
        onChosen: function (action) {
            if (action.indexOf("taildrop:") === 0) {
                root.sendTaildrop(action.substring("taildrop:".length))
                return
            }
            root.act(action)
        }
    }

    // shell.qml's IPC reads this to assert menu contents without OCR, see docs "Testing".
    function menuEntries() { return menu.entries }
    function menuSubmenuGlyphs() { return menu.submenuGlyphs() }
    function menuSubmenuEntries() { return menu.submenuEntries }

    function openConvert() { Ops.openConvert(root) }
    function moveToDropbox() { Ops.moveToDropbox(root, sidebar.dropboxReady ? root.home + "/Dropbox" : "") }
    // The three foreign programs live in ui/PaneWire.qml with the backend's replies; these only name the row.
    function copyShareLink() { wire.shareLink.copy(root.join(root.path, root.cursorRow ? root.cursorRow.n : "")) }
    function sendTaildrop(peerId) { Ops.sendTaildrop(root, wire.taildrop, peerId) }

    // The keyboard's own entrance to the row menu, under the cursor row the way the rail's opens under
    // its own; setCursor first, because a wheel scroll in the grid can leave the cursor off screen.
    function openCursorMenu() {
        root.setCursor(root.cursorIndex)
        var row = root.visibleItemFor(root.cursorIndex)
        if (row)
            menu.openAt(row.mapToItem(null, Theme.spacing.rowPaddingX, row.height))
        return row !== null
    }

    Flea.StateMessage {
        anchors.fill: root.listArea
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.rightMargin: Theme.spacing.rowPaddingX
        message: root.stateMessage
        listingState: root.listingState
        lockedMode: root.lockedMode
        total: root.total
    }

    Timer {
        id: typedClear
        interval: root.typeAheadClearMs
        repeat: false
        onTriggered: root.typed = ""
    }
}
