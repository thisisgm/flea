import QtQuick
import "." as Flea
import "js/Menu.js" as Menu
import "js/MenuContext.js" as MenuContext

// Menu state and receivers stay live; the visual warms after the first frame and listing idle.
Item {
    id: root
    required property var pane
    anchors.fill: parent
    visible: false
    readonly property var item: visual.item
    readonly property int status: visual.status
    property bool syncing: false
    property bool firstFrame: false
    readonly property bool ready: root.item !== null && visual.status === Loader.Ready
    readonly property bool settled: root.pane.listingState !== "loading" && !root.pane.listInFlight
    readonly property real inputAt: root.pane.inputAt
    onInputAtChanged: if (warm.running) warm.restart()

    signal chosen(string action)
    signal refused(string reason)
    signal noticed(string text)
    signal snapshotRequested()
    signal railChosen(string action, string key)
    signal lockedChosen(string action, string path)

    property Item focusOwner: null
    property bool showHidden: false
    property bool providersRefreshing: false
    property var taildropPeers: []
    property bool rowIsFile: false
    property bool taildropInstalled: false
    property string taildropReason: ""
    property string taildropCode: ""
    property var archiveFormats: []
    property bool canConvert: false
    property bool canExtract: false
    property string archiveName: ""
    property int rowMode: 0
    property int selectionCount: 0
    property bool cursorIsTarget: false
    property var openWithApps: []
    property bool openWithLoaded: false
    property var identitySource: null
    property bool clipboardAvailable: false
    property bool clipboardWatchFailed: false
    property bool rowIsArchive: false
    property bool rowIsImage: false
    property string rowName: ""
    property string flashZipMember: ""
    property bool rowIsSymlink: false
    property bool rowHasShebang: false
    property var selectionModes: []
    property var compareKinds: []
    property bool dropboxInstalled: false
    property var localSend: ({ installed: false, peers: [], checking: false })
    property string dropboxPath: ""
    property string dropboxReason: ""
    property string dropboxCode: ""
    property bool rowInDropbox: false
    property bool canTrash: true
    property bool dirWritable: true
    property bool canLink: true
    property var tagSubmenu: null
    property string storageClass: ""
    property bool hasFolderSort: false
    property string tileTarget: ""
    property int tileMode: 0
    property rect workArea: Qt.rect(0, 0, root.width, root.height)
    property bool opened: false
    property bool preparing: false
    property bool hasRow: true
    property bool forHeader: false
    property string openedIdentity: ""
    property string lockedPath: ""
    property int lockedMode: 0
    property var railEntries: []
    property string railKey: ""
    property Item focusHolder: null
    property int cursor: 0
    property int openSubmenuRow: -1
    property int submenuCursor: 0
    property string loneFlyoutAction: ""
    property var lastProviderAnswer: null

    readonly property bool forRail: root.railEntries.length > 0
    readonly property bool forLocked: root.lockedPath.length > 0
    readonly property bool submenuOpen: root.openSubmenuRow >= 0 || root.loneFlyoutAction.length > 0
    readonly property var entries: root.item ? root.item.entries : []
    readonly property var submenuEntries: root.item ? root.item.submenuEntries : []
    readonly property var frameItem: root.item ? root.item.frameItem : null
    readonly property var submenuFrameItem: root.item ? root.item.submenuFrameItem : null
    readonly property bool keyboardFocused: root.item ? root.item.keyboardFocused : false
    readonly property var inputNames: [
        "focusOwner", "showHidden", "providersRefreshing", "taildropPeers",
        "rowIsFile", "taildropInstalled", "taildropReason", "taildropCode",
        "archiveFormats", "canConvert", "canExtract", "archiveName",
        "rowMode", "selectionCount", "cursorIsTarget", "openWithApps",
        "openWithLoaded", "identitySource", "clipboardAvailable", "clipboardWatchFailed",
        "rowIsArchive", "rowIsImage", "rowName", "flashZipMember",
        "rowIsSymlink", "rowHasShebang", "selectionModes", "compareKinds", "dropboxInstalled",
        "localSend", "dropboxPath", "dropboxReason", "dropboxCode",
        "rowInDropbox", "canTrash", "dirWritable", "canLink",
        "tagSubmenu", "storageClass", "hasFolderSort", "tileTarget",
        "tileMode", "workArea"
    ]
    readonly property var stateNames: [
        "opened", "preparing", "hasRow", "forHeader",
        "openedIdentity", "lockedPath", "lockedMode", "railEntries",
        "railKey", "focusHolder", "cursor", "openSubmenuRow",
        "submenuCursor", "loneFlyoutAction", "lastProviderAnswer"
    ]

    function liveInput(name) { return Qt.binding(function () { return root[name] }) }
    function copyState(name, from, to) {
        if (root.syncing || !root.item) return
        root.syncing = true
        try { to[name] = from[name] } finally { root.syncing = false }
    }
    function watchState(name) {
        root[name + "Changed"].connect(function () { root.copyState(name, root, root.item) })
        root.item[name + "Changed"].connect(function () { root.copyState(name, root.item, root) })
    }
    function configure() {
        if (String(visual.source).length > 0) return
        var values = { parent: root.liveInput("parent") }
        for (var i = 0; i < root.inputNames.length; i++) values[root.inputNames[i]] = root.liveInput(root.inputNames[i])
        visual.setSource("ContextMenu.qml", values)
    }
    function loaded() {
        for (var s = 0; s < root.stateNames.length; s++) root.item[root.stateNames[s]] = root[root.stateNames[s]]
        for (var n = 0; n < root.stateNames.length; n++) root.watchState(root.stateNames[n])
        root.item.chosen.connect(root.chosen)
        root.item.refused.connect(root.refused)
        root.item.noticed.connect(root.noticed)
        root.item.snapshotRequested.connect(root.snapshotRequested)
        root.item.railChosen.connect(root.railChosen)
        root.item.lockedChosen.connect(root.lockedChosen)
    }
    function prewarm() {
        if (visual.active) return
        root.configure()
        visual.active = true
    }
    function ensure() {
        if (root.item) return root.item
        warm.stop()
        // Cancelling first preserves inputs from birth when Qt restarts unfinished compilation.
        visual.active = false
        visual.source = ""
        visual.asynchronous = false
        root.configure()
        visual.active = true
        if (!root.item) throw new Error("ContextMenu.qml did not load: Loader status " + visual.status)
        return root.item
    }

    Loader {
        id: visual
        active: false
        asynchronous: true
        onLoaded: root.loaded()
        onStatusChanged: if (status === Loader.Error) throw new Error("ContextMenu.qml did not load: Loader status " + status)
    }

    // Match Quick Look's one-second idle hold, after both the first swap and listing readiness.
    Timer {
        id: warm
        interval: 1000
        running: root.firstFrame && root.settled && !visual.active
        onTriggered: root.prewarm()
    }
    Connections {
        target: root.firstFrame ? null : root.Window.window
        function onFrameSwapped() { root.firstFrame = true }
    }

    function selectionIdentity() { return root.identitySource ? root.identitySource.menuSelectionIdentity() : "" }
    function listingContext(flyoutAction, includeHidden) {
        if (root.item) return root.item.listingContext(flyoutAction, includeHidden)
        return MenuContext.listingContext(root, flyoutAction, includeHidden, {
            scripts: Flea.Scripts.entries, hiddenActions: ViewState.menuHidden,
            langMarks: ViewState.langMarks,
            thumbPreview: ViewState.preview, updateVersion: UpdateCheck.menuVersion
        })
    }
    function buildEntries(flyoutAction, includeHidden) {
        if (root.item) return root.item.buildEntries(flyoutAction, includeHidden)
        if (root.forRail) return root.railEntries
        if (root.forHeader) return Menu.headerEntries(ViewState.hiddenCols, root.showHidden)
        return Menu.listingEntries(root.listingContext(flyoutAction, includeHidden))
    }
    function validateChoice(action, subId, includeHidden) {
        if (root.item) return root.item.validateChoice(action, subId, includeHidden)
        return MenuContext.validateChoice(root, action, subId, includeHidden)
    }
    function refuseLone(kind) {
        root.close()
        root.refused(kind === "moved" ? "Selected items changed; reopen the menu."
                                     : "That action is no longer available; reopen the menu.")
    }
    function providersSettled() {
        if (root.item) root.item.providersSettled()
        else root.lastProviderAnswer = MenuContext.settle(MenuContext.live(root))
    }
    function refreshProviderRows() { if (root.item) root.item.refreshProviderRows() }
    function close() {
        if (root.item) root.item.close()
        else root.opened = false
    }
    function clearRail() {
        if (root.item) root.item.clearRail()
        else { root.railEntries = []; root.railKey = ""; root.lockedPath = ""; root.lockedMode = 0 }
    }
    function openAt(point) { root.ensure().openAt(point) }
    function openBackground(point) { root.ensure().openBackground(point) }
    function openLocked(path, mode, point) { root.ensure().openLocked(path, mode, point) }
    function openForHeader(point) { root.ensure().openForHeader(point) }
    function openForRail(key, rows, point) { if (rows && rows.length) root.ensure().openForRail(key, rows, point) }
    function openSubmenu(index) { if (root.item) root.item.openSubmenu(index) }
    function openSubmenuFor(action) { return root.ensure().openSubmenuFor(action) }
    function choose(action) { if (root.item) root.item.choose(action) }
    function chooseSub(id) { if (root.item) root.item.chooseSub(id) }
    function setEntries(rows, flyoutRow) { root.ensure().setEntries(rows, flyoutRow) }
    function itemFor(index) { return root.item ? root.item.itemFor(index) : null }
    function submenuItemFor(index) { return root.item ? root.item.submenuItemFor(index) : null }
    function submenuGlyphs() { return root.item ? root.item.submenuGlyphs() : "" }
}
