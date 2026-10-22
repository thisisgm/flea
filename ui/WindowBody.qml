import Quickshell
import QtQuick
import qs.Commons
import "."
import "." as Flea
import "js/Focus.js" as Focus
import "js/MarkdownPrepared.js" as Prepared
import "js/TextSize.js" as TextSize
import "js/MouseNav.js" as MouseNav
import "js/Nav.js" as Nav
import "js/Ops.js" as Ops
import "js/RailKeys.js" as RailKeys
import "js/RailMenu.js" as RailMenu
import "js/Search.js" as Search
import "js/Startup.js" as Startup
import "js/Tabs.js" as Tabs

// Everything inside the window ui/boot/shell.qml maps, arriving by file: URL on the first frame
// because that is the only way Qt caches it; see AGENTS.md "The first window".
Rectangle {
    id: view
    anchors.fill: parent
    // On glass each plane draws its own ground, so this one would put a second plane under every pixel.
    color: Glass.on ? "transparent" : Theme.color.background
    // The window this body was loaded into. Named host, not fleaWindow: a root property of
    // that name would bind to itself through Ipc and read null.
    property var host
    readonly property bool dualMode: ViewState.state.view === "dual"
    property int focusSide: (ViewState.state.dual || {}).focus === 1 ? 1 : 0
    // dual.split: the left listing's share of the room right of the rail in ten-thousandths, 5000 the equal split.
    readonly property int dualSplitScale: 10000
    readonly property int dualEqualSplit: 5000
    readonly property real dualRoom: view.width - primaryPane.railInset - Theme.spacing.hairline
    readonly property real dualSplit: typeof (ViewState.state.dual || {}).split === "number" ? ViewState.state.dual.split : view.dualEqualSplit
    readonly property real dualShare: view.dualShareOf(view.dualSplit)
    // False when the room fits no two narrowest listings: nothing moves there, so a drag writes nothing.
    readonly property bool dualFits: Theme.dualMinWidth / view.dualRoom <= 0.5
    // Held between the narrowest listing and its mirror, equal when the room fits no two; the stored split is never rewritten by it.
    function dualShareOf(split) {
        var low = Theme.dualMinWidth / view.dualRoom
        return low > 0.5 ? 0.5 : Math.min(1 - low, Math.max(low, split / view.dualSplitScale))
    }
    // The run card, null until the first ask builds it for the window and keeps it.
    property var runCard: null
    function runLane() {
        if (!view.runCard) view.runCard = Qt.createComponent("RunCard.qml").createObject(view, { z: 2 })
        return view.runCard
    }
    function askRunCard(pane, info) { view.runLane().open(info, pane) }
    // Null until the first Quick Look opens, see previewLoader below.
    readonly property var quickLook: previewLoader.item
    readonly property bool quickLookActive: view.quickLook !== null && view.quickLook.active
    // QuickLookWarm, built by the cursor's first landing on a file and kept: a move off it must not drop the compiled units. A Markdown file builds it in that turn, any other file off the key once a key has moved the cursor, so a launch builds none.
    readonly property var cursorFile: view.currentPane.cursorRow
    onCursorFileChanged: {
        var markdown = Prepared.isMarkdownRow(view.cursorFile)
        if (warmLoader.active || !view.cursorFile || view.cursorFile.d || !(markdown || view.currentPane.inputAt > 0)) return
        warmLoader.asynchronous = !markdown; warmLoader.active = true
    }
    Loader { id: warmLoader; active: false; source: "QuickLookWarm.qml"; onLoaded: item.pane = Qt.binding(function () { return view.currentPane }) }
    readonly property var currentPane: dualMode && focusSide === 1 && secondPane.item ? secondPane.item.pane : primaryPane
    property bool initialized: false
    property bool closing: false
    // Built once per window on first use, so the flasher stays out of the first frame.
    property var flashHost: null
    function openFlash(pane, image, device) {
        if (!view.flashHost)
            view.flashHost = Qt.createComponent("FlashHost.qml").createObject(view, { z: 2 })
        view.flashHost.openFor(pane, image, device)
    }
    function closeFlash() {
        if (view.flashHost)
            view.flashHost.close()
    }
    property var drained: [false, false]

    function focusPane(side) {
        if (!dualMode) return
        focusSide = side
        // The pane left behind gets its list back, so the ring resumes from the list and never from a rail, bar or strip stop the user left (Shift+Tab from the right lands on the left list, not the rail).
        if (side === 1) primaryPane.focusView = Focus.LIST
        else if (secondPane.item) secondPane.item.pane.focusView = Focus.LIST
        currentPane.forceActiveFocus()
        Qt.callLater(view.rememberDual)
    }
    function switchToLeftRail() { focusPane(0); primaryPane.focusView = Focus.RAIL }
    function rememberPaths() {
        // Path and tab changes in one turn share the deferred navigation patch.
        if (initialized && !dualMode && primaryPane.path)
            view.queueTabStrip()
        if (!initialized || !dualMode || !secondPane.item || !primaryPane.path || !secondPane.item.pane.path) return
        Qt.callLater(view.rememberDual)
    }
    // Strip changes coalesce; an in-flight navigation waits for its path and rows to land before saving.
    property bool tabStripQueued: false
    // Stage trace, on only with FLEA_TRACE_TABDRAG=1; read once, silent otherwise.
    readonly property bool tabTrace: Quickshell.env("FLEA_TRACE_TABDRAG") === "1"
    function traceTab(stage, detail) { if (view.tabTrace) console.log("TABDRAG " + stage + " pid=" + Quickshell.processId + " " + detail) }
    function queueTabStrip() {
        if (view.tabStripQueued)
            return
        view.tabStripQueued = true
        Qt.callLater(function() { view.tabStripQueued = false; view.rememberTabStrip() })
    }
    function rememberTabStrip() {
        if (initialized && !dualMode && primaryPane.path && !primaryPane.listInFlight)
            ViewState.rememberNavigation(primaryPane.path, Tabs.remembered(primaryPane))
    }
    // The mode binding reads ViewState.state; its handlers must finish before persistence replaces it.
    function rememberDual() {
        if (!initialized || !dualMode) return
        var saved = {focus: focusSide}
        if (secondPane.item && primaryPane.path && secondPane.item.pane.path)
            saved.paths = [primaryPane.path, secondPane.item.pane.path]
        ViewState.changeLeaf("dual", saved)
    }
    function quitBackends() {
        if (closing) return
        closing = true
        drained = [false, secondPane.item === null]
        backend.quit()
        if (secondPane.item) secondPane.item.backend.quit()
    }
    function backendDrained(side) {
        var next = drained.slice()
        next[side] = true
        drained = next
        // Finish quitReady delivery before SIGTERM can interrupt another observer.
        if (closing && drained[0] && drained[1]) Qt.callLater(view.finishQuit)
    }
    function finishQuit() {
        Quickshell.execDetached(["kill", String(Quickshell.processId)])
    }
    onDualModeChanged: {
        if (!initialized) return
        if (!dualMode) { focusSide = 0; primaryPane.forceActiveFocus() }
        else if (initialized) { focusPane(focusSide); rememberPaths() }
    }


    // Issue 9's chords, aliased by keys.toml onto the Display section's own text size. The
    // panel writes ui/ViewState.qml directly and shows the value in the row; a chord has no
    // readout of its own with the panel shut, so this one adds the status line.
    function applyTextSize(direction) {
        if (direction === 0)
            ViewState.followTextSize()
        else
            ViewState.stepTextSize(direction)
        view.currentPane.message(TextSize.announce(ViewState.textSize, ViewState.omarchyBase), false)
    }

    // The backend is told first and answers when it has drained, so closing never leaves a half
    // file; the last-window handler itself is in ui/boot/shell.qml, which always exists.
    Connections { target: backend; function onQuitReady() { view.backendDrained(0) } }

    Backend {
        id: backend
        preserveSort: view.dualMode
    }

    // Built with the window and outliving the rail, so hiding the rail mid-mount kills no wait.
    Flea.NetworkMounts {
        id: networkHost
        backend: primaryPane.backend
        origin: primaryPane
        savedFavourites: Favourites.records
    }
    readonly property var networkService: networkHost
    function ensureNetworkService() {
        return networkHost
    }

    // The canvas's own top chrome: where you are on the left, how you are looking at it on
    // the right. The path lives here, which is why the status bar below carries counts instead.
    Flea.ChromeBar {
        id: chrome
        inputLive: !view.quickLookActive
        visible: view.dualMode || (!view.currentPane.trash.opened && !view.currentPane.flashActive)
        height: visible ? Theme.chromeHeight : 0
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        // A history is a location and not a directory, so the chrome says its own name.
        path: view.currentPane.flashActive ? "Flash image"
            : view.currentPane.trash.opened ? "Trash"
            : view.currentPane.recentMode.length > 0 ? "Recent"
            : view.currentPane.path
        // True only when the drawn path's listing failed, so the bar retries it and nothing else.
        pathFailed: Nav.pathFailed(view.currentPane)
        home: view.currentPane.home
        pathAlias: view.currentPane.pathAlias
        dropPane: view.currentPane
        stopFocused: view.currentPane.focusView === Focus.CHROME
        canGoBack: view.currentPane.canGoBack
        canGoUp: view.currentPane.canGoUp
        viewMode: view.dualMode ? "dual" : view.currentPane.viewMode
        showPath: !view.dualMode
        showHidden: view.currentPane.showHidden
        canFilter: view.currentPane.searchMode.length === 0
        canSort: view.currentPane.searchMode.length === 0
        onBackRequested: view.currentPane.goBack()
        onUpRequested: view.currentPane.openParent()
        onSearchRequested: view.currentPane.act("search")
        onFilterRequested: view.currentPane.act("filter")
        onSortRequested: view.currentPane.act("sortNext")
        // The primary pane is the one the window keeps after Dual, so every view choice, the secondary pane's keys included, lands there.
        onViewChosen: function (mode) { primaryPane.chooseView(mode) }
        // The path bar's four. The primaryPane navigates and answers for the keyboard exactly as it
        // does for every other route in, so a path typed and a row opened end the same way.
        // Issue 194: a typed smb:// (or other network) address mounts through the window-long
        // host's own open-a-share path, which lists a server's shares, rather than listing a
        // local path. A network address is never listed as a folder: with no host it is refused
        // with a sentence, the way 0.3.5 refused the line.
        onPathEntered: function (path) {
            if (/^(smb|sftp|ftp|ftps|dav|davs|nfs|afp):\/\//i.test(path)) {
                var target = view.currentPane
                var service = target ? target.ensureNetworkService() : null
                if (service) service.openShare(path, false, "", false, { origin: target })
                else target.message("That network address cannot be opened here.", true)
                return
            }
            view.currentPane.open(path)
        }
        onEditClosed: view.currentPane.forceActiveFocus()
        // Tab peeks in name order with dotfiles first; the columns view asked nothing, so it drops the reply.
        onCompleteRequested: function (dir, hidden) { view.currentPane.backend.peek(dir, view.currentPane.windowSize, hidden, false) }
        onJumpRequested: function (id, ranking, favourites, recent) { view.currentPane.backend.jump(id, ranking, favourites, recent) }
        onSaid: function (text) { bar.say(text, false) }
        onSettingsRequested: settingsPanel.open(view.currentPane)
    }

    // The peek behind Tab. Every peeked line carries the directory and the hidden flag it
    // answers for, so the bar takes the reply to its own request and the columns view, which
    // peeks the same wire for the primaryPane's ancestors, goes on taking its own.
    Connections {
        target: view.currentPane.backend
        function onPeeked(path, hidden, total, rows, readFailed, mode) { chrome.completeWith(path, hidden, rows) }
        // A bar never opened sent no ask, so an answer with no jump is dropped like a stale one.
        function onJumped(id, favourites, zoxide, recent, frecency) { if (chrome.jump !== null) chrome.jump.take(id, favourites, zoxide, recent, frecency) }
    }

    Flea.TabBar {
        id: tabBar
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: chrome.bottom
        pane: view.currentPane
        stopFocused: view.currentPane.focusView === Focus.STRIP
    }

    // Load the acknowledgment handler from the boot URL to preserve startup without ui/qmldir.
    Loader {
        id: tabAck
        active: true
        source: "file://" + Quickshell.shellDir + "/fleatab.qml"
        onLoaded: {
            item.tabBar = tabBar
            item.view = view
            item.tabs = Tabs
            item.panes = Qt.binding(function () { return [primaryPane, secondPane.item ? secondPane.item.pane : null] })
        }
    }

    // A hidden lone-tab strip still needs a pointer-transparent receiver with the strip's validation.
    DropArea {
        anchors.fill: parent
        keys: [Tabs.TAB_MIME]
        enabled: !view.dualMode && !(view.currentPane.tabs && view.currentPane.tabs.items
            && view.currentPane.tabs.items.length > 1)
        onEntered: function (drag) {
            var ok = Tabs.enterAccepts(drag.formats, drag.getDataAsString(Tabs.TAB_MIME), undefined, Tabs.canReceive(view.currentPane), false)
            view.traceTab("enter-window", "formats=" + String(drag.formats) + " ok=" + ok)
            if (!ok)
                drag.accepted = false
        }
        onExited: view.traceTab("leave-window", "")
        onDropped: function (drop) {
            var payload = drop.getDataAsString(Tabs.TAB_MIME)
            view.traceTab("drop-window", "empty=" + (payload.length === 0) + " len=" + payload.length)
            var info = Tabs.parseTabMime(payload)
            if (!info) {
                view.traceTab("drop-skip", "reason=window-bad-payload")
                return
            }
            if (Tabs.isOwnTab(info)) {
                view.traceTab("drop-skip", "reason=window-own-tab")
                return
            }
            // The take decision answers Move at once; the peek behind it may still refuse, and then no ack goes out.
            if (Tabs.dropDecision(info, undefined, false, Tabs.canReceive(view.currentPane)) !== Tabs.DROP_TAKE) {
                view.traceTab("drop-skip", "reason=window-decision-ignore canReceive=" + Tabs.canReceive(view.currentPane))
                return
            }
            drop.accept(Qt.MoveAction)
            tabBar.acceptTabDrop(payload, info, -1)
        }
    }

    Flea.Pane {
        id: primaryPane
        anchors.left: parent.left
        width: view.dualMode ? railInset + (view.width - railInset - Theme.spacing.hairline) * view.dualShare : view.width
        anchors.top: tabBar.bottom
        anchors.bottom: bar.top
        backend: backend
        networkService: view.networkService
        dualMode: view.dualMode
        paneFocused: view.currentPane === primaryPane
        railPane: view.currentPane
        chromeBar: chrome
        tabStrip: tabBar
        onFocusRequested: view.focusPane(0)
        onSwitchPane: view.focusPane(1)
        onSwitchToRail: view.switchToLeftRail()
        onPathChanged: view.rememberPaths()
        onListingStateChanged: if (primaryPane.listingState !== "loading") ipcLoader.active = true
        onListInFlightChanged: { if (!primaryPane.listInFlight) view.queueTabStrip() }
        onTabsChanged: view.queueTabStrip()
        onClipboardChanged: if (secondPane.item && secondPane.item.pane.clipboard !== clipboard) secondPane.item.pane.clipboard = clipboard
        overlayParent: view
        preview: view.quickLook
        previewLoader: previewLoader
        shareBrowser: shareBrowser
        keymapSheet: keymapSheet
        settingsPanel: settingsPanel
        statusBar: bar
        onMessage: function (text, isError) { bar.say(text, isError) }
        onArmPrompted: function (text) { bar.sayArm(text) }
        onForgetMessage: function (text) { bar.forget(text) }
        onOperationResult: function (headline, detail, isError) { bar.say(headline, isError, detail) }
        // A running operation's line, which stands until the operation replaces it; see ui/StatusBar.qml.
        onSticky: function (text) { bar.setActivity(primaryPane, text, primaryPane.transfer) }
        onStickyStarting: function (text) { bar.setActivity(primaryPane, text, primaryPane.transfer, true) }
        onConvertRequested: function (name) { convertDialog.open(name, primaryPane) }
        // Archives040: compress opens over resolved paths, extract over the named archive.
        onCompressPasswordRequested: function (paths, menuId) {
            view.openArchiveCards().askCompressPassword(primaryPane, paths, menuId)
        }
        onExtractPasswordRequested: function (path, dest, err) { view.openArchiveCards().openExtractPassword({ path: path, dest: dest }, primaryPane, err) }
        onPermissionsRequested: function (path) { permissionsDialog.open(path, primaryPane) }
        // Permissions040: the whole selection's paths for the multi-row card.
        onPermissionsBatchRequested: function (paths) { permissionsDialog.openMany(paths, primaryPane) }
        // Bulk rename takes the whole selection's paths for its card.
        onBulkRenameRequested: function (paths) { view.openBulkRename(paths, primaryPane) }
        onPathBarRequested: chrome.startEdit()
        // Issue 9. ViewState persists the stop and Theme derives its own tokens from it, so
        // the whole window follows without any surface reading the chord itself.
        onTextSizeRequested: function (direction) { view.applyTextSize(direction) }
        onOpened: function (path) { if (shareBrowser.owner === primaryPane) shareBrowser.close() }
    }

    Loader {
        id: secondPane
        anchors { left: primaryPane.right; right: parent.right; top: tabBar.bottom; bottom: bar.top }
        anchors.leftMargin: Theme.spacing.hairline
        property bool built: false
        active: view.dualMode || built
        visible: view.dualMode
        sourceComponent: Item {
            readonly property alias pane: otherPane
            readonly property alias backend: otherBackend
            Backend {
                id: otherBackend
                preserveSort: true
                onQuitReady: view.backendDrained(1)
            }
            Flea.Pane {
                id: otherPane
                anchors.fill: parent
                backend: otherBackend
                clipboardState: primaryPane.clipboardState
                sharedSidebar: primaryPane.sidebar
                chromeBar: chrome
                tabStrip: tabBar
                networkService: view.networkService
                sharedNetworkService: primaryPane.networkService
                dualMode: true
                listOnly: true
                paneFocused: view.currentPane === otherPane
                overlayParent: view
                preview: primaryPane.preview
                previewLoader: primaryPane.previewLoader
                shareBrowser: primaryPane.shareBrowser
                keymapSheet: primaryPane.keymapSheet
                settingsPanel: primaryPane.settingsPanel
                statusBar: bar
                onFocusRequested: view.focusPane(1)
                onViewRequested: function (mode) { chrome.viewChosen(mode) }
                onSwitchPane: view.focusPane(0)
                onSwitchToRail: view.switchToLeftRail()
        onPathChanged: view.rememberPaths()
                onClipboardChanged: if (primaryPane.clipboard !== clipboard) primaryPane.clipboard = clipboard
                onMessage: function(text, error) { bar.say(text, error) }
                onArmPrompted: function(text) { bar.sayArm(text) }
                onOperationResult: function(headline, detail, error) { bar.say(headline, error, detail) }
                onSticky: function(text) { bar.setActivity(otherPane, text, otherPane.transfer) }
                onStickyStarting: function(text) { bar.setActivity(otherPane, text, otherPane.transfer, true) }
                onConvertRequested: function(name) { convertDialog.open(name, otherPane) }
                onCompressPasswordRequested: function (paths, menuId) {
                    view.openArchiveCards().askCompressPassword(otherPane, paths, menuId)
                }
                onExtractPasswordRequested: function (path, dest, err) { view.openArchiveCards().openExtractPassword({ path: path, dest: dest }, otherPane, err) }
                onPermissionsRequested: function(path) { permissionsDialog.open(path, otherPane) }
                onPermissionsBatchRequested: function (paths) { permissionsDialog.openMany(paths, otherPane) }
                onBulkRenameRequested: function (paths) { view.openBulkRename(paths, otherPane) }
                onPathBarRequested: chrome.startEdit()
                onTextSizeRequested: function(direction) { view.applyTextSize(direction) }
                onOpened: if (otherPane.shareBrowser.owner === otherPane) otherPane.shareBrowser.close()
            }
        }
        onLoaded: {
            built = true
            // Before initialized this is the launch, the only time a command-line folder applies.
            var named = view.initialized ? "" : (Quickshell.env("FLEA_PATH") || "")
            var pair = Startup.dualPaths(ViewState.state.dual, primaryPane.path || primaryPane.home, named)
            item.pane.clipboard = primaryPane.clipboard
            if (pair.launchSide === 1) {
                item.pane.pendingSelect = Quickshell.env("FLEA_SELECT") || ""
                Tabs.prepareCursor(item.pane, named ? Quickshell.env("FLEA_TAB_CURSOR") : "")
            }
            item.pane.open(pair.paths[1])
            if (view.initialized && view.dualMode) view.focusPane(view.focusSide)
        }
    }

    // The zone starts on the divider pixel and runs right over the second listing's row padding, so the first listing keeps its scroll lane whole.
    Loader {
        x: primaryPane.width
        y: primaryPane.y
        active: view.dualMode
        sourceComponent: Item {
            id: divider
            property bool dragging: false
            property bool moved: false
            property real startX: 0
            property real dx: 0
            readonly property real leftWidth: primaryPane.width - primaryPane.railInset
            width: handle.width
            height: primaryPane.height
            function shareAt(travel) { return view.dualShareOf(view.dualSplitScale * (divider.leftWidth + travel) / view.dualRoom) }
            function begin(mouse) {
                divider.startX = handle.mapToItem(view, mouse.x, mouse.y).x
                divider.dx = 0
                divider.moved = false
                divider.dragging = true
            }
            // A press that never travels is a click and writes nothing; the accent line follows the pointer, held to the clamp.
            function move(mouse) {
                if (!divider.dragging) return
                var travel = handle.mapToItem(view, mouse.x, mouse.y).x - divider.startX
                if (!divider.moved && Math.abs(travel) < 1) return
                divider.moved = true
                divider.dx = view.dualRoom * divider.shareAt(travel) - divider.leftWidth
            }
            // One leaf on release, so both listings re-lay from it in the same frame.
            function end() {
                if (!divider.dragging) return
                if (divider.moved && view.dualFits) ViewState.changeLeaf("dual", {split: Math.round(view.dualSplitScale * divider.shareAt(divider.dx))})
                divider.reset()
            }
            function reset() { divider.dragging = false; divider.moved = false; divider.dx = 0 }
            Rectangle {
                width: Theme.spacing.hairline
                height: parent.height
                color: Theme.color.mutedFrame
            }
            Flea.ResizeHandle {
                id: handle
                height: parent.height
                lineX: divider.dx
                hot: divider.dragging
                onPressed: function (mouse) { divider.begin(mouse) }
                onMoved: function (mouse) { divider.move(mouse) }
                onCanceled: divider.reset()
                onReleased: divider.end()
                onDoubleClicked: ViewState.changeLeaf("dual", {split: view.dualEqualSplit})
            }
        }
    }

    Flea.StatusBar {
        id: bar
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        path: view.currentPane.trash.opened ? "Trash"
            : view.currentPane.path
        total: view.currentPane.flashActive && view.flashHost ? view.flashHost.count
            : view.currentPane.trash.opened ? view.currentPane.trash.total : view.currentPane.total
        // The counts zone names the drives while the flasher owns the pane; disk facts stay the covered folder's.
        flashCount: view.currentPane.flashActive && view.flashHost ? view.flashHost.countText : ""
        listingState: view.currentPane.listingState
        pane: (view.currentPane.trash.opened || view.currentPane.flashActive) ? null : view.currentPane
        armOwner: view.currentPane.trash.opened ? view.currentPane.trash.item : view.currentPane
        selectionCount: view.currentPane.flashActive ? 0
            : view.currentPane.trash.opened ? view.currentPane.trash.selectedCount : view.currentPane.selectionCount()
        fsName: view.currentPane.fsName
        fsFree: view.currentPane.fsFree
        fsPending: !view.currentPane.fsAnswered
        searchRunning: view.currentPane.searchRunning
        searchLine: Search.live(view.currentPane)
                    ? Search.statusLine(view.currentPane.searchRunning, view.currentPane.total, view.currentPane.searchScanned, view.currentPane.searchMs)
                    : ""
        retryLine: view.currentPane.trash.opened || view.currentPane.flashActive ? "" : view.currentPane.retrySelectionText
        onTransferCancelRequested: function (id) {
            // A flash cancels through its own request: only Cancel stops one, wherever the card sits.
            if (bar.transfer.flash === true) bar.transferOwner.backend.flashCancel(id)
            else bar.transferOwner.backend.transfercancel(id)
        }
    }

    // Quick Look is built by the first Space, so a window that never previews pays nothing for it; Pane.quickLook() turns it on.
    Loader {
        id: previewLoader
        anchors.fill: parent
        z: 1
        active: false
        source: "Preview.qml"
        onLoaded: item.pane = Qt.binding(function () { return view.currentPane })
    }

    MouseArea {
        anchors.fill: parent
        z: 4
        visible: primaryPane.trash.confirming || (secondPane.item && secondPane.item.pane.trash.confirming)
        hoverEnabled: true
        acceptedButtons: Qt.AllButtons
        onWheel: function(wheel) { wheel.accepted = true }
    }

    // Every overlay below is built by its first open and kept, see AGENTS.md rule 6: a launch
    // that never opens one pays neither its compile nor its objects. Each Loader carries the
    // one or two members its callers read, and ui/Ipc.qml reads the built item or null.
    // Each Loader carries its item's z, because a z set inside the item orders it only within
    // the Loader, and the share browser below would otherwise paint over an open card.
    Loader {
        id: convertDialog
        z: 2
        anchors.fill: parent
        active: false
        source: "ConvertDialog.qml"
        readonly property bool opened: item !== null && item.opened
        property var owner: null
        function open(name, holder) { owner = holder; active = true; item.open(name, holder) }
    }
    Connections {
        target: convertDialog.item
        function onAccepted(source, format, strip, requestId) { Ops.convert(convertDialog.owner, source, format, strip, requestId) }
    }

    // Archives040: both password cards build on first open, never at startup.
    property var archiveCards: null
    function openArchiveCards() {
        if (!view.archiveCards) {
            var component = Qt.createComponent("ArchiveCards.qml")
            view.archiveCards = component.status === Component.Ready ? component.createObject(view, { z: 2 }) : null
            if (!view.archiveCards) throw new Error("ArchiveCards.qml did not build: " + component.errorString())
        }
        return view.archiveCards
    }

    Loader {
        id: permissionsDialog
        z: 2
        anchors.fill: parent
        active: false
        source: "PermissionsDialog.qml"
        readonly property bool opened: item !== null && item.opened
        property var owner: null
        function open(path, holder) { owner = holder; active = true; item.open(path, holder.listArea) }
        function openMany(paths, holder) { owner = holder; active = true; item.openMany(paths, holder.listArea) }
    }
    Connections {
        target: permissionsDialog.item
        function onRequested(message) { permissionsDialog.owner.backend.send(message) }
        function onChanged(note) { permissionsDialog.owner.refresh("", true); bar.say(note && note.length > 0 ? note : "Permissions changed.", false) }
        function onRefreshNeeded() { permissionsDialog.owner.refresh("", true) }
    }
    // The bulk card is built once per window on its first multi-row Rename, with no placeholder before that.
    property var bulkRenameDialog: null
    function openBulkRename(paths, holder) {
        if (!view.bulkRenameDialog) view.bulkRenameDialog = Qt.createComponent("BulkRenameDialog.qml").createObject(view, { z: 2 })
        view.bulkRenameDialog.open(paths, holder)
    }
    Connections {
        target: permissionsDialog.owner ? permissionsDialog.owner.backend : null
        function onPermissionsResult(message) {
            if (permissionsDialog.item) permissionsDialog.item.receive(message)
        }
        function onFailed(where, input, message, mode) {
            if (where === "backend" && permissionsDialog.item) permissionsDialog.item.backendFailed()
        }
    }

    // The keymap sheet ? opens, over the whole window as the convert popup is.
    Loader {
        id: keymapSheet
        z: 2
        anchors.fill: parent
        active: false
        source: "KeymapSheet.qml"
        readonly property bool opened: item !== null && item.opened
        function open(holder) { active = true; item.open(holder) }
    }

    // The settings panel, reached by the comma key from either view, by the toolbar's sliders
    // button, and by the third door the Settings board draws: the background menu's own
    // Settings row, which ui/js/Menu.js backgroundEntries builds and ui/Pane.qml act routes.
    Loader {
        id: settingsPanel
        z: 3
        anchors.fill: parent
        active: false
        source: "SettingsPanel.qml"
        readonly property bool opened: item !== null && item.opened
        function open(holder) { active = true; item.open(holder) }
    }

    Loader {
        id: networkDialog
        z: 2
        anchors.fill: parent
        active: false
        source: "NetworkDialog.qml"
        property var owner: null
        property Item origin: null
        readonly property bool opened: item !== null && item.opened
        function open(holder) {
            if (opened) return
            // The pane that asked for the form is the one its answer returns to; the hosts page's
            // manual row passes its own owner, which in dual view is not always the focused pane.
            origin = holder || view.currentPane
            active = true
            item.open()
        }
        function openLocation(uri, label, password, reason, failedConnect, fromPane) {
            if (opened || !fromPane) return
            origin = fromPane
            active = true
            item.openLocation(uri, label, password, reason, failedConnect)
        }
    }
    Connections {
        target: networkDialog.item
        // FocusScope remembers its own last-focused child, list or rail, and restores it.
        function onClosed() { if (view.currentPane.sidebar) view.currentPane.sidebar.editingPlace = ""; view.currentPane.forceActiveFocus() }
        function onMountRequested(requestId, uri, label, password) {
            if (!networkDialog.origin) {
                networkDialog.item.mountFinished(requestId, uri, false, "The requesting pane is no longer available.", false)
                return
            }
            // The host outlives the rail, so hiding the rail mid-dialog loses no answer: the
            // completed below still lands and the dialog still finishes.
            var sidebar = networkDialog.origin ? networkDialog.origin.sidebar : null
            if (sidebar) RailMenu.placeSubmitted(sidebar, requestId)
            networkDialog.owner = view.ensureNetworkService()
            networkDialog.owner.saveLocation(uri, label, password, requestId, networkDialog.origin)
        }
        function onCancelRequested(requestId) { if (networkDialog.owner) networkDialog.owner.cancelLocation(requestId) }
        // The dialog's Browse trades the form for discovery; the hosts answer reopens this dialog's
        // origin as the share browser's host rows, exactly as a listed server's shares would.
        function onBrowseRequested() {
            view.browseHosts(networkDialog.origin || view.currentPane)
        }
    }

    // The dialog's answer lands here rather than on the rail, so it lands whether the rail is
    // shown or has been unloaded mid-mount. Every host answer routes once by origin here;
    // ui/PaneRail.qml carries no duplicate handling, so a dual-view open cannot fire twice.
    Connections {
        target: view.networkService
        function onCompleted(requestId, uri, success, reason, connectFailed) {
            var sidebar = view.currentPane.sidebar
            if (sidebar && view.networkService) RailMenu.placeSaved(sidebar, view.networkService, requestId, uri, success)
            if (networkDialog.item) networkDialog.item.mountFinished(requestId, uri, success, reason, connectFailed)
        }
        function onSharesListed(baseUri, baseLabel, names, origin) { if (origin) shareBrowser.open(baseUri, baseLabel, names, origin) }
        // Hosts rows ride the bare workgroup root: activating one is the bare-root open whose
        // answer is its shares, so the browser walks down one level without new routing here.
        // An empty LAN is a real answer with nothing to show: the form opens with the reason in
        // its own banner, so the "+" still leads somewhere that can connect.
        function onHostsListed(names, origin) {
            if (!origin) return
            // A late answer from a browse the pane already left steers nothing: only a crawl still
            // waiting on hosts may become rows, or hand the pane to the form.
            if (!shareBrowser.active || !shareBrowser.scanning) return
            if (!names || names.length === 0) {
                // The crawl that opened with the ＋ ends here: the form takes the pane, so the
                // looking page does not sit behind it waiting for an answer that already came.
                shareBrowser.close()
                networkDialog.openLocation("", "", "", "No network hosts answered. Type an address instead.", false, origin)
                return
            }
            shareBrowser.openHosts(names, origin)
        }
        function onRetryRequested(uri, label, password, reason, failedConnect, origin) {
            shareBrowser.revertToHosts()
            networkDialog.openLocation(uri, label, password, reason, failedConnect, origin)
        }
        function onOpened(path, origin) { if (origin) RailKeys.openFrom(origin, path, origin.sidebar) }
        function onOpenFileRequested(path, origin) { if (origin) origin.openFile(path) }
        function onMessage(text, isError) {
            // A failure toast while a crawl is up is the crawl's own ending: the page it was
            // reading for is not coming, so the hosts page returns rather than spinning on.
            shareBrowser.revertToHosts()
            var pane = view.currentPane
            if (pane.sidebar) RailKeys.messaged(pane.sidebar, isError)
            pane.message(text, isError)
        }
        function onSticky(text, origin) { (origin || view.currentPane).sticky(text) }
        function onStartingBridge(text, origin) { (origin || view.currentPane).stickyStarting(text) }
        function onRenamed() { if (view.currentPane.sidebar) view.currentPane.sidebar.reloadBookmarks() }
    }

    // One entry point for both paths into discovery: the rail's "+" (below) and the dialog's
    // Browse button, so neither can drift from the other.
    function browseHosts(pane) {
        // The crawl opens before the ask: the LAN's answer takes seconds, and the pane the ＋
        // was pressed on should say "looking" for every one of them.
        shareBrowser.openScanning(pane)
        var service = view.ensureNetworkService()
        if (service)
            service.listHosts(pane)
        else
            shareBrowser.close()
    }

    Connections {
        target: view.currentPane.sidebar
        function onAddRequested() { view.browseHosts(view.currentPane) }
        function onEditRequested(uri, label, password, reason, failedConnect, origin) {
            networkDialog.openLocation(uri, label, password, reason, failedConnect, origin)
        }
        function onUnlockRequested(device, label, size, fstype) { view.ensureUnlockHost().openFor(device, label, size, fstype, view.currentPane.sidebar) }
    }

    // Unlock (U039): created once on first use and reused after, so no placeholder object stands before it.
    property var unlockHost: null
    function ensureUnlockHost() {
        if (view.unlockHost === null)
            view.unlockHost = Qt.createComponent("UnlockHost.qml").createObject(view, { z: 2, window: view, footer: bar })
        return view.unlockHost
    }

    // A bare Network entry's own shares, same listArea placement as EmptyState above.
    // An Item fronts this Loader because its callers read active, which is a Loader's own load switch.
    Item {
        id: shareBrowser
        property Item owner: primaryPane
        onOwnerChanged: if (!owner) close()
        visible: owner !== null && (owner === primaryPane || view.dualMode)
        x: owner ? (owner === primaryPane ? primaryPane.x : secondPane.x) + owner.listSlot.x : 0
        y: owner ? tabBar.y + tabBar.height + owner.listSlot.y : 0
        width: owner ? owner.listSlot.width : 0
        height: owner ? owner.listSlot.height : 0
        readonly property bool active: shareLoader.item !== null && shareLoader.item.active
        // The crawl's own mark, forwarded the way active is: a handler that asks whether the
        // browser is still waiting (a late answer must steer nothing) reads the wrapper, not
        // the loader's item, and the wrapper without this read undefined as false.
        readonly property bool scanning: shareLoader.item !== null && shareLoader.item.scanning
        function open(uri, label, names, origin) { owner = origin; shareLoader.active = true; shareLoader.item.open(uri, label, names) }
        function openHosts(names, origin) { owner = origin; shareLoader.active = true; shareLoader.item.openHosts(names) }
        // The instant page: avahi can take seconds to answer, so the ＋ opens the crawl first
        // and the answer replaces it — a blank pane in between reads as broken.
        function openScanning(origin) { owner = origin; shareLoader.active = true; shareLoader.item.openScanning() }
        // A host row's open holds the same pane: its crawl ("reading") stands until the shares
        // or the sign-in form replaces it.
        function openReading(label, origin) { owner = origin; shareLoader.active = true; shareLoader.item.openReading(label) }
        // A crawl that ends in a refusal or an invitation falls back to the page it came from,
        // so no looking or reading page outlives its own ask.
        function revertToHosts() {
            if (shareLoader.item && shareLoader.item.scanning && shareLoader.item.hostsPage !== null)
                shareLoader.item.open("smb://", "Network", shareLoader.item.hostsPage)
        }
        function close() { if (shareLoader.item) shareLoader.item.close() }
        // ui/js/Focus.js shareBrowserAct's two other verbs, reached only while the overlay is up.
        function moveCursor(delta) { if (shareLoader.item) shareLoader.item.moveCursor(delta) }
        function activateCursor() { if (shareLoader.item) shareLoader.item.activateCursor() }
        Loader { id: shareLoader; anchors.fill: parent; active: false; source: "ShareBrowser.qml" }
    }
    Connections {
        target: shareLoader.item
        function onClosed() { view.currentPane.forceActiveFocus() }
        // The hosts page's last row: the form opens over the page that led to it, the mirror of
        // the dialog's Browse button closing the form for the page.
        function onManualRequested() {
            // Captured before the close: the browser's owner is the pane the form's mount answer
            // must return to, and view.currentPane can differ from it in dual view.
            var holder = shareBrowser.owner
            shareBrowser.close()
            networkDialog.open(holder)
        }
        function onActivated(uri, label) {
            view.focusPane(shareBrowser.owner === primaryPane ? 0 : 1)
            // The host outlives the rail, so a hidden rail never blocks a share the browser lists.
            var service = shareBrowser.owner.ensureNetworkService()
            if (!service) {
                shareBrowser.owner.message("That network share cannot be opened here.", true)
                return
            }
            // A host row's own open is a wait the pane should own from the first instant: the
            // crawl (its word "reading") holds it until the shares or the form replace it.
            if (service.isBareRoot(uri))
                shareBrowser.openReading(label, shareBrowser.owner)
            service.openChildShare(uri, label, shareBrowser.owner)
        }
    }

    // Issue 20: the mouse's side buttons belong to the window, not a row; Nav.js mouseBack and MouseNav.js forward decide, and the overlays below refuse both.
    TapHandler {
        acceptedButtons: Qt.BackButton | Qt.ForwardButton
        onTapped: function (eventPoint, button) {
            if (view.currentPane.menuActions.opened || settingsPanel.opened || view.currentPane.trash.confirming || chrome.editing || convertDialog.opened || permissionsDialog.opened || (view.bulkRenameDialog !== null && view.bulkRenameDialog.opened) || keymapSheet.opened
                    || networkDialog.opened || (view.unlockHost !== null && view.unlockHost.opened) || (shareBrowser.active && shareBrowser.owner === view.currentPane) || view.quickLookActive
                    || view.currentPane.renameEditor() !== null || (view.currentPane.sidebar && view.currentPane.sidebar.renameEditor() !== null))
                return
            if (button === Qt.ForwardButton) MouseNav.forward(view.currentPane)
            else if (view.currentPane.flashActive) view.closeFlash()
            else if (view.currentPane.trash.opened) view.currentPane.trash.close()
            else Nav.mouseBack(view.currentPane)
        }
    }

    Component.onCompleted: {
        var home = Quickshell.env("HOME")
        var named = Quickshell.env("FLEA_PATH") || ""
        var start = Startup.startPath(ViewState.state, home, named)
        var pair = Startup.dualPaths(ViewState.state.dual, start, named)
        // Read once, and only on the side that took the named folder; Pane.applyPendingSelect() forgets it after the first rows.
        if (!view.dualMode || pair.launchSide !== 1) {
            primaryPane.pendingSelect = Quickshell.env("FLEA_SELECT") || ""
            Tabs.prepareCursor(primaryPane, named ? Quickshell.env("FLEA_TAB_CURSOR") : "")
        }
        // Tabs040 callout 2: Last folder reopens every remembered tab in order at its folder.
        // Dual startup keeps its own pair, and a named path outranks the strip either way.
        var plan = view.dualMode ? null : Tabs.restorePlan(ViewState.state, named)
        if (plan)
            primaryPane.tabs = Tabs.pack(Tabs.restoreItems(primaryPane, plan.paths), plan.index)
        primaryPane.open(plan ? plan.paths[plan.index] : (view.dualMode ? pair.paths[0] : start))
        view.initialized = true
        // flea --unlock opens the same dialog a rail row opens, for scripts and a udev rule.
        var unlockDevice = Quickshell.env("FLEA_UNLOCK") || ""
        if (unlockDevice.length > 0)
            view.ensureUnlockHost().openForDevice(unlockDevice, view.currentPane.sidebar)
        if (view.dualMode) view.focusPane(view.focusSide)
        trashSweep.start()
        UpdateCheck.startPolling()
        // Last, behind the listing request, and still before ui/boot/shell.qml creates the window and reads this body's colour.
        Glass.launch()
    }

    // The 30 day sweep runs off the startup path, not on it: a Trash listing costs one gio call per
    // item and first paint is measured. Late enough that the window is up and the backend is
    // answering, long before anyone reaches the Trash rail row. ui/TrashHost.qml refuses it when the
    // setting is off, when it has already run today, and when the Trash window exists at all.
    Timer {
        id: trashSweep
        interval: 2000
        repeat: false
        onTriggered: primaryPane.trash.sweep()
    }

    // The test seam (AGENTS.md "Testing"), built once the first listing answers or waits, never for the first frame.
    Loader {
        id: ipcLoader
        active: false
        asynchronous: true
        Component.onCompleted: setSource("Ipc.qml", {
            fleaWindow: Qt.binding(function () { return view.host }),
            pane: Qt.binding(function () { return view.currentPane }),
            panes: Qt.binding(function () { return [primaryPane, secondPane.item ? secondPane.item.pane : null] }),
            bar: Qt.binding(function () { return bar }),
            backend: Qt.binding(function () { return view.currentPane.backend }),
            chrome: Qt.binding(function () { return chrome }),
            tabBar: Qt.binding(function () { return tabBar }),
            convertDialog: Qt.binding(function () { return convertDialog.item }),
            permissionsDialog: Qt.binding(function () { return permissionsDialog.item }),
            bulkRenameDialog: Qt.binding(function () { return view.bulkRenameDialog }),
            keymapSheet: Qt.binding(function () { return keymapSheet.item }),
            settingsPanel: Qt.binding(function () { return settingsPanel.item }),
            networkDialog: Qt.binding(function () { return networkDialog.item }),
            shareBrowser: Qt.binding(function () { return shareLoader.item }),
            emptyState: Qt.binding(function () { return view.currentPane.emptyState }),
            archiveCards: Qt.binding(function () { return view.archiveCards }),
            unlockHost: Qt.binding(function () { return view.unlockHost })
        })
    }
    readonly property bool clearSurface: Glass.clear
}
