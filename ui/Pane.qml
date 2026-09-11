import QtQuick
import Quickshell
import "." as Flea
import "js/DirSizes.js" as DirSizes
import "js/Dropbox.js" as Dropbox
import "js/Filter.js" as Filter
import "js/Focus.js" as Focus
import "js/Menu.js" as Menu
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
    // Set with pendingSelect by a right click on a peeked column row: the menu opens on the row once it is the cursor.
    property bool pendingMenu: false
    property int total: 0
    property int cursorIndex: 0
    property string listingState: "loading"
    property string stateMessage: ""
    property int lockedMode: 0
    // Off by default: dotfiles stay out of every listing until the context menu or "." turns them on.
    property bool showHidden: ViewState.state.hidden === true
    // Issue 27's state-file key: with it on a cursor step past an end comes round; ui/js/Focus.js step is the only reader.
    readonly property bool wrapAtEnds: ViewState.state.wrapAtEnds === true
    // When the first d of the dd pair landed; ui/js/Focus.js reads it and Nav's reset clears it.
    property double trashArmedAt: 0
    property string keySequence: ""
    property string keySequenceIdentity: ""
    // "" off, "typing" while the query line has the keyboard, "results" once a walk was asked for; ui/js/Search.js owns every transition.
    property string searchMode: ""
    // Where the search was started from, which a home-wide walk leaves behind; see ui/js/Search.js.
    property string searchFrom: ""
    property string searchQuery: ""
    // Issue 30: which scope the next walk takes, flipped by tab on the query line; see ui/js/Search.js.
    property bool searchHere: false
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
    property var sharedSidebar: null
    property var railPane: root
    readonly property var sidebar: root.sharedSidebar || railLoader.item
    readonly property real sidebarWidth: railLoader.width
    property bool paneFocused: true
    property bool listOnly: false
    signal focusRequested()
    // Which view Tab last handed the keyboard to; the rail's own cursor is read the same way.
    property string focusView: Focus.LIST
    readonly property int railCursor: root.sidebar ? root.sidebar.cursorIndex : 0
    // So a test can wait for the rail's async FileViews instead of sleeping and guessing.
    readonly property int railCount: root.sidebar ? root.sidebar.entries.length : 0
    property var preview: null
    property var statusBar: null
    // shell.qml's ui/ShareBrowser.qml overlay, wired the same way as preview above.
    property var shareBrowser: null
    // shell.qml's ui/KeymapSheet.qml, which ? opens from either the list or the rail.
    property var keymapSheet: null
    // shell.qml's ui/SettingsPanel.qml, which the comma key opens from the list and the rail alike, see act() below.
    property var settingsPanel: null
    property Item overlayParent: null
    readonly property alias trash: trashHost
    readonly property alias menuActions: menuActions
    readonly property alias emptyState: emptyState
    readonly property alias stateMessageItem: paneMessage
    readonly property alias retrySelectionText: wire.retrySelectionText
    readonly property string menuSelectionIdentity: JSON.stringify([root.path, root.held, root.rows,
        root.selectionVersion, root.cursorIndex, root.total, root.listInFlight])

    signal opened(string path)
    signal message(string text, bool isError)
    signal forgetMessage(string text)
    signal operationResult(string headline, string detail, bool isError)
    // The status bar's sticky slot, which unlike message does not time out; empty clears it.
    signal sticky(string text)
    // The one popup, hosted in shell.qml beside the network dialog rather than inside the pane.
    signal convertRequested(string name)
    signal permissionsRequested(string path)
    signal pathBarRequested()  // ":" and Ctrl+L; the bar is chrome, so shell.qml opens it as it does the popup above
    signal textSizeRequested(int direction)  // issue 9's zoom pair, +1, -1 or 0 to follow Omarchy again; the size is the window's

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
    readonly property int visibleRows: Math.max(1, Math.ceil(list.height / Theme.fileRowHeight))
    readonly property int windowSize: visibleRows + 2 * buffer
    readonly property int refetchMargin: 25
    readonly property int cacheRows: 4
    readonly property int coalesceMs: 16
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
    property bool preferencesReady: false
    Component.onCompleted: {
        root.viewMode = root.listOnly || ViewState.state.view === "dual" ? "list" : ViewState.state.view || "list"
        root.preferencesReady = true
    }
    // Only the list view draws a filter, so leaving it takes the filter with it.
    onViewModeChanged: {
        Filter.close(root)
        if (root.preferencesReady && !root.listOnly && (!root.dualMode || root.viewMode !== "list") && ViewState.state.view !== root.viewMode)
            ViewState.changeKey("view", root.viewMode)
    }
    readonly property string listingPreferences: JSON.stringify([ViewState.state.hidden, ViewState.state.sort,
        ViewState.state.foldersFirst, ViewState.state.groupByKind])
    property string appliedListingPreferences: ""
    onListingPreferencesChanged: {
        // A hidden dual pane retains its session sort when the single pane changes the saved default.
        if (root.backend && root.backend.preserveSort && !root.visible && root.appliedListingPreferences.length > 0) {
            var applied = JSON.parse(root.appliedListingPreferences)
            applied[1] = ViewState.state.sort
            root.appliedListingPreferences = JSON.stringify(applied)
        }
        preferences.restart()
    }
    onVisibleChanged: if (root.visible) preferences.restart()
    onListInFlightChanged: if (!root.listInFlight) preferences.restart()
    onSearchModeChanged: if (root.searchMode.length === 0) preferences.restart()
    Timer {
        id: preferences
        interval: 0
        onTriggered: {
            var desired = root.listOnly || ViewState.state.view === "dual" ? "list" : ViewState.state.view || "list"
            if (root.viewMode !== desired) root.viewMode = desired
            if (!root.visible || !root.path || root.listInFlight || root.searchMode.length > 0
                    || root.appliedListingPreferences === root.listingPreferences) return
            root.openWithoutHistory(root.path)
        }
    }
    Connections {
        target: ViewState
        function onStateChanged() { preferences.restart() }
    }


    // Each tab retains its own back and forward navigation.
    property var history: []
    property var forwardHistory: []
    property var tabs: null
    readonly property bool canGoBack: trashHost.opened || root.history.length > 0
    readonly property bool canGoUp: root.path.length > 1

    // The filesystem line the status bar draws, refreshed once per directory rather than per row.
    property string fsName: ""
    property real fsFree: 0

    function goBack() { if (trashHost.opened) trashHost.close(); else Nav.back(root) }
    function goForward() { if (!trashHost.opened) Nav.forward(root) }

    // Rename lives in ui/js/Ops.js with the other write operations; ui/List.qml's editor commits through this.
    function commitRename(newName) { Ops.commitRename(root, newName) }
    property int renameMenuId: 0
    property string renameSource: ""
    property string renameError: ""
    property var renameRequest: null
    readonly property bool renamePending: root.renameRequest !== null
    property var convertSource: null
    onRenamingIndexChanged: if (root.renamingIndex < 0) {
        root.renameMenuId = 0
        root.renameSource = ""
        root.renameError = ""
    }

    // A set of row indices over the current listing, mutated in place; selectionVersion tells a reactive binding (List.qml's delegate, StatusBar's count) to re-read it. Task 8 declined ScriptModel plus ItemSelectionModel on measured memory, see AGENTS.md "The list model".
    property var selection: Selection.create()
    property int selectionVersion: 0
    property int selectionAnchor: 0
    property var selectionBand: null
    function isSelected(index) { return root.selectionVersion >= 0 && root.selection.has(index) }
    function selectionCount() { return root.selectionVersion >= 0 ? root.selection.count() : 0 }
    function selectedIndices() { return root.selectionVersion >= 0 ? root.selection.indices() : [] }
    function toggleSelect() { root.selection.toggle(root.cursorIndex); root.selectionAnchor = root.cursorIndex; root.selectionVersion++ }
    function selectAll() { Filter.selectAll(root); root.selectionVersion++ }
    function clearSelection() { root.selection.clear(); root.selectionVersion++ }
    function selectOnly(index) {
        root.setCursor(index)
        root.selection.only(root.cursorIndex)
        root.selectionAnchor = root.cursorIndex
        root.selectionVersion++
    }
    function extendSelection(delta) { Filter.extend(root, delta) }
    // Ctrl+click and shift+click, the mouse's twins of v and shift+j/k; see keys.toml's [[pointer]].
    function toggleSelectAt(index) { root.setCursor(index); root.toggleSelect() }
    function extendSelectionTo(index) { Filter.extendToRow(root, index) }
    // Named escapePressed, not escape, which collides with the JS global URI function; clears an active selection first, see keys.toml.
    function escapePressed() { if (root.selection.count() > 0) { root.clearSelection(); return }; root.message("", false) }

    function applyPendingSelect() { Nav.applyPendingSelect(root) }
    function refresh(selectPath) { Nav.refresh(root, selectPath) }

    function open(newPath) {
        if (trashHost.confirming) return
        trashHost.close()
        Nav.open(root, newPath)
    }

    function openWithoutHistory(newPath) {
        if (!root.listInFlight) {
            var applied = root.appliedListingPreferences ? JSON.parse(root.appliedListingPreferences) : []
            // Search exit can enter here before the preferences timer consumes a deferred Settings change.
            if (JSON.stringify(applied[1]) !== JSON.stringify(ViewState.state.sort)) root.backend.resetSort()
            root.showHidden = ViewState.state.hidden === true
        }
        Nav.openWithoutHistory(root, newPath)
    }

    // The toggle re-lists rather than filtering client-side: the model is a row count over the
    // backend's own listing, which never held the dotfiles to begin with when they were off.
    // Re-listing also clears the cursor and selection, the same as opening any other directory.
    function toggleHidden() {
        root.showHidden = !root.showHidden
        ViewState.changeKey("hidden", root.showHidden)
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

    // Lifted to Focus.act, see ui/js/Focus.js, which routes "settings" here from the list and the rail alike.
    function act(action, menuId, paths) {
        if (trashHost.confirming) return
        if (action === "openTrash" || action === "emptyTrash" || action === "restoreAll") { trashHost.action(action); return }
        if (action === "settings") { root.settingsPanel.open(root); return }
        if (action === "permissions") { root.openPermissions(); return }
        if (["newFile", "rename", "openWith", "moveTo", "copyTo", "properties", "deletePermanently"].indexOf(action) >= 0) {
            menuActions.open(action, menuId || 0)
            return
        }
        Focus.act(action, root, menuId, paths)
    }
    function performMenu(action, menuId, paths) {
        if (action.indexOf("taildrop:") === 0) { root.sendTaildrop(action.substring("taildrop:".length), paths && paths.length === 1 ? paths[0] : ""); return }
        if (action === "sharelink") { root.copyShareLink(paths && paths.length === 1 ? paths[0] : ""); return }
        if (action === "copypath") { wire.opener.copyText(paths && paths.length ? paths[0] : root.join(root.path, root.cursorRow.n)); return }
        if (action.indexOf("col:") === 0) { ViewState.toggleColumn(action.substring("col:".length)); return }
        root.act(action, menuId, paths)
    }
    function permissionSelection() {
        var indices = Ops.targetIndices(root)
        return indices.length === 1 ? root.rowFor(indices[0]) : null
    }
    function openPermissions() {
        var row = root.permissionSelection()
        if (!row || Menu.permissionsEntry(row.p, Ops.targetIndices(root).length).disabled) {
            root.message("Permissions takes one file or folder, not a link.", true)
            return
        }
        root.permissionsRequested(root.join(root.path, row.n))
    }

    // index is a listing row, which is what every caller outside ui/js/Filter.js holds; the clamp
    // and the scroll both happen in view space, because a filter can be narrowing what is drawn.
    function setCursor(index) { Filter.setCursor(root, index) }
    // ListView.Contain has no name inside a .pragma library, so the scroll itself stays here.
    function showRow(view) { root.listArea.positionViewAtIndex(view, ListView.Contain); root.listArea.restartCoalesce() }

    // A successful pointer commit preserves the newly selected row; a refusal returns to its editor.
    function commitOpenRename() {
        var item = root.renameEditor()
        if (!item || root.renamePending) return
        root.renameKeepsPointerRow = true
        if (!item.commitEditor() || root.renameError.length > 0) root.renameKeepsPointerRow = false
    }

    // The live editor or null: a set renamingIndex is not evidence one exists, see ui/RenameField.qml.
    function renameEditor() {
        if (root.renamingIndex < 0) return null
        // The columns view draws one editor over its active column rather than one inside each row.
        if (root.viewMode === "columns")
            return root.columnsArea && root.columnsArea.activeColumn().renaming ? root.columnsArea.activeColumn() : null
        var item = root.visibleItemFor(root.renamingIndex)
        return item && item.renaming ? item : null
    }

    function openCursor() { Nav.openCursor(root, wire.opener) }

    // A path the caller already resolved, for the columns view's neighbour rows, which have no cursor.
    function openFile(path) { wire.opener.open(path) }

    // A terminal in the directory being shown, through ui/Opener.qml's flea --terminal.
    function openTerminal() { wire.opener.openTerminal(root.path) }

    function newWindow() { Quickshell.execDetached([Quickshell.env("FLEA_BIN") || "flea", root.path]) }

    function copyDirPath() { wire.opener.copyText(root.path) }

    function openParent() { if (trashHost.opened) trashHost.close(); else Nav.parent(root) }

    function join(base, name) {
        return base === "/" ? "/" + name : base + "/" + name
    }

    Flea.PaneWire {
        id: wire
        pane: root
    }

    Loader {
        id: railLoader
        anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
        width: item ? item.implicitWidth : 0
        active: !root.listOnly && root.sharedSidebar === null
        sourceComponent: Flea.Sidebar {
            backend: root.backend
            navigationPane: root.railPane
            focused: root.railPane.focusView === Focus.RAIL
            trashActive: root.railPane.trash.opened
            onOpened: function(path) { root.railPane.open(path) }
            onNetworkOpened: function(path, origin) { if (origin) origin.open(path) }
            onTrashRequested: root.railPane.trash.open()
            onMessage: function(text, isError) { root.railPane.message(text, isError) }
            onForgetMessage: function(text) { root.railPane.forgetMessage(text) }
            menu: root.railPane.contextMenu()
            onRenameFinished: root.railPane.listArea.forceActiveFocus()
        }
    }

    Rectangle {
        id: panePath
        anchors { left: railLoader.right; right: parent.right; top: parent.top }
        height: root.dualMode && !trashHost.opened ? Theme.chromeHeight : 0
        visible: height > 0
        color: root.paneFocused ? Theme.color.surface : Theme.color.background
        Text {
            anchors.fill: parent
            anchors.leftMargin: Theme.spacing.rowPaddingX
            anchors.rightMargin: Theme.spacing.rowPaddingX
            verticalAlignment: Text.AlignVCenter
            text: Nav.crumbs(root.path, root.home).map(function(c) { return c.text }).join("")
            textFormat: Text.PlainText
            color: Theme.color.foreground
            font { family: Theme.font.family; pixelSize: Theme.font.caption }
            elide: Text.ElideLeft
        }
        Rectangle {
            anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
            height: Theme.spacing.hairline
            color: Theme.color.muted
        }
        TapHandler { onDoubleTapped: root.pathBarRequested() }
    }

    PointHandler {
        id: focusPointer
        acceptedButtons: Qt.AllButtons
        onActiveChanged: if (active && focusPointer.point.position.x >= root.sidebarWidth) root.focusRequested()
    }

    Flea.Header {
        id: header
        // Only the list view has columns to head, and neither the grid board nor the columns board
        // draws one; the strip collapses rather than hiding, so the view below starts at the top of
        // the pane instead of a gap. A search takes the strip whole, in any view.
        visible: !trashHost.opened && (root.viewMode === "list" || root.searchMode.length > 0)
        height: visible ? implicitHeight : 0
        anchors.top: panePath.bottom
        anchors.left: railLoader.right
        anchors.right: parent.right
        sortBy: root.backend.sortBy
        sortDesc: root.backend.sortDesc
        dualMode: root.dualMode
        hiddenCols: root.dualMode ? ["mode", "kind"].concat(ViewState.hiddenCols) : ViewState.hiddenCols
        onSortRequested: function (key) { Sort.column(root, key) }
        onMenuRequested: function (pos) { menu.openForHeader(pos) }
        searchMode: root.searchMode
        searchQuery: root.searchQuery
        searchScope: Search.scope(Search.scopeRoot(root.path, root.home, root.searchHere), root.home)
        searchNote: Search.note(root.total, root.searchRunning, root.searchCancelled)
    }

    // The two views share the same slot, the same rows and the same cursor; only one is ever up, and
    // listArea points at whichever it is, so every caller of restartSettle stays view-agnostic and
    // shell.qml lays the empty-state overlay over the right one.
    // The list stands in for a view still being built, so no binding on listArea ever reads null.
    readonly property var listArea: root.viewMode === "grid" && gridLoader.item ? gridLoader.item
                                  : root.viewMode === "columns" && columnsLoader.item ? columnsLoader.item : list
    // How far a cursor step down moves: one row in the list, one row of tiles in the grid.
    // The columns view's own preview, exposed so a test can assert its facts without OCR; null until built.
    readonly property var columnsArea: columnsLoader.item
    // Where the view sits, for an anchor: a Loader's item is no sibling of anything here, its Loader is.
    readonly property Item listSlot: root.viewMode === "grid" ? gridLoader : root.viewMode === "columns" ? columnsLoader : list
    readonly property int cursorStride: root.viewMode === "grid" && gridLoader.item ? gridLoader.item.columns : 1

    Flea.FilterStrip {
        id: filterStrip
        anchors.top: header.bottom
        anchors.left: railLoader.right
        anchors.right: parent.right
        pane: root
    }

    // A hidden view is not a free view, see AGENTS.md rule 6: each of these two is built by its first
    // switch and kept, so a launch in the list view pays for one view's rows and marks, not three.
    property bool columnsBuilt: false
    property bool gridBuilt: false
    // setSource, not source: the view reads pane in its own bindings, so it has to hold one from birth.
    Loader {
        id: columnsLoader
        active: root.viewMode === "columns" || root.columnsBuilt
        visible: !trashHost.opened
        focus: visible && root.viewMode === "columns"
        anchors { top: filterStrip.bottom; left: railLoader.right; right: parent.right; bottom: parent.bottom }
        Component.onCompleted: setSource("ColumnsArea.qml", { pane: root, menu: menu, focus: true })
        onLoaded: { root.columnsBuilt = true; item.visible = Qt.binding(function () { return root.viewMode === "columns" }) }
    }

    Loader {
        id: gridLoader
        active: root.viewMode === "grid" || root.gridBuilt
        visible: !trashHost.opened
        focus: visible && root.viewMode === "grid"
        anchors { top: filterStrip.bottom; left: railLoader.right; right: parent.right; bottom: parent.bottom }
        Component.onCompleted: setSource("GridArea.qml", { pane: root, menu: menu })
        onLoaded: { root.gridBuilt = true; item.visible = Qt.binding(function () { return root.viewMode === "grid" }) }
    }

    // The grid's plans land the way the list's do below: it computes, and only the pane writes the two states.
    Connections {
        target: columnsLoader.item
        function onThumbsApplied(work) { root.thumbState = Thumbs.applied(root.thumbState, work) }
    }

    Connections {
        target: gridLoader.item
        function onThumbsApplied(work) { root.thumbState = Thumbs.applied(root.thumbState, work) }
        function onDirSizesApplied(ask) { root.dirSizeState = DirSizes.applied(root.dirSizeState, ask) }
        function onDirSizesCancelled() { root.dirSizeState = DirSizes.cancelled(root.dirSizeState) }
    }

    readonly property int previewIndex: root.viewMode === "columns" && columnsLoader.item ? columnsLoader.item.previewIndex : -1
    readonly property var previewColumnItem: root.viewMode === "columns" && columnsLoader.item ? columnsLoader.item.previewColumn : null
    function loadSelectionPreview() {
        if (!ViewState.previewColumn || root.dualMode) return
        if (root.viewMode === "columns" && columnsLoader.item) columnsLoader.item.loadSelection()
    }
    function togglePreviewColumn() { ViewState.changeLeaf("preview", { column: !ViewState.previewColumn }) }
    function chooseView(mode) { ViewState.changeKey("view", mode) }
    function focusPreviewColumn() {
        if (!ViewState.previewColumn || root.dualMode) return
        if (root.viewMode === "columns" && columnsLoader.item) columnsLoader.item.focusPreview()
    }
    property bool dualMode: false
    signal switchPane()

    Flea.List {
        id: list
        visible: !trashHost.opened && root.viewMode === "list"
        focus: visible
        anchors.top: filterStrip.bottom
        anchors.left: railLoader.right
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
        Keys.onPressed: function (event) { event.accepted = Focus.handleKey(event, root, root.sidebar) }
    }

    Rectangle {
        anchors { top: panePath.bottom; bottom: parent.bottom; left: railLoader.right }
        visible: root.dualMode && root.paneFocused
        width: Theme.spacing.hairline * 2
        color: Theme.color.accent
    }

    Flea.TrashHost {
        id: trashHost
        pane: root
        overlayParent: root.overlayParent
        anchors { top: parent.top; left: railLoader.right; right: parent.right; bottom: parent.bottom }
    }

    // Directory cursors retain the installed provider with its explicit file-only reason.
    readonly property var cursorRow: root.rowFor(root.cursorIndex)
    readonly property alias taildropService: wire.taildrop
    readonly property var dropboxService: root.sidebar ? root.sidebar.providerService : null

    Flea.ContextMenu {
        id: menu
        parent: root.overlayParent || root
        focusOwner: root.listArea
        showHidden: root.showHidden
        providersRefreshing: menuActions.providersRefreshing
        taildropPeers: (root.cursorRow && !root.cursorRow.d) ? wire.taildrop.peers : []
        taildropInstalled: !root.backend.providers.taildrop || root.backend.providers.taildrop.installed !== false
        taildropReason: root.cursorRow && root.cursorRow.d ? "Taildrop sends files only" : wire.taildrop.reason
        archiveFormats: root.backend.archiveFormats
        canConvert: root.backend.canConvert
        canExtract: root.cursorRow && /\.7z$/i.test(root.cursorRow.n)
            ? root.backend.extraction.sevenZip : root.backend.extraction.archive
        rowMode: root.permissionSelection() ? root.permissionSelection().p : 0
        selectionCount: Ops.targetIndices(root).length
        openWithApps: menuActions.openWithApps
        openWithLoaded: menuActions.openWithLoaded
        selectionIdentity: root.menuSelectionIdentity
        clipboardAvailable: root.clipboard.paths.length > 0
        onSnapshotRequested: menuActions.snapshot()
        onRefused: function(reason) { root.message(reason, true) }
        rowIsArchive: root.cursorRow !== null && !root.cursorRow.d && Archive.isArchive(root.cursorRow.n)
        rowIsImage: root.cursorRow !== null && root.cursorRow.i === "image-x-generic"
        dropboxInstalled: !root.backend.providers.dropbox || root.backend.providers.dropbox.installed !== false
        dropboxPath: root.dropboxService && root.dropboxService.dropboxReady ? root.dropboxService.dropboxPath : ""
        dropboxReason: root.dropboxService ? root.dropboxService.dropboxReason : "Dropbox service unavailable"
        rowInDropbox: root.dropboxService && root.cursorRow
            && Dropbox.contains(root.dropboxService.dropboxPath, root.join(root.path, root.cursorRow.n))
        onChosen: function (action) {
            menuActions.activate(action, menu.hasRow && !menu.forHeader)
        }
    }

    Flea.PaneMenuActions {
        id: menuActions
        parent: root.overlayParent || root
        pane: root
    }

    // The one ui/ContextMenu.qml this pane owns, for ui/Ipc.qml: entries, flyout and row geometry
    // are read off it directly, so a new reader costs the seam a line and this file none.
    function contextMenu() { return menu }

    Flea.EmptyState {
        id: emptyState
        // The hero belongs over the listing that is empty, which in the columns view is the active
        // column and not the whole area right of the parent. Measured on this box, spanning the
        // active column and the child slot together centred the mark at 1755 against the list
        // view's 1364: the animation jumped a third of the window on a view switch and landed on
        // the divider between the two slots. Over the active column it lands at 1363, so all three
        // views draw it in the same place and none of them draws it on a rule.
        x: root.listSlot.x + (root.viewMode === "columns" && root.columnsArea ? root.columnsArea.columnWidth : 0)
        y: root.listSlot.y
        width: root.viewMode === "columns" && root.columnsArea
               ? root.columnsArea.columnWidth : root.listSlot.width
        height: root.listSlot.height
        visible: !trashHost.opened && root.listingState === "empty"
        caption: root.searchMode === "results" ? "Nothing matches " + root.searchQuery : ""
        mark: "search"
        hint: root.searchMode === "results" ? "Press Escape to clear."
            : ViewState.keyHints ? "Press Ctrl+Shift+N for a new folder." : ""
    }
    Flea.LoadingState {
        anchors.fill: root.listSlot
        visible: !trashHost.opened && root.listingState === "loading"
    }

    function openConvert(menuId) { Ops.openConvert(root, menuId) }
    function moveToDropbox(menuId) { Ops.moveToDropbox(root, root.dropboxService && root.dropboxService.dropboxReady ? root.dropboxService.dropboxPath : "", menuId) }
    // The three foreign programs live in ui/PaneWire.qml with the backend's replies; these only name the row.
    function copyShareLink(path) {
        if (!path) { root.message("Cursor source was not validated; reopen the menu.", true); return }
        wire.shareLink.copy(path)
    }
    function sendTaildrop(peerId, path) { Ops.sendTaildrop(root, wire.taildrop, peerId, path) }

    // The keyboard's own entrance to the row menu; the placement itself is ui/js/Menu.js's.
    function openCursorMenu() { return Menu.openAtCursor(root, menu, Theme.spacing.rowPaddingX) }

    Flea.StateMessage {
        id: paneMessage
        active: !trashHost.opened
        anchors.fill: root.listSlot
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.rightMargin: Theme.spacing.rowPaddingX
        message: root.stateMessage
        listingState: root.listingState
        lockedMode: root.lockedMode
        total: root.total
    }

}
