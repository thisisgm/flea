import Quickshell
import QtQuick
import qs.Commons
import "."
import "." as Flea
import "js/TextSize.js" as TextSize
import "js/Nav.js" as Nav
import "js/Ops.js" as Ops
import "js/RailKeys.js" as RailKeys
import "js/RailMenu.js" as RailMenu
import "js/Search.js" as Search
import "js/Startup.js" as Startup

// Everything inside the window ui/boot/shell.qml maps, arriving by file: URL on the first frame
// because that is the only way Qt caches it; see AGENTS.md "The first window".
Rectangle {
    id: view
    anchors.fill: parent
    color: Theme.color.background
    // The window this body was loaded into. Named host, not fleaWindow: a root property of
    // that name would bind to itself through Ipc and read null.
    property var host
    readonly property bool dualMode: ViewState.state.view === "dual"
    property int focusSide: (ViewState.state.dual || {}).focus === 1 ? 1 : 0
    readonly property var currentPane: dualMode && focusSide === 1 && secondPane.item ? secondPane.item.pane : primaryPane
    property bool initialized: false
    property bool closing: false
    property var drained: [false, false]

    function focusPane(side) {
        if (!dualMode) return
        focusSide = side
        currentPane.forceActiveFocus()
        Qt.callLater(view.rememberDual)
    }
    function rememberPaths() {
        // "Last folder" has to have a folder to return to, and the pair below is the dual
        // view's own. The primary pane is the one a single-view window opens, so it is the
        // one recorded; a write that lands the value already stored owes nothing, see
        // ui/ViewState.qml "owe".
        if (initialized && !dualMode && primaryPane.path)
            ViewState.rememberLastPath(primaryPane.path)
        if (!initialized || !dualMode || !secondPane.item || !primaryPane.path || !secondPane.item.pane.path) return
        Qt.callLater(view.rememberDual)
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
        if (closing && drained[0] && drained[1]) Quickshell.execDetached(["kill", String(Quickshell.processId)])
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
    }
    readonly property var networkService: networkHost
    function ensureNetworkService() {
        return networkHost
    }

    // The canvas's own top chrome: where you are on the left, how you are looking at it on
    // the right. The path lives here, which is why the status bar below carries counts instead.
    Flea.ChromeBar {
        id: chrome
        inputLive: !preview.active
        visible: view.dualMode || !view.currentPane.trash.opened
        height: visible ? Theme.chromeHeight : 0
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        path: view.currentPane.trash.opened ? "Trash"
            : view.currentPane.path
        // True only when the drawn path's listing failed, so the bar retries it and nothing else.
        pathFailed: Nav.pathFailed(view.currentPane)
        home: view.currentPane.home
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
        onViewChosen: function (mode) { ViewState.changeKey("view", mode) }
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
        onJumpRequested: function (id, favourites, recent) { view.currentPane.backend.jump(id, favourites, recent) }
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
    }

    Flea.Pane {
        id: primaryPane
        anchors.left: parent.left
        width: view.dualMode ? sidebarWidth + (view.width - sidebarWidth - Theme.spacing.hairline) / 2 : view.width
        anchors.top: tabBar.bottom
        anchors.bottom: bar.top
        backend: backend
        networkService: view.networkService
        dualMode: view.dualMode
        paneFocused: view.currentPane === primaryPane
        railPane: view.currentPane
        onFocusRequested: view.focusPane(0)
        onSwitchPane: view.focusPane(1)
        onPathChanged: view.rememberPaths()
        onClipboardChanged: if (secondPane.item && secondPane.item.pane.clipboard !== clipboard) secondPane.item.pane.clipboard = clipboard
        overlayParent: view
        preview: preview
        shareBrowser: shareBrowser
        keymapSheet: keymapSheet
        settingsPanel: settingsPanel
        statusBar: bar
        onMessage: function (text, isError) { bar.say(text, isError) }
        onForgetMessage: function (text) { bar.forget(text) }
        onOperationResult: function (headline, detail, isError) { bar.say(headline, isError, detail) }
        // A running operation's line, which stands until the operation replaces it; see ui/StatusBar.qml.
        onSticky: function (text) { bar.setActivity(primaryPane, text, primaryPane.transfer) }
        onConvertRequested: function (name) { convertDialog.open(name, primaryPane) }
        onPermissionsRequested: function (path) { permissionsDialog.open(path, primaryPane) }
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
                sharedSidebar: primaryPane.sidebar
                networkService: view.networkService
                sharedNetworkService: primaryPane.networkService
                dualMode: true
                listOnly: true
                paneFocused: view.currentPane === otherPane
                overlayParent: view
                preview: primaryPane.preview
                shareBrowser: primaryPane.shareBrowser
                keymapSheet: primaryPane.keymapSheet
                settingsPanel: primaryPane.settingsPanel
                statusBar: bar
                onFocusRequested: view.focusPane(1)
                onSwitchPane: view.focusPane(0)
                onPathChanged: view.rememberPaths()
                onClipboardChanged: if (primaryPane.clipboard !== clipboard) primaryPane.clipboard = clipboard
                onMessage: function(text, error) { bar.say(text, error) }
                onOperationResult: function(headline, detail, error) { bar.say(headline, error, detail) }
                onSticky: function(text) { bar.setActivity(otherPane, text, otherPane.transfer) }
                onConvertRequested: function(name) { convertDialog.open(name, otherPane) }
                onPermissionsRequested: function(path) { permissionsDialog.open(path, otherPane) }
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
            if (pair.launchSide === 1)
                item.pane.pendingSelect = Quickshell.env("FLEA_SELECT") || ""
            item.pane.open(pair.paths[1])
            if (view.initialized && view.dualMode) view.focusPane(view.focusSide)
        }
    }

    Rectangle {
        x: primaryPane.width
        y: primaryPane.y
        width: Theme.spacing.hairline
        height: primaryPane.height
        visible: view.dualMode
        color: Theme.color.muted
    }

    Flea.StatusBar {
        id: bar
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        path: view.currentPane.trash.opened ? "Trash"
            : view.currentPane.path
        total: view.currentPane.trash.opened ? view.currentPane.trash.total : view.currentPane.total
        listingState: view.currentPane.listingState
        pane: view.currentPane.trash.opened ? null : view.currentPane
        armOwner: view.currentPane.trash.opened ? view.currentPane.trash.item : view.currentPane
        selectionCount: view.currentPane.trash.opened ? view.currentPane.trash.selectedCount : view.currentPane.selectionCount()
        fsName: view.currentPane.fsName
        fsFree: view.currentPane.fsFree
        searchRunning: view.currentPane.searchRunning
        searchLine: view.currentPane.searchMode === "results"
                    ? Search.statusLine(view.currentPane.searchRunning, view.currentPane.total, view.currentPane.searchScanned, view.currentPane.searchMs)
                    : ""
        retryLine: view.currentPane.trash.opened ? "" : view.currentPane.retrySelectionText
        onTransferCancelRequested: function (id) { bar.transferOwner.backend.transfercancel(id) }
    }

    Flea.Preview { id: preview; pane: view.currentPane }

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

    Loader {
        id: permissionsDialog
        z: 2
        anchors.fill: parent
        active: false
        source: "PermissionsDialog.qml"
        readonly property bool opened: item !== null && item.opened
        property var owner: null
        function open(path, holder) { owner = holder; active = true; item.open(path, holder) }
    }
    Connections {
        target: permissionsDialog.item
        function onRequested(message) { permissionsDialog.owner.backend.send(message) }
        function onChanged() { permissionsDialog.owner.refresh(); bar.say("Permissions changed.", false) }
    }
    Connections {
        target: permissionsDialog.owner ? permissionsDialog.owner.backend : null
        function onPermissionsResult(message) {
            if (permissionsDialog.item) permissionsDialog.item.receive(message)
        }
        function onFailed(where, input, message, mode) {
            if (where === "backend" && permissionsDialog.item) permissionsDialog.item.backendFailed(message)
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
        function open() {
            if (opened) return
            origin = view.currentPane
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
                networkDialog.item.mountFinished(requestId, uri, false, "The requesting pane is no longer available.")
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
    }

    // The dialog's answer lands here rather than on the rail, so it lands whether the rail is
    // shown or has been unloaded mid-mount. Every host answer routes once by origin here;
    // ui/PaneRail.qml carries no duplicate handling, so a dual-view open cannot fire twice.
    Connections {
        target: view.networkService
        function onCompleted(requestId, uri, success, reason) {
            var sidebar = view.currentPane.sidebar
            if (sidebar && view.networkService) RailMenu.placeSaved(sidebar, view.networkService, requestId, uri, success)
            if (networkDialog.item) networkDialog.item.mountFinished(requestId, uri, success, reason)
        }
        function onSharesListed(baseUri, baseLabel, names, origin) { if (origin) shareBrowser.open(baseUri, baseLabel, names, origin) }
        function onRetryRequested(uri, label, password, reason, failedConnect, origin) {
            networkDialog.openLocation(uri, label, password, reason, failedConnect, origin)
        }
        function onOpened(path, origin) { if (origin) RailKeys.openFrom(origin, path, origin.sidebar) }
        function onOpenFileRequested(path, origin) { if (origin) origin.openFile(path) }
        function onMessage(text, isError) {
            var pane = view.currentPane
            if (pane.sidebar) RailKeys.messaged(pane.sidebar, isError)
            pane.message(text, isError)
        }
        function onSticky(text, origin) { (origin || view.currentPane).sticky(text) }
        function onRenamed() { if (view.currentPane.sidebar) view.currentPane.sidebar.reloadBookmarks() }
    }

    Connections {
        target: view.currentPane.sidebar
        function onAddRequested() { networkDialog.open() }
        function onEditRequested(uri, label, password, reason, failedConnect, origin) {
            networkDialog.openLocation(uri, label, password, reason, failedConnect, origin)
        }
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
        function open(uri, label, names, origin) { owner = origin; shareLoader.active = true; shareLoader.item.open(uri, label, names) }
        function close() { if (shareLoader.item) shareLoader.item.close() }
        // ui/js/Focus.js shareBrowserAct's two other verbs, reached only while the overlay is up.
        function moveCursor(delta) { if (shareLoader.item) shareLoader.item.moveCursor(delta) }
        function activateCursor() { if (shareLoader.item) shareLoader.item.activateCursor() }
        Loader { id: shareLoader; anchors.fill: parent; active: false; source: "ShareBrowser.qml" }
    }
    Connections {
        target: shareLoader.item
        function onClosed() { view.currentPane.forceActiveFocus() }
        function onActivated(uri, label) {
            view.focusPane(shareBrowser.owner === primaryPane ? 0 : 1)
            // The host outlives the rail, so a hidden rail never blocks a share the browser lists.
            var service = shareBrowser.owner.ensureNetworkService()
            if (!service) {
                shareBrowser.owner.message("That network share cannot be opened here.", true)
                return
            }
            service.openChildShare(uri, label, shareBrowser.owner)
        }
    }

    // Issue 20: the mouse's side buttons, taken by the window because no row is being clicked;
    // ui/js/Nav.js mouseBack chooses between the history and the climb, mouseForward retraces a back
    // (never in Trash, as Pane.goForward). The menu's refusal is in Nav.js, where a suite can drive
    // it; the list below is the other overlays a side button press must not act behind.
    TapHandler {
        acceptedButtons: Qt.BackButton | Qt.ForwardButton
        onTapped: function (eventPoint, button) {
            if (view.currentPane.menuActions.opened || settingsPanel.opened || view.currentPane.trash.confirming || chrome.editing || convertDialog.opened || permissionsDialog.opened || keymapSheet.opened
                    || networkDialog.opened || (shareBrowser.active && shareBrowser.owner === view.currentPane) || preview.active
                    || view.currentPane.renameEditor() !== null || (view.currentPane.sidebar && view.currentPane.sidebar.renameEditor() !== null))
                return
            if (button === Qt.ForwardButton) { if (!view.currentPane.trash.opened) Nav.mouseForward(view.currentPane) }
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
        if (!view.dualMode || pair.launchSide !== 1)
            primaryPane.pendingSelect = Quickshell.env("FLEA_SELECT") || ""
        primaryPane.open(view.dualMode ? pair.paths[0] : start)
        view.initialized = true
        if (view.dualMode) view.focusPane(view.focusSide)
        trashSweep.start()
        UpdateCheck.startPolling()
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

    // The seam the tests drive, see AGENTS.md "Testing". Every reader lives in ui/Ipc.qml.
    Flea.Ipc {
        fleaWindow: view.host
        pane: view.currentPane
        panes: [primaryPane, secondPane.item ? secondPane.item.pane : null]
        bar: bar
        backend: view.currentPane.backend
        chrome: chrome
        tabBar: tabBar
        convertDialog: convertDialog.item
        permissionsDialog: permissionsDialog.item
        keymapSheet: keymapSheet.item
        settingsPanel: settingsPanel.item
        networkDialog: networkDialog.item
        shareBrowser: shareLoader.item
        emptyState: view.currentPane.emptyState
    }
}
