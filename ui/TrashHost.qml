import QtQuick
import "js/Focus.js" as Focus
import "js/Menu.js" as Menu

Loader {
    id: root
    required property var pane
    property Item overlayParent: null
    active: false
    visible: opened
    source: "TrashView.qml"
    readonly property bool opened: item !== null && item.opened
    readonly property bool confirming: item !== null && item.confirming
    readonly property int total: item ? item.total : 0
    readonly property int selectedCount: item ? item.selectedCount : 0

    function open(action) {
        if (confirming) return
        if (opened) { root.action(action || "openTrash"); return }
        pane.preview.close()
        pane.shareBrowser.close()
        pane.clearSelection()
        pane.focusView = Focus.LIST
        active = true
        item.open(action || "")
    }
    function close() { if (opened && !confirming) item.close() }
    function menuAt(point, selection) {
        var entries = selection ? Menu.applyHidden([
            {label: "Restore", action: "restoreTrashSelection", glyph: "undo", disabled: item.busy},
            {separator: true},
            {id: "delete", label: "Delete permanently", action: "deletePermanently", glyph: "trash", danger: true, disabled: item.busy}
        ], ViewState.menuHidden) : Menu.trashEntries(total, item.busy)
        pane.contextMenu().openForRail("trash", entries, point)
        pane.contextMenu().focusHolder = item
    }
    function action(name) {
        if (confirming) return
        if (!opened) { open(name); return }
        if (name === "restoreTrashSelection") item.restore(false)
        else if (name === "deletePermanently") item.prepare(false)
        else if (name === "restoreAll") item.restore(true)
        else if (name === "emptyTrash") item.prepare(true)
        else item.forceActiveFocus()
    }
    onLoaded: {
        item.overlayParent = root.overlayParent
        item.statusBar = root.pane.statusBar
        item.railKeyHandler = function(event) {
            if (pane.focusView !== Focus.RAIL) return false
            Focus.handleKey(event, pane, pane.sidebar)
            return true
        }
    }
    Connections {
        target: root.item
        function onRequested(message) { root.pane.backend.send(message) }
        function onBackRequested() { root.pane.focusView = Focus.LIST; root.pane.listArea.forceActiveFocus() }
        function onFocusRailRequested() { root.pane.focusView = Focus.RAIL }
        function onActionRequested(action) { root.pane.act(action) }
        function onStatusReported(message, error) { root.pane.message(message, error) }
        function onOperationResult(headline, detail, error) { root.pane.operationResult(headline, detail, error); root.pane.sidebar.refreshTrash() }
        function onContextRequested(x, y, selection) { root.menuAt(root.item.mapToItem(null, x, y), selection) }
    }
    Connections {
        target: root.pane.backend
        function onTrashResult(message) { if (root.item) root.item.receive(message) }
    }
    Connections {
        target: root.pane.sidebar
        function onTrashChanged() { if (root.opened) root.item.sourceChanged() }
    }
    Connections {
        target: root.pane.contextMenu()
        function onRailChosen(action, key) { if (key === "trash") root.action(action) }
    }
}
