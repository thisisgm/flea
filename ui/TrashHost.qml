import QtQuick
import "js/Focus.js" as Focus
import "js/Menu.js" as Menu
import "js/TrashDates.js" as TrashDates

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

    // The 30 day sweep, GM's ruling of 2026-09-11. It runs the same three requests the window runs,
    // in the same order, so prepare still reviews every item against the identity the listing
    // reported and delete still refuses a token the trash has moved under. This host drives it
    // because the backend keeps ONE Trash session: two clients listing into it would take each
    // other's snapshot away, so the window and the sweep are never in flight together.
    readonly property int sweepDays: 30
    // One window of the listing, which is also the most the backend will answer with. A Trash holding
    // more than this is swept over as many days as it takes, which is slower and never wrong.
    readonly property int sweepWindow: 350
    property int sweepId: 0
    property string sweepStage: ""
    property var sweepDoomed: []
    readonly property bool sweeping: root.sweepStage.length > 0

    function sweep() {
        if (root.sweeping || root.active || !ViewState.trashAutoEmpty)
            return
        if (TrashDates.dayNumber(Date.now()) === ViewState.trashSweptOn)
            return
        root.sweepSend("list", {start: 0, count: root.sweepWindow, recover: false})
    }

    function sweepSend(op, fields) {
        var message = fields || {}
        message.c = "trashbrowse"
        message.op = op
        message.id = ++root.sweepId
        root.sweepStage = op
        root.pane.backend.send(message)
    }

    // The operator reaching for Trash outranks a sweep nobody asked to watch. Cancelling drops the
    // prepared confirmation in the backend, so the window's own list starts from a clean session.
    function sweepStop() {
        if (!root.sweeping)
            return
        root.sweepStage = ""
        root.sweepDoomed = []
        root.pane.backend.send({c: "trashbrowse", op: "cancel", id: ++root.sweepId, clearSelection: true})
    }

    // Every reply while a sweep is in flight is the sweep's, because the window cannot be open then.
    // A failure at any stage ends the sweep and records nothing, so the next launch tries again.
    function sweepReceive(message) {
        if (message.ok !== true) { root.sweepStage = ""; root.sweepDoomed = []; return }
        if (message.op === "list") {
            var rows = message.rows || []
            var now = Date.now()
            var doomed = []
            for (var i = 0; i < rows.length; i++) {
                if (TrashDates.expired(rows[i].deleted, now, root.sweepDays))
                    doomed.push(rows[i])
            }
            if (doomed.length === 0) { root.sweepFinished(); return }
            root.sweepDoomed = doomed
            root.sweepSend("prepare", {all: false, emptyTrash: false,
                uris: doomed.map(function (row) { return row.uri }),
                identities: doomed.map(function (row) { return row.identity })})
            return
        }
        if (message.op === "prepare") {
            // A prepare that reviewed nothing has nothing to delete, and asking anyway would spend a
            // token on an empty confirmation.
            if (!message.count) { root.sweepFinished(); return }
            root.sweepSend("delete", {token: message.token})
            return
        }
        if (message.op === "delete") {
            root.pane.sidebar.refreshTrash()
            root.sweepFinished()
        }
    }

    function sweepFinished() {
        root.sweepStage = ""
        root.sweepDoomed = []
        ViewState.recordTrashSweep(TrashDates.dayNumber(Date.now()))
    }

    function open(action) {
        if (confirming) return
        root.sweepStop()
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
        function onTrashResult(message) {
            if (root.sweeping) { root.sweepReceive(message); return }
            if (root.item) root.item.receive(message)
        }
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
