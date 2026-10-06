import QtQuick
import Quickshell
import "." as Flea
import "js/DirSizes.js" as DirSizes
import "js/Dropbox.js" as Dropbox
import "js/ExtThumbs.js" as ExtThumbs
import "js/Filter.js" as Filter
import "js/Format.js" as Format
import "js/Focus.js" as Focus
import "js/Marks.js" as Marks
import "js/Menu.js" as Menu
import "js/Mounts.js" as Mounts
import "js/Search.js" as Search
import "js/Archive.js" as Archive
import "js/Startup.js" as Startup
import "js/Nav.js" as Nav
import "js/RecentMode.js" as RecentMode
import "js/Ops.js" as Ops
import "js/Clipboard.js" as Clipboard
import "js/Permissions.js" as Permissions
import "js/Selection.js" as Selection
import "js/Anchor.js" as Anchor
import "js/SlowClick.js" as SlowClick
import "js/Sort.js" as Sort
import "js/Tabs.js" as Tabs
import "js/Thumbs.js" as Thumbs

FocusScope {
    id: root
    focus: true
    // Unhandled child keys reach the pane, including keys from an actually focused rail.
    Keys.onPressed: function (event) { event.accepted = Focus.handleKey(event, root.sidebar && root.sidebar.activeFocus ? root.railPane : root, root.sidebar) }
    enabled: !root.settingsPanel || !root.settingsPanel.opened
    property var backend: null
    property string path: ""
    // Set once by shell.qml from FLEA_SELECT; applied to the first `rows` this pane receives, then forgotten.
    property string pendingSelect: ""
    // Set with pendingSelect by a right click on a peeked column row: the menu opens on the row once it is the cursor.
    property bool pendingMenu: false
    // Set by a right click on a neighbour column's empty space: the drawn directory opens, then its background menu opens at the stored scene point once the rows land.
    property string pendingBackground: ""
    property var pendingBackgroundAt: null
    // The directory the listing in flight asked for, which is not pane.path until the reply lands.
    property string listingPath: ""
    property int total: 0
    property int cursorIndex: 0
    // Filter and Marks bump this on every cursor move, ending a shift gesture.
    property int cursorSeq: 0
    property string listingState: "loading"
    property string stateMessage: ""
    property int lockedMode: 0
    // The folder the Locked tile names while one is drawn, "" otherwise; ui/js/Nav.js owns it,
    // and the tile's right click and the m key open that folder's own menu through it.
    readonly property string lockedTarget: Nav.lockedTarget(root)
    // Off by default: dotfiles stay out of every listing until the context menu or "." turns them on.
    property bool showHidden: ViewState.state.hidden === true
    // Issue 27's state-file key: with it on a cursor step past an end comes round; ui/js/Focus.js step is the only reader.
    readonly property bool wrapAtEnds: ViewState.state.wrapAtEnds === true
    // ClickAndRefresh under Settings, View: Escape climbs, one tap opens, and the slow click renames.
    readonly property bool escapeUp: ViewState.state.escapeUp === true
    readonly property bool singleClick: ViewState.state.openMode === "single"
    readonly property bool clickRename: ViewState.state.clickRename !== false
    // A manual reload's own count, or -1; ui/js/Reload.js sets it and ui/PaneSwap.qml spends it.
    property int reloadFrom: -1
    // Added plus removed rows on the reload's listed line, or -1; ui/PaneSwap.qml sets it from changed.
    property int reloadChanged: -1
    // The slow click's own window: the last tap's row and time, read by ui/js/SlowClick.js alone.
    property double slowClickAt: 0
    property int slowClickIndex: SlowClick.CLEARED
    // One slow-click timer for the whole pane; a tap arms it and a second tap in time opens as a double click does, and its firing renames only while the row still holds the cursor.
    Timer {
        id: slowClickTimer
        interval: Qt.styleHints.mouseDoubleClickInterval
        repeat: false
        onTriggered: SlowClick.fire(root)
    }
    function armSlowClick(index, modifiers, dragging, wasSole) {
        if (SlowClick.arm(root, index, modifiers, Date.now(), Qt.styleHints.mouseDoubleClickInterval, dragging, wasSole))
            slowClickTimer.restart()
        else slowClickTimer.stop()
    }
    function slowClickWasSole(index) { return SlowClick.wasSoleSelection(root, index) }
    // A press stops the timer without clearing the tap record, so a hold past the interval cannot fire while down;
    // the release still arms through armSlowClick, and a drag clears fully at lift.
    function pressSlowClick() { slowClickTimer.stop() }
    function cancelSlowClick() { slowClickTimer.stop(); SlowClick.cancel(root) }
    // ui/js/Tabs.js is a .pragma library and cannot reach a QML singleton, so the state it asks
    // ui/js/Startup.js about rides in through the pane, the way every other setting it reads does.
    readonly property var uiState: ViewState.state
    // When the first d of the dd pair landed; ui/js/Focus.js reads it and Nav's reset clears it.
    property double trashArmedAt: 0
    // The first row a trash request went out with, or -1: what the block left, where the cursor lands.
    property int trashedFirst: -1
    property string keySequence: ""
    property string keySequenceIdentity: ""
    // "" off, "typing" while the query line has the keyboard, "results" once a walk was asked for; ui/js/Search.js owns every transition.
    property string searchMode: ""
    // Search origin survives a home-wide walk.
    property string searchFrom: ""
    property string searchQuery: ""
    // Issue 30: which scope the next walk takes, flipped by tab on the query line; see ui/js/Search.js.
    property bool searchHere: false
    property bool searchRunning: false
    property bool searchCancelled: false
    // Recent place: "" off, "results" once answered; RecentMode.js owns transitions; pane path is history base.
    property string recentMode: ""
    // Where Recent was opened over, and the listing it hands back on leaving.
    property string recentFrom: ""
    // The history paths the listing stands on, so a refresh with the rail hidden re-asks them.
    property var recentPaths: []
    property var recentVisits: ({})
    // The sort Recent replaced, handed back on leaving; ui/js/RecentMode.js run and restoreSort write it.
    property string recentSortBy: ""
    property bool recentSortDesc: false
    // Filter.js owns the standing query and its typing state.
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
    // The window-long network host, injected by ui/WindowBody.qml (the primary pane's own,
    // shared by the second pane): mounts, bridge waits and dialog answers outlive the rail.
    property var sharedNetworkService: null
    property var networkService: null
    // Answers the one network host ui/WindowBody.qml builds with the window.
    function ensureNetworkService() {
        if (root.networkService) return root.networkService
        if (root.sharedNetworkService) { root.networkService = root.sharedNetworkService; return root.networkService }
        if (root.overlayParent) root.networkService = root.overlayParent.ensureNetworkService()
        return root.networkService
    }
    readonly property var sidebar: root.sharedSidebar || railHost.item
    readonly property real sidebarWidth: railHost.railWidth
    // RailAdditions rule 4 and directive 77, both answered in ui/PaneRail.qml.
    readonly property bool railHidden: railHost.hidden
    // What the rail takes from the pane, which an overlay never does; the sidebar case reads it.
    readonly property real railInset: railHost.inset
    // Directive 77: a withdrawn rail is still a place Tab can go, because arriving there reveals it.
    readonly property bool railAvailable: root.sidebar !== null || railHost.overlay
    function toggleRail() { ViewState.toggleRail() }
    // Ctrl+E with the rail hidden: the Sidebar is unloaded, so the key cannot read entries or
    // release through it; PaneRail spins a transient DeviceMounts for one listing instead.
    function ejectHidden() { railHost.ejectHidden() }
    // Hiding a focused rail strands the keyboard on a view that no longer exists, so the listing takes it.
    onRailHiddenChanged: if (root.railHidden) Focus.railHidden(root)
    property bool paneFocused: true
    property bool listOnly: false
    signal focusRequested()
    // Which view Tab last handed the keyboard to; the rail's own cursor is read the same way.
    property string focusView: Focus.LIST
    readonly property int railCursor: root.sidebar ? root.sidebar.cursorIndex : 0
    // So a test can wait for the rail's async FileViews instead of sleeping and guessing.
    readonly property int railCount: root.sidebar ? root.sidebar.entries.length : 0
    property var preview: null
    // WindowBody's Quick Look loader; quickLook() builds the overlay on the first open and answers it.
    property var previewLoader: null
    function quickLook() {
        if (root.previewLoader)
            root.previewLoader.active = true
        return root.preview
    }
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
    readonly property var emptyState: paneStates.emptyItem
    readonly property var stateMessageItem: paneStates.messageItem
    readonly property alias retrySelectionText: wire.retrySelectionText
    // The listing swap, ui/PaneSwap.qml: ui/js/Nav.js starts a hold through it and the views read holding.
    readonly property alias swap: wire.swap
    readonly property alias wire: wire
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
    // MenuAdditions040: the whole selection's paths for the multi-row card.
    signal permissionsBatchRequested(var paths)
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
    // The class verdict the last refresh spent, so a preview change that moves nothing for
    // this directory (zoom step, other classes) refreshes nothing; see ui/js/ExtThumbs.js.
    property string extVerdict: ExtThumbs.verdict("", ViewState.preview)
    // Same cap as thumbState, same reason; a directory row's size is only ever asked for by a viewport.
    property var dirSizeState: DirSizes.empty()
    // Input to rows is stamped inside the UI, because a harness that polls IPC across the interval times itself; see AGENTS.md "Testing".
    property real inputAt: 0
    property real rowsAt: 0

    // The cut or copied paths, absolute because a paste lands in a different directory; see ui/js/Ops.js.
    property var clipboard: Ops.emptyClipboard()
    // Shared system clipboard session, including outstanding backend replies and owner tokens.
    property var clipboardState: Clipboard.state()
    // True when this pane's clipboard watcher could not start; Paste then reads on demand.
    property bool clipboardWatchFailed: false
    // The outstanding ordered clipboard request, or null; late replies cannot publish an older choice.
    property var clipPending: null
    // Ordered choices waiting for the untagged paths reply or already resolved by a menu snapshot.
    property var clipQueue: []
    // Monotonic choice number; only the newest request can publish to the system clipboard.
    property int clipSequence: 0
    // Which asker a pending paths reply belongs to, null meaning the clipboard, which is what every
    // reply meant before compress also had to resolve a selection wider than this pane holds.
    property var pathsPending: null
    // A file drag in progress holds the watched re-read, so rows never shift under a live pointer.
    property bool dragHolding: false
    property bool awaitingPaths: false
    readonly property int anchorRowHeight: Theme.fileRowHeight
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
        root.viewMode = root.listOnly ? "list" : ViewState.view
        root.preferencesReady = true
    }
    // A filter belongs to the view it was typed in, so leaving the view takes it with it.
    onViewModeChanged: {
        Filter.close(root)
        if (root.preferencesReady && !root.listOnly && (!root.dualMode || root.viewMode !== "list") && ViewState.state.view !== root.viewMode)
            ViewState.changeKey("view", root.viewMode)
    }
    readonly property string listingPreferences: JSON.stringify([ViewState.state.hidden, ViewState.state.sort,
        ViewState.state.foldersFirst, ViewState.state.groupByKind, ViewState.state.hiddenLast, ViewState.state.rememberSort])
    property string appliedListingPreferences: ""
    // A preference re-list keeps selection and cursor by name, Finder's rule; the watcher defers
    // while a selection stands, this one preserves it instead. Cleared when its rows land.
    property var preferenceAnchor: null
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
    // A restored folder that never answers leaves "loading" for the waiting state, with Home
    // reachable through it; the late rows, if they ever land, are dropped by the swap as stale.
    readonly property int listingWaitMs: Startup.LISTING_WAIT_MS
    Timer {
        id: listingWait
        interval: root.listingWaitMs
        onTriggered: {
            if (root.listInFlight && root.listingState === "loading") {
                root.listingState = "waiting"
                root.stateMessage = "That folder is not responding."
            }
        }
    }
    onListInFlightChanged: if (!root.listInFlight) { preferences.restart(); Sort.applyPending(root); listingWait.stop() } else listingWait.restart()
    onSearchModeChanged: if (root.searchMode.length === 0) preferences.restart()
    onRecentModeChanged: if (root.recentMode.length === 0) preferences.restart()
    Timer {
        id: preferences
        interval: 0
        onTriggered: {
            var desired = root.listOnly ? "list" : ViewState.view
            if (root.viewMode !== desired) root.viewMode = desired
            if (!root.visible || !root.path || root.listInFlight || root.searchMode.length > 0
                    || root.recentMode.length > 0
                    || root.appliedListingPreferences === root.listingPreferences) return
            root.preferenceAnchor = Anchor.preference(root)
            root.openWithoutHistory(root.path, { inPlace: true })
            if (root.preferenceAnchor && root.preferenceAnchor.start > 0)
                root.backend.window(root.preferenceAnchor.start, root.windowSize)
        }
    }
    Connections {
        target: ViewState
        function onStateChanged() { preferences.restart() }
        // A class switched on in Settings decodes again the same way the menu row does.
        function onPreviewChanged() { root.refreshExtThumbs() }
    }


    // Each tab retains its own back and forward navigation.
    property var history: []
    property var forwardHistory: []
    property var tabs: null
    readonly property bool canGoBack: trashHost.opened || root.recentMode.length > 0 || root.history.length > 0
    readonly property bool canGoUp: root.path.length > 1 || root.recentMode.length > 0

    // The filesystem line the status bar draws, refreshed once per directory rather than per row.
    property string fsName: ""
    property real fsFree: 0
    // ExtThumbs: src/backend/extclass.rs's own word for the directory being shown, network,
    // phone, usb or "", carried beside the fsinfo line once per directory change, never per row.
    // storageKnown is false until that line lands, so the first settle never spends unknown as local.
    property string storageClass: ""
    property bool storageKnown: false

    // Back and up close Recent rather than travelling; forward stays put outside the history.
    function goBack() { if (trashHost.opened) trashHost.close(); else if (root.recentMode.length > 0) { if (root.listInFlight) root.message("A directory is already loading.", false); else RecentMode.close(root) } else Nav.back(root) }
    function goForward() { if (!trashHost.opened && root.recentMode.length === 0) Nav.forward(root) }

    // Rename lives in ui/js/Ops.js with the other write operations; ui/List.qml's editor commits through this.
    function commitRename(newName) { Ops.commitRename(root, newName) }
    property int renameMenuId: 0
    property string renameSource: ""
    property string renameError: ""
    property var renameRequest: null
    readonly property bool renamePending: root.renameRequest !== null
    // A sort asked for while an edit was open or a rename pending, held by
    // ui/js/Sort.js and applied when the edit ends; never dropped, see applyPending.
    property var pendingSort: null
    property var convertSource: null
    onRenamingIndexChanged: if (root.renamingIndex < 0) {
        root.renameMenuId = 0
        root.renameSource = ""
        root.renameError = ""
        // A rename that settled and an Escape that abandoned both end here, so a
        // held sort applies on either; a navigation holds listInFlight and drops it.
        Sort.applyPending(root)
    }
    // A navigation during a slow rename leaves renamingIndex at -1 with the
    // request still pending, so no index change fires when it settles.
    onRenamePendingChanged: if (!root.renamePending) Sort.applyPending(root)

    // A set of row indices over the current listing, mutated in place; selectionVersion tells a reactive binding (List.qml's delegate, StatusBar's count) to re-read it. Task 8 declined ScriptModel plus ItemSelectionModel on measured memory, see AGENTS.md "The list model".
    property var selection: Selection.create()
    property int selectionVersion: 0
    property int selectionAnchor: 0
    property var selectionBand: null
    function isSelected(index) { return root.selectionVersion >= 0 && root.selection.has(index) }
    function selectionCount() { return root.selectionVersion >= 0 ? root.selection.count() : 0 }
    function selectedIndices() { return root.selectionVersion >= 0 ? root.selection.indices() : [] }
    function toggleSelect() { Marks.toggleSelect(root) }
    function selectAll() { Marks.selectAll(root); root.selectionVersion++ }
    function clearSelection() { root.selection.clear(); root.selectionVersion++ }
    function selectOnly(index, context) {
        root.setCursor(index, context)
        root.selection.only(root.cursorIndex)
        root.selectionAnchor = root.cursorIndex
        root.selectionVersion++
    }
    function extendSelection(delta) { Marks.extend(root, delta) }
    // Ctrl+click and shift+click, the mouse's twins of v and shift+j/k; see keys.toml's [[pointer]].
    function toggleSelectAt(index) { Marks.toggleRow(root, index) }
    function extendSelectionTo(index) { Marks.extendToRow(root, index) }
    // Named escapePressed, not escape, which collides with the JS global URI function; clears an active selection first, see keys.toml.
    function escapePressed() { if (root.selection.count() > 0) { root.clearSelection(); return }; root.message("", false) }

    function applyPendingSelect() { Nav.applyPendingSelect(root); Nav.applyPendingBackground(root) }
    function openBackgroundMenu(at) { menu.openBackground(at) }
    function refresh(selectPath, keepSelection) {
        // Recent re-reads the history rather than re-listing its base, which is the root: see
        // ui/js/RecentMode.js refresh, and Nav.refresh for every other listing.
        if (root.recentMode.length > 0 && keepSelection !== true) RecentMode.refresh(root, selectPath)
        else Nav.refresh(root, selectPath, keepSelection)
    }

    function open(newPath) {
        if (trashHost.confirming) return
        trashHost.close()
        Nav.open(root, newPath)
    }

    // An eject releases the mount under path: queued reads stop first, the preview lets go,
    // and a pane showing that volume moves Home rather than standing on a vanished folder.
    function quiesceVolume(path) {
        if (root.backend)
            root.backend.quiesce()
        if (root.preview)
            root.preview.close()
        var home = root.home
        if (home.length > 0 && String(path).length > 0
                && (root.path === path || root.path.indexOf(path + "/") === 0))
            Nav.openPlace(root, home)
    }

    // options.keepHidden is the tab restore's alone: it just put back this tab's own dotfile answer, which the standing preference would overwrite.
    // A refused hop clears nothing: Nav refuses while a listing is out, and Recent's rows must survive it.
    function openWithoutHistory(newPath, options) {
        if (root.listInFlight) {
            root.message("A directory is already loading.", false)
            return
        }
        // Every real navigation leaves Recent and Trash, restoring the folder beneath each; options.inPlace, a re-list of this folder, leaves Trash up.
        if (!options || options.inPlace !== true) trashHost.close()
        RecentMode.leave(root)
        if (!root.listInFlight) {
            var applied = root.appliedListingPreferences ? JSON.parse(root.appliedListingPreferences) : []
            // Search exit can enter here before the preferences timer consumes a deferred Settings change.
            if (JSON.stringify(applied[1]) !== JSON.stringify(ViewState.state.sort)) root.backend.resetSort(newPath)
            if (!options || options.keepHidden !== true) root.showHidden = ViewState.state.hidden === true
        }
        Nav.openWithoutHistory(root, newPath, options)
    }

    // The toggle re-lists rather than filtering client-side: the model is a row count over the
    // backend's own listing, which never held the dotfiles to begin with when they were off.
    // Re-listing also clears the cursor and selection, the same as opening any other directory.
    // Recent lists what the history names, dotfiles among them, so the toggle has nothing to hide there.
    function toggleHidden() {
        if (root.recentMode.length > 0) { root.message("This listing is a history, and keeps the dotfiles it names.", false); return }
        root.showHidden = !root.showHidden
        ViewState.changeKey("hidden", root.showHidden)
        root.open(root.path)
    }

    // Where a drop on this pane lands. While a listing is out the pane's own path is still the
    // directory it is leaving, so a drop taken in that window landed in the wrong one: measured by
    // tests/drag.sh R7, where a drop on a tab whose listing was still out copied into the source.
    readonly property string dropPath: root.listInFlight && root.listingPath.length > 0
                                       ? root.listingPath : root.path

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
    function act(action, menuId, paths, context) {
        // Every dispatched action ends a pending slow click; its own rename runs after the timer cleared the record.
        cancelSlowClick()
        if (trashHost.confirming) return
        if (action === "openTrash" || action === "emptyTrash" || action === "restoreAll") { trashHost.action(action); return }
        if (action === "settings") { root.settingsPanel.open(root); return }
        if (action === "permissions") { root.openPermissions(); return }
        // Recent is a history, not a directory: a new file there would land in the root it stands on.
        if (action === "newFile" && root.recentMode.length > 0) { root.message("This listing is a history, and cannot take a new file.", false); return }
        if (["newFile", "rename", "openWith", "moveTo", "copyTo", "properties", "deletePermanently"].indexOf(action) >= 0) {
            menuActions.open(action, menuId || 0, context)
            return
        }
        Focus.act(action, root, menuId, paths)
    }
    // The menu's own dispatch lives with the rest of the menu machinery; this is the one seam the
    // rail's place menu and the dialogs still call through.
    function performMenu(action, menuId, paths) { menuActions.perform(action, menuId, paths) }
    // The sheet runs a row with no menu opened, so stand the menu on the cursor row as openAt does, or validateChoice finds nothing it opened.
    function sheetMenuAction(action) {
        menu.clearRail()
        menu.forHeader = false
        menu.hasRow = true
        menu.openedIdentity = root.menuSelectionIdentity
        menuActions.snapshot()
        menuActions.sheetRequest = menuActions.requestId
        menuActions.activate(action, true)
    }

    function permissionSelection() {
        var indices = Ops.targetIndices(root)
        return indices.length === 1 ? root.rowFor(indices[0]) : null
    }
    // MenuAdditions040 callout 10: Make executable acts on exactly one target and that target is
    // the cursor row; anything else hides the row and the action refuses.
    function isSingleCursorTarget() {
        var indices = Ops.targetIndices(root)
        return indices.length === 1 && indices[0] === root.cursorIndex
    }
    // The cursor row's two-byte shebang probe, asked of the backend once per menu open for that one file.
    property bool rowHasShebang: false
    property string shebangAsked: ""
    property int shebangId: 0
    property int makeExecPendingId: 0
    // Show original's own pending id, dropped by any navigation before its reply lands.
    property int linkTargetPendingId: 0
    // The one path a shebang answer may land for, "" unless the cursor row is a regular file without any execute bit.
    function shebangTarget() {
        var row = root.cursorRow
        if (!row || row.d) return ""
        var bits = Number(row.p) || 0
        if ((bits & Format.S_IFMT) !== Format.S_IFREG || (bits & Format.ANY_EXECUTE_BIT) !== 0) return ""
        return root.join(root.path, row.n)
    }
    function checkShebang() {
        var path = root.shebangTarget()
        if (path.length === 0) {
            root.shebangAsked = ""
            root.rowHasShebang = false
            return
        }
        root.shebangId += 1
        root.shebangAsked = path
        root.rowHasShebang = false
        root.backend.send({ c: "shebang", path: path, id: root.shebangId })
    }
    // Only the newest id on the asked path lands, so a late answer for an earlier file never arms a later one.
    function noteShebang(path, hasShebang, id) {
        if (!Permissions.landsShebang(path, id, root.shebangAsked, root.shebangId)) return
        root.rowHasShebang = hasShebang
        if (menu.opened && menu.hasRow) menu.refreshProviderRows()
    }
    // MenuAdditions040 callout 10: adds the owner execute bit through the permissions batch, so one undo restores it.
    function makeExecutable(paths) {
        if (!root.isSingleCursorTarget()) {
            root.message("Make executable needs a script without its execute bit.", false)
            return
        }
        var row = root.rowFor(root.cursorIndex)
        if (!row || !Menu.canMakeExecutable(row.p, 1, root.rowHasShebang, true)) {
            root.message("Make executable needs a script without its execute bit.", false)
            return
        }
        var taken = [root.join(root.path, row.n)]
        var oldBits = (Number(row.p) || 0) & 0o7777
        var octal = Permissions.octal((oldBits | Format.S_IXUSR) & 0o7777)
        root.makeExecPendingId += 1
        root.backend.send({ c: "permissionsBatch", paths: taken, modes: [octal], id: Permissions.MAKE_EXEC_ID + root.makeExecPendingId })
    }
    // MenuAdditions040: Permissions takes the whole selection, so the menu
    // row carries every target's mode and the dialog inspects each in turn.
    function permissionModes() {
        var out = []
        var indices = Ops.targetIndices(root)
        for (var i = 0; i < indices.length; i++) {
            var row = root.rowFor(indices[i])
            if (row) out.push(row.p)
        }
        return out
    }
    function openPermissions() {
        var idx = Ops.targetIndices(root)
        if (idx.length === 0) { Ops.sayNoTarget(root); return }
        if (root.pathsPending || root.clipPending !== null) { Ops.pathsBusy(root); return }
        // The selection can reach past the held window, so the backend
        // resolves the indices while it still can, the same rule clip follows.
        root.pathsPending = { kind: "permissions" }
        root.backend.askPaths(idx)
    }
    function openPermissionsWith(paths) {
        if (paths.length === 0) { Ops.sayNoTarget(root); return }
        if (paths.length === 1) {
            var target = root.permissionSelection()
            var single = target ? target.p : 0
            if (Menu.permissionsEntry(single, 1).disabled) {
                root.message("Permissions takes one file or folder, not a link.", true)
                return
            }
            root.permissionsRequested(paths[0])
            return
        }
        root.permissionsBatchRequested(paths)
    }
    // c and P open Copy as and Paste as on their first flyout row.
    function openCopyAs() {
        if (!root.openCursorMenu()) {
            root.message("No row under the cursor to open a menu on.", false)
            return
        }
        menu.openSubmenuFor("copyAs")
    }
    function openPasteAs() {
        if (!root.openCursorMenu())
            menu[root.cursorRow ? "openAt" : "openBackground"](root.listSlot.mapToItem(null, root.listSlot.width / 2, root.listSlot.height / 2))
        if (!menu.clipboardAvailable) {
            menu.close()
            root.message("There is nothing to paste; y copies and x cuts.", false)
            return
        }
        menu.openSubmenuFor("pasteAs")
    }
    // MenuAdditions040: V flips the marks over the rows the listing draws;
    // the filter applies, and a close match is never selected.
    function invertSelection() { Marks.invert(root) }
    // MenuAdditions040: Show original reveals a symlink's target in its own
    // folder, the same path Show in folder uses.
    function showOriginal() {
        var row = root.rowFor(root.cursorIndex)
        if (!row || !Format.isSymlink(row.p)) {
            root.message("Show original needs the cursor on a symlink.", false)
            return
        }
        root.requestLinkTarget(root.join(root.path, row.n))
    }
    // Both entrances send through here, so one pending id covers the cursor and the menu.
    function requestLinkTarget(path) {
        root.linkTargetPendingId = root.backend.nextLinkTargetId()
        root.backend.send({ c: "linktarget", path: path, id: root.linkTargetPendingId })
    }
    // Paste as links reads the file clipboard and asks through the collision card.
    function pasteLink(kind) {
        if (RecentMode.refusePaste(root)) return
        Ops.pasteLink(root, kind)
    }

    // index is a listing row, which is what every caller outside ui/js/Filter.js holds; the clamp
    // and the scroll both happen in view space, because a filter can be narrowing what is drawn.
    // A click carries context 0; a keyboard move keeps the default 3.
    function setCursor(index, context) { Filter.setCursor(root, index, context) }
    // The cursor keeps three rows of context above and below: the list and the columns view hold
    // it through their own showCursor, and the grid keeps Contain, whose tiles are not rows.
    // ListView.Contain has no name inside a .pragma library, so the scroll itself stays here.
    function showRow(view, context) {
        if (root.viewMode === "columns" && root.columnsArea) root.columnsArea.activeColumn().showCursor(view, context)
        else if (root.viewMode === "list") list.showCursor(view, context)
        else root.listArea.positionViewAtIndex(view, ListView.Contain)
        root.listArea.restartCoalesce()
    }

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
        var item = root.viewMode === "grid" ? (root.listArea ? root.listArea.currentRenameTile(root) : null) : root.visibleItemFor(root.renamingIndex)
        return item && item.renaming ? item : null
    }

    function openCursor() { Nav.openCursor(root, wire.opener) }

    // A path the caller already resolved, for the columns view's neighbour rows, which have no cursor.
    function openFile(path) { wire.opener.open(path) }

    // A terminal in the caller's place or the directory being shown, through flea --terminal.
    function openTerminal(path) { wire.opener.openTerminal(path || root.path) }

    function newWindow() { Quickshell.execDetached(Tabs.freshLaunch([Quickshell.env("FLEA_BIN") || "flea", root.path])) }

    // Quoted when it holds whitespace, because this one is pasted into a shell: see ui/js/Format.js.
    function copyDirPath() { wire.opener.copyText(Format.shellQuoted(root.path)) }

    function openParent() { if (trashHost.opened) trashHost.close(); else if (root.recentMode.length > 0) { if (root.listInFlight) root.message("A directory is already loading.", false); else RecentMode.close(root) } else Nav.parent(root) }

    // The sort Recent replaced, handed back on leaving; ui/js/Search.js reaches it through here.
    function restoreRecentSort() { RecentMode.restoreSort(root) }
    // The rail's Recent row answers with the history's paths, read bounded the way the
    // path jump reads them; a second open while one lands replaces it, the way a navigation does.
    function openRecent(paths, visits) { RecentMode.run(root, paths, visits) }
    function closeRecent() { RecentMode.close(root) }

    function join(base, name) {
        return base === "/" ? "/" + name : base + "/" + name
    }

    Flea.PaneWire {
        id: wire
        pane: root
    }

    Flea.PaneRail {
        id: railHost
        anchors.left: parent.left
        // Over the listing, because with auto-hide on the rail is an overlay and not a column.
        z: 3
        pane: root
        service: root.networkService
    }

    Flea.PanePath {
        id: panePath
        anchors { left: railHost.right; right: parent.right; top: parent.top }
        height: root.dualMode && !trashHost.opened ? Theme.chromeHeight : 0
        visible: height > 0
        // A history is a location and not a directory, so the strip says its own name.
        path: root.recentMode.length > 0 ? "Recent" : root.path
        home: root.home
        focused: root.paneFocused
        inputLive: !(root.preview && root.preview.active)
        onChosen: function (path) { root.open(path) }
        onEditRequested: root.pathBarRequested()
    }

    PointHandler {
        id: focusPointer
        acceptedButtons: Qt.AllButtons
        onActiveChanged: if (active) railHost.focusPress(focusPointer.point.position)
    }

    Flea.Header {
        id: header
        // Only the list view has columns to head, and neither the grid board nor the columns board
        // draws one; the strip collapses rather than hiding, so the view below starts at the top of
        // the pane instead of a gap. A search takes the strip whole, in any view.
        visible: !trashHost.opened && (root.viewMode === "list" || root.searchMode.length > 0)
        height: visible ? implicitHeight : 0
        anchors.top: panePath.bottom
        anchors.left: railHost.right
        anchors.right: parent.right
        // The held rows a double click fits and F4 fits whole; ListColumns040 never scans the directory.
        pane: root
        viewMode: root.viewMode
        sortBy: root.backend.sortBy
        sortDesc: root.backend.sortDesc
        dualMode: root.dualMode
        recent: root.recentMode.length > 0
        hiddenCols: root.dualMode ? ["mode", "kind"].concat(ViewState.hiddenCols) : ViewState.hiddenCols
        onSortRequested: function (key) { Sort.column(root, key) }
        onMenuRequested: function (pos) { menu.openForHeader(pos) }
        searchMode: root.searchMode
        searchQuery: root.searchQuery
        searchScope: Search.scope(Search.scopeRoot(root.path, root.home, root.searchHere), root.home)
        searchNote: Search.note(root.total, root.searchRunning, root.searchCancelled)
        searchWayOut: Search.wayOut(root.searchRunning)
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
    readonly property bool dragActive: root.dragHolding || !!(root.listArea && root.listArea.fileDragActive === true)
    // Where the view sits, for an anchor: a Loader's item is no sibling of anything here, its Loader is.
    readonly property Item listSlot: root.viewMode === "grid" ? gridLoader : root.viewMode === "columns" ? columnsLoader : list
    readonly property int cursorStride: root.viewMode === "grid" && gridLoader.item ? gridLoader.item.columns : 1

    Flea.FilterStrip {
        id: filterStrip
        anchors.top: header.bottom
        anchors.left: railHost.right
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
        anchors { top: filterStrip.bottom; left: railHost.right; right: parent.right; bottom: parent.bottom }
        Component.onCompleted: setSource("ColumnsArea.qml", { pane: root, menu: menu, focus: true })
        onLoaded: { root.columnsBuilt = true; item.visible = Qt.binding(function () { return root.viewMode === "columns" }) }
    }

    Loader {
        id: gridLoader
        active: root.viewMode === "grid" || root.gridBuilt
        visible: !trashHost.opened
        focus: visible && root.viewMode === "grid"
        anchors { top: filterStrip.bottom; left: railHost.right; right: parent.right; bottom: parent.bottom }
        Component.onCompleted: setSource("GridArea.qml", { pane: root, menu: menu })
        onLoaded: { root.gridBuilt = true; item.visible = Qt.binding(function () { return root.viewMode === "grid" }) }
    }

    // The grid's plans land the way the list's do below: it computes, and only the pane writes the two states.
    Connections {
        target: columnsLoader.item
        function onThumbsApplied(work) { root.thumbState = Thumbs.applied(root.thumbState, work) }
        function onDirSizesApplied(ask) { root.dirSizeState = DirSizes.applied(root.dirSizeState, ask) }
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
    // ExtThumbs: the background menu's class row writes the class setting, never "this drive".
    // A class switched on decodes again: its cache-only misses leave and the viewport re-asks.
    function toggleExtThumbs() {
        var key = ExtThumbs.keyForClass(root.storageClass)
        if (key === "") return
        var on = !ExtThumbs.classOn(root.storageClass, ViewState.preview)
        // No direct refresh: the setting change below routes through onPreviewChanged,
        // which refreshes once on the verdict edge instead of twice.
        ViewState.changeSetting("preview." + key, on)
        root.message(ExtThumbs.statusLine(root.storageClass, on), false)
    }
    function refreshExtThumbs() {
        var now = ExtThumbs.verdict(root.storageClass, ViewState.preview)
        if (now === root.extVerdict) return
        root.extVerdict = now
        root.thumbState = ExtThumbs.forgetMisses(root.thumbState)
        if (root.listArea) root.listArea.restartSettle()
        // The preview follows rather than loads, so a class just switched off holds instead of decoding.
        if (root.viewMode === "columns" && columnsLoader.item) columnsLoader.item.askMeta()
    }
    function chooseView(mode) { ViewState.changeKey("view", mode) }
    function focusPreviewColumn() {
        if (!ViewState.previewColumn || root.dualMode) return
        if (root.viewMode === "columns" && columnsLoader.item) columnsLoader.item.focusPreview()
    }
    property bool dualMode: false
    readonly property alias pathCrumbs: panePath.crumbItems
    readonly property alias pathSlot: panePath.crumbSlot
    readonly property alias pathStrip: panePath
    signal switchPane()

    Flea.List {
        id: list
        visible: !trashHost.opened && root.viewMode === "list"
        focus: visible
        anchors.top: filterStrip.bottom
        anchors.left: railHost.right
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        pane: root
        menu: menu

        // List only computes the clamp and the thumbnail plan; cursorIndex and thumbState are Pane's own to write.
        onCursorClamped: function (first, last) { Filter.clampCursor(root, first, last) }
        onThumbsApplied: function (work) { root.thumbState = Thumbs.applied(root.thumbState, work) }
        onDirSizesApplied: function (ask) { root.dirSizeState = DirSizes.applied(root.dirSizeState, ask) }
        onDirSizesCancelled: root.dirSizeState = DirSizes.cancelled(root.dirSizeState)
    }

    Rectangle {
        anchors { top: panePath.bottom; bottom: parent.bottom; left: railHost.right }
        visible: root.dualMode && root.paneFocused
        width: Theme.spacing.hairline * 2
        color: Theme.color.accent
    }

    Flea.TrashHost {
        id: trashHost
        pane: root
        overlayParent: root.overlayParent
        anchors { top: parent.top; left: railHost.right; right: parent.right; bottom: parent.bottom }
    }

    // Directory cursors retain the installed provider with its explicit file-only reason.
    readonly property var cursorRow: root.rowFor(root.cursorIndex)
    readonly property alias taildropService: wire.taildrop
    readonly property alias opener: wire.opener
    readonly property var dropboxService: root.networkService
    // The keymap sheet's query asks the menu's own rows, so the values stay live for as long as the sheet is open.
    readonly property bool sheetReadsMenu: root.keymapSheet !== null && root.keymapSheet.opened === true
    readonly property bool menuValuesLive: menu.preparing || menu.opened || root.sheetReadsMenu
    readonly property var menuPermissionRow: root.menuValuesLive ? root.permissionSelection() : null

    Flea.ContextMenu {
        id: menu
        parent: root.overlayParent || root
        focusOwner: root.listArea
        showHidden: root.showHidden
        providersRefreshing: menuActions.providersRefreshing
        taildropPeers: wire.taildrop.peers
        rowIsFile: root.cursorRow !== null && !root.cursorRow.d
        taildropInstalled: !root.backend.providers.taildrop || root.backend.providers.taildrop.installed !== false
        taildropReason: root.cursorRow && root.cursorRow.d ? "Taildrop sends files only" : wire.taildrop.reason
        archiveFormats: root.backend.archiveFormats
        canConvert: root.backend.canConvert
        canExtract: root.menuValuesLive && root.cursorRow !== null
            && Archive.canExtract(root.cursorRow.n, root.backend.extraction)
        rowMode: root.menuPermissionRow ? root.menuPermissionRow.p : 0
        selectionCount: root.menuValuesLive ? Ops.targetIndices(root).length : 0
        cursorIsTarget: root.menuValuesLive && root.isSingleCursorTarget()
        openWithApps: menuActions.openWithApps
        openWithLoaded: menuActions.openWithLoaded
        selectionIdentity: root.menuSelectionIdentity
        clipboardAvailable: root.clipboard.paths.length > 0
        clipboardWatchFailed: root.clipboardWatchFailed
        // Scripts, send peers and the shebang are requested only when a row menu opens.
        onSnapshotRequested: { menuActions.snapshot(); Flea.Scripts.refresh(); menuActions.localSend.refresh(menu.localSend.installed); root.checkShebang() }
        onRefused: function(reason) { root.message(reason, true) }
        rowIsArchive: root.menuValuesLive && root.cursorRow !== null && !root.cursorRow.d && Archive.isArchive(root.cursorRow.n)
        rowIsImage: root.cursorRow !== null && root.cursorRow.i === "image-x-generic"
        // MenuAdditions040: Show original is visible but only on a symlink.
        rowIsSymlink: root.cursorRow !== null && Format.isSymlink(root.cursorRow.p)
        rowHasShebang: root.rowHasShebang
        // Permissions carries every target's mode only while the menu needs it.
        selectionModes: root.menuValuesLive ? root.permissionModes() : []
        dropboxInstalled: !root.backend.providers.dropbox || root.backend.providers.dropbox.installed !== false
        localSend: ({ installed: (root.backend.providers.localsend || {}).installed === true, checking: menuActions.localSend.checking,
                      peers: (root.cursorRow && !root.cursorRow.d) ? menuActions.localSend.peers : [], answeredOnce: menuActions.localSend.answeredOnce })
        dropboxPath: root.dropboxService && root.dropboxService.dropboxReady ? root.dropboxService.dropboxPath : ""
        dropboxReason: root.dropboxService ? root.dropboxService.dropboxReason : "Dropbox service unavailable"
        rowInDropbox: root.menuValuesLive && root.dropboxService && root.cursorRow
            && Dropbox.contains(root.dropboxService.dropboxPath, root.join(root.path, root.cursorRow.n))
        // Issue 133: no GVFS mount, share or phone, has a trash of its own, so the row is not offered there.
        // A read-only folder has none either: the listed line's own writability rides along.
        canTrash: Mounts.trashable(root.path, root.backend ? root.backend.dirWritable !== false : true)
        // The listing's own w flag turns the menu's write rows off in a read-only folder.
        dirWritable: root.backend ? root.backend.dirWritable !== false : true
        // vfat and exfat hold no links, so the Paste as rows are not offered there either.
        canLink: root.fsName !== "vfat" && root.fsName !== "exfat"
        // ExtThumbs: the class the background menu's thumbnail row is present for, "" locally.
        storageClass: root.storageClass
        // Issue 179: the Sort by flyout offers its forget row only where this folder has its own sort.
        hasFolderSort: root.menuValuesLive && root.searchMode.length === 0 && root.backend ? root.backend.folderHasSort(root.path) : false
        // The Locked tile's folder and mode while one is drawn; ui/ContextMenu.qml routes a
        // background right click to that folder's own menu through them.
        tileTarget: root.lockedTarget
        tileMode: root.lockedMode
        onChosen: function (action) {
            // Issue #170: Rename opens the editor at once by the route F2 takes
            // rather than queueing an activate behind the cold Open-with catalogue.
            if (action === "rename") { menuActions.openRenameFromMenu(); return }
            menuActions.activate(action, menu.hasRow && !menu.forHeader)
        }
        // The Locked tile's folder arrives with the row: close() cleared the menu's own copy
        // before this runs, so reading menu.lockedPath here would always see "" and every row
        // would fall into the background dispatch above.
        onLockedChosen: function (action, path) { root.performLocked(path, action) }
    }

    Flea.PaneMenuActions {
        id: menuActions
        parent: root.overlayParent || root
        pane: root
    }

    // Every paste and drop asks through here first, see ui/js/Collide.js.
    Flea.CollideHost { id: collideHost; parent: root.overlayParent || root; pane: root }
    readonly property alias collide: collideHost

    // The one ui/ContextMenu.qml this pane owns, for ui/Ipc.qml: entries, flyout and row geometry
    // are read off it directly, so a new reader costs the seam a line and this file none.
    function contextMenu() { return menu }

    Flea.PaneStates {
        id: paneStates
        pane: root
        trashOpen: trashHost.opened
    }

    function openConvert(menuId) { Ops.openConvert(root, menuId) }
    function moveToDropbox(menuId) { Ops.moveToDropbox(root, root.dropboxService && root.dropboxService.dropboxReady ? root.dropboxService.dropboxPath : "", menuId) }
    // The three foreign programs live in ui/PaneWire.qml with the backend's replies; these only name the row.
    function copyShareLink(path) {
        if (!path) { root.message("Cursor source was not validated; reopen the menu.", true); return }
        wire.shareLink.copy(path)
    }
    function sendTaildrop(peerId, path) { Ops.sendTaildrop(root, wire.taildrop, peerId, path) }

    // The Locked tile's own dispatch: every row its menu offers acts on the locked folder
    // alone, by path, never on the cursor, the selection or the parent standing behind it.
    // The menu row is already disabled where it cannot act, so no row here can fail refused.
    function performLocked(path, action) {
        if (path.length === 0) return
        if (action === "openTerminal") { wire.opener.openTerminal(path); return }
        if (action === "copypath") { wire.opener.copyText(path); return }
        // MenuAdditions040: the Locked tile offers Copy as over the one folder
        // it names, each leaf copying that path in its own form.
        if (action.indexOf("copyAs:") === 0) {
            var leaf = action.substring("copyAs:".length)
            var kinds = { copyPath: "path", copyName: "name", copyStem: "stem",
                copydirpath: "dirpath", copyUri: "uri", copyQuoted: "quoted" }
            if (kinds[leaf] !== undefined) { Ops.copyAs(root, kinds[leaf], [path]); return }
        }
        if (action === "copyAs") { Ops.copyAs(root, "path", [path]); return }
        if (action === "permissions") { root.permissionsRequested(path); return }
        root.message(action + " is not built yet.", false)
    }

    // The keyboard's own entrance to the row menu; the placement itself is ui/js/Menu.js's.
    function openCursorMenu() {
        if (root.lockedTarget.length > 0) {
            var at = root.listSlot.mapToItem(null, root.listSlot.width / 2, root.listSlot.height / 2)
            menu.openLocked(root.lockedTarget, root.lockedMode, at)
            return true
        }
        return Menu.openAtCursor(root, menu, Theme.spacing.rowPaddingX)
    }

}
