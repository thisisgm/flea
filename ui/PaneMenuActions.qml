import QtQuick
import "js/Menu.js" as Menu
import "js/Ops.js" as Ops

Loader {
    id: root
    required property var pane
    anchors.fill: parent
    z: 2
    active: false
    // OpenWith.html rule 4: Open with owns its own card, so the shared dialog is not asked to be one.
    property string dialogFor: ""
    source: root.dialogFor === "openWith" ? "OpenWithDialog.qml" : "MenuActionDialog.qml"
    readonly property bool opened: item !== null && item.opened
    readonly property bool deleting: item !== null && item.deletionActive
    property int requestId: 0
    property string identity: ""
    property string folder: ""
    property bool ready: false
    property string pendingAction: ""
    property bool pendingActivation: false
    property bool activationUsed: false
    // Which row a keyboard rename was asked for, so its reply cannot open the editor over another.
    property int pendingRenameIndex: -1
    property int launchingId: 0
    // OpenWith.html's flyout: the registry for the cursor row, asked for as the menu opens so the
    // submenu is populated by the time the row is reached. The flyout writes nothing; the dialog
    // stays the only place a default is written, which is Douglas de Moura's architecture kept whole.
    property var openWithApps: []
    property bool openWithLoaded: false
    property var survivors: []
    property int survivorId: 0
    property string survivorFolder: ""
    property string survivorListing: ""
    property string backgroundFolder: ""
    property bool providersRefreshing: false
    property int providerFormatsId: 0
    property bool providerQueriesStarted: false
    property bool providerDestinationPending: false
    property bool providerValidated: false
    property var providerFacts: ({})
    property bool taildropRefreshWaiting: false
    property bool dropboxRefreshWaiting: false
    property bool refreshingTaildrop: false
    property bool refreshingDropbox: false

    function providerAction(action) {
        return action.indexOf("taildrop:") === 0 || action === "dropbox" || action === "sharelink"
    }
    function refreshProviders(action) {
        providersRefreshing = true
        refreshingTaildrop = !action || action.indexOf("taildrop:") === 0
        refreshingDropbox = !action || action === "dropbox" || action === "sharelink"
        providerQueriesStarted = false
        providerDestinationPending = false
        taildropRefreshWaiting = false
        dropboxRefreshWaiting = false
        providerFormatsId = pane.backend.askFormats()
        pane.contextMenu().refreshProviderRows()
    }
    function finishProviders() {
        if (!providersRefreshing || !providerQueriesStarted || providerDestinationPending || !ready
                || taildropRefreshWaiting || dropboxRefreshWaiting
                || (refreshingTaildrop && pane.taildropService.checking)
                || (refreshingDropbox && pane.dropboxService && pane.dropboxService.dropboxChecking)) return
        if (!pendingActivation && pane.dropboxService && pane.dropboxService.dropboxReady) {
            providerDestinationPending = true
            pane.backend.send({c: "menuaction", op: "providerDestination", id: requestId, dest: pane.dropboxService.dropboxPath})
            return
        }
        providersFinished()
    }
    function providersFinished() {
        providersRefreshing = false
        pane.contextMenu().refreshProviderRows()
        if (pendingActivation && providerAction(pendingAction)) {
            providerValidated = true
            validateActivation()
        }
    }

    // rows, when the caller has one: a keyboard rename acts on the cursor, and Ops.targetIndices
    // answers with the selection whenever there is one, so the snapshot covered rows the rename was
    // never going to touch and the backend refused the cursor's own path as "not in the selection".
    function snapshot(rows) {
        if (deleting || survivorId) return
        requestId++
        ready = false
        pendingAction = ""
        pendingActivation = false
        activationUsed = false
        identity = pane.menuSelectionIdentity
        folder = pane.path
        // The registry belongs to the row that was snapshotted. Carrying the last row's answer over
        // offered one file's applications for another, and kept the self-hide rule from ever firing.
        openWithApps = []
        openWithLoaded = false
        pane.backend.send({c: "menuaction", op: "snapshot", id: requestId,
            rows: rows !== undefined ? rows : Ops.targetIndices(pane), cursor: pane.cursorIndex})
    }
    function open(action, menuId) {
        if (opened) return
        if (action === "rename" && pane.renamePending) { pane.message("Rename is still finishing.", false); return }
        if (deleting || survivorId) { pane.message("The deletion is still finishing.", false); return }
        if (action === "newFile") {
            requestId++
            folder = pane.path
            show(action)
            return
        }
        // The row the editor will open over, captured now: the cursor can move between this request
        // and its reply, and the editor used to open over wherever it had got to by then.
        if (action === "rename" && !menuId) pendingRenameIndex = pane.cursorIndex
        if (!requestId || identity !== pane.menuSelectionIdentity || (action === "rename" && !menuId))
            snapshot(action === "rename" && !menuId ? [pane.cursorIndex] : undefined)
        pendingAction = action
        pendingActivation = false
        if (ready) show(action)
    }
    function activate(action, selected) {
        if (!selected) {
            if (action === "addFavourite") {
                if (backgroundFolder !== pane.path) { pane.message("Folder changed; reopen the menu.", true); return }
                addFavourite(backgroundFolder)
            } else pane.performMenu(action, 0, null)
            return
        }
        if (activationUsed) return
        if (deleting || survivorId) { pane.message("The deletion is still finishing.", false); return }
        if (!requestId || identity !== pane.menuSelectionIdentity) {
            pane.message("Selected items changed; reopen the menu.", true)
            return
        }
        activationUsed = true
        providerValidated = false
        pendingAction = action
        pendingActivation = true
        if (ready) validateActivation()
    }
    function validateActivation() {
        if (providerAction(pendingAction) && !providerValidated) { refreshProviders(pendingAction); return }
        var action = pendingAction
        pendingAction = ""
        pendingActivation = false
        var split = action.indexOf(":")
        if (providerAction(action) && !pane.contextMenu().validateChoice(split < 0 ? action : action.substring(0, split),
                split < 0 ? "" : action.substring(split + 1))) return
        // OpenWith.html rule 2: the flyout is a one-off override that writes nothing, so a chosen
        // application launches through the same registry op the dialog submits, and the tail row is
        // the only way into the dialog, which stays the one place a default is written.
        if (action.indexOf("openWith:") === 0) {
            var chosen = action.substring("openWith:".length)
            if (chosen === Menu.OPEN_WITH_OTHER) { root.show("openWith"); return }
            root.launchingId = requestId
            pane.backend.send({c: "menuaction", op: "openWith", id: requestId, application: chosen})
            return
        }
        pane.backend.send({c: "menuaction", op: "activate", id: requestId, action: action,
            dest: pane.dropboxService ? pane.dropboxService.dropboxPath : ""})
    }
    function show(action) {
        pendingAction = ""
        if (action === "rename") { Ops.startRename(pane, requestId, pendingRenameIndex); pendingRenameIndex = -1; return }
        dialogFor = action
        active = true
        item.open(action, requestId, folder, pane.listArea)
    }
    function addFavourite(path) {
        Favourites.add(path, path.split("/").filter(function (part) { return part.length > 0 }).pop() || "/")
    }
    function locateSurvivors() {
        if (!survivorId || pane.listInFlight) return
        if (pane.path !== survivorFolder) { survivorId = 0; survivors = []; return }
        survivorListing = pane.menuSelectionIdentity
        pane.backend.send({c: "locate", paths: survivors, id: survivorId, menuId: survivorId})
    }
    Connections {
        target: root.pane.contextMenu()
        function onOpenedChanged() {
            var menu = root.pane.contextMenu()
            if (menu.opened && !menu.hasRow && !menu.forRail && !menu.forHeader)
                root.backgroundFolder = root.pane.path
            if (menu.opened && menu.hasRow && !menu.forRail && !menu.forHeader) root.refreshProviders()
        }
    }
    Connections {
        target: root.pane.taildropService
        function onRefreshed() {
            if (root.providersRefreshing && root.taildropRefreshWaiting) root.taildropRefreshWaiting = !root.pane.taildropService.refresh(root.providerFacts)
            root.finishProviders()
        }
    }
    Connections {
        target: root.pane.dropboxService
        function onDropboxRefreshed() {
            if (root.providersRefreshing && root.dropboxRefreshWaiting) root.dropboxRefreshWaiting = !root.pane.dropboxService.refreshDropbox(root.providerFacts)
            root.finishProviders()
        }
    }
    Connections {
        target: root.pane
        function onListInFlightChanged() { if (!root.pane.listInFlight) root.locateSurvivors() }
    }
    Connections {
        target: root.pane.backend
        function onFormatsResult(message) {
            if (!root.providersRefreshing || message.id !== root.providerFormatsId) return
            root.providerFacts = message.providers || {}
            root.taildropRefreshWaiting = root.refreshingTaildrop && !root.pane.taildropService.refresh(root.providerFacts)
            root.dropboxRefreshWaiting = root.refreshingDropbox && root.pane.dropboxService
                ? !root.pane.dropboxService.refreshDropbox(root.providerFacts) : false
            root.providerQueriesStarted = true
            root.finishProviders()
        }
        function onChanged(path) { if (path === root.folder && root.item) root.item.sourceChanged() }
        function onLocated(message) {
            if (!root.survivorId || message.id !== root.survivorId) return
            root.survivorId = 0
            root.survivors = []
            if (message.directory !== root.pane.path || root.pane.path !== root.survivorFolder
                    || root.survivorListing !== root.pane.menuSelectionIdentity) return
            if (!message.ok) { root.pane.message(message.error, true); return }
            var matches = message.matches || []
            root.pane.selection.clear()
            for (var i = 0; i < matches.length; i++) root.pane.selection.toggle(matches[i].index)
            root.pane.selectionVersion++
            if (matches.length) root.pane.setCursor(matches[0].index)
        }
        function onMenuResult(message) {
            if (message.op === "openWith" && message.id === root.launchingId) {
                root.launchingId = 0
                if (!root.opened || message.id !== root.requestId) {
                    if (!message.ok && !message.cancelled) root.pane.message(message.error || "The application could not be opened.", true)
                    return
                }
            }
            if (message.id !== root.requestId) return
            if (message.op === "applications") {
                // Both readers want it: the flyout's registry, and an open dialog that asked for it.
                if (root.item) root.item.receive(message)
                root.openWithApps = message.applications || []
                root.openWithLoaded = true
                root.pane.contextMenu().refreshProviderRows()
                return
            }
            if (message.op === "providerDestination") {
                root.providerDestinationPending = false
                if (!message.ok && root.pane.dropboxService) root.pane.dropboxService.dropboxReason = message.error
                root.providersFinished()
                return
            }
            if (message.op === "snapshot") {
                root.ready = message.ok === true && root.identity === root.pane.menuSelectionIdentity
                if (!root.ready) {
                    root.pendingAction = ""
                    root.pendingActivation = false
                    root.providersRefreshing = false
                    root.identity = ""
                    root.pane.message(message.error || "Selected items changed; reopen the menu.", true)
                } else if (root.pendingAction) {
                    if (root.pendingActivation) root.validateActivation()
                    else root.show(root.pendingAction)
                }
                if (root.ready && root.pane.contextMenu().opened && root.pane.contextMenu().hasRow)
                    // No installed flag: the flyout draws the registry alone, and asking for the
                    // whole catalogue here walked every applications directory on every right-click.
                    root.pane.backend.send({c: "menuaction", op: "applications", id: root.requestId})
                root.finishProviders()
                return
            }
            if (message.op === "activate") {
                if (!message.ok || root.identity !== root.pane.menuSelectionIdentity) {
                    root.pane.message(message.error || "Selected items changed; reopen the menu.", true)
                    return
                }
                var split = message.action.indexOf(":")
                var action = split < 0 ? message.action : message.action.substring(0, split)
                var subId = split < 0 ? "" : message.action.substring(split + 1)
                if (root.pane.contextMenu().validateChoice(action, subId)) {
                    if (action === "addFavourite") {
                        if (!message.paths || message.paths.length !== 1) { root.pane.message("The selected folder could not be read.", true); return }
                        root.addFavourite(message.paths[0])
                    } else root.pane.performMenu(message.action, message.id, message.paths)
                }
                return
            }
            if (root.item) root.item.receive(message)
        }
        function onFailed(where, input, message, mode) {
            if (where !== "backend") return
            root.survivorId = 0
            root.survivors = []
            root.ready = false
            root.identity = ""
            root.pendingAction = ""
            root.pendingActivation = false
            root.providersRefreshing = false
            if (root.opened || root.deleting) root.item.receive({id: root.requestId, op: root.deleting ? "delete" : "", ok: false, error: message})
        }
    }
    Connections {
        target: root.item
        // Two dialogs load here and only one of them creates files or deletes them, so the handlers
        // the other never raises are absent by design rather than misspelled.
        ignoreUnknownSignals: true
        function onRequested(message) {
            if (message.op === "openWith") root.launchingId = message.id
            root.pane.backend.send(message)
        }
        function onApproved(message) {
            root.pane.backend.send({c: "transfer", op: message.action === "moveTo" ? "move" : "copy",
                menuId: message.id, dest: message.dest})
        }
        function onCreated(path) { root.pane.refresh(path); root.pane.message("File created.", false) }
        function onDeleted(message) {
            var text = "Deleted " + message.deleted + " of " + message.count
            if (message.failed) text += " · " + message.failed + " failed"
            if (message.cancelled) text += " · cancelled"
            root.pane.operationResult(text, message.error || "", message.failed > 0)
            if (root.pane.path !== root.folder) return
            root.survivors = message.remaining || []
            root.survivorId = root.survivors.length ? message.id : 0
            root.survivorFolder = root.folder
            root.pane.refresh()
        }
        function onClosed() {
            root.ready = false
            root.identity = ""
            // The close op kills the launcher this id was waiting on, so its error is the cancellation
            // the operator asked for and not something to raise at them.
            root.launchingId = 0
        }
    }
}
