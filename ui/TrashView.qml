import QtQuick
import Quickshell
import qs.Commons
import "." as Flea
import "js/Format.js" as Format
import "js/Icons.js" as Icons
import "js/Keymap.js" as Keymap
import "js/TrashDates.js" as Trash
import "js/Trash.js" as TrashKeys

// Trash keeps the normal integer ListView model, holding metadata only for its current window.
FocusScope {
    id: root
    property bool opened: false
    property Item overlayParent: null
    property var railKeyHandler: null
    property var statusBar: null
    property int total: 0
    property real totalBytes: 0
    property bool bytesReady: false
    property bool bytesPartial: false
    property int first: 0
    property var rows: []
    property var selected: ({})
    property var selectionIdentities: ({})
    property bool allSelected: false
    property int selectionToken: 0
    property int selectionCount: 0
    property int cursor: 0
    property double trashArmedAt: 0
    property int requestId: 0
    property string pendingOp: ""
    property bool operationActive: false
    readonly property bool busy: pendingOp.length > 0
    property bool refreshPending: false
    property bool confirming: false
    property string initialAction: ""
    property int pendingSelection: -1
    property var pendingExtend: false
    property string errorText: ""
    property bool confirmingAll: false
    property var confirmingUris: []
    property var confirmingExcluded: []
    property bool confirmingSelectionAll: false
    property int confirmingSelectionToken: 0
    property var confirmingIdentities: []
    property int confirmingToken: 0
    readonly property bool confirmationOpen: confirmation.opened
    readonly property var confirmationItem: confirmation
    readonly property var backItem: backButton
    readonly property var upItem: upButton
    readonly property string countText: countLabel.text
    readonly property var headerLabels: [nameTitle.text, locationTitle.text, deletedTitle.text]
    function rowItemFor(index) { return listing.itemAtIndex(index) }
    readonly property int windowRows: Math.max(1, Math.min(350, Math.ceil(listing.height / Theme.fileRowHeight) + 2))
    readonly property int selectedCount: allSelected ? Math.max(0, selectionCount - excludedUris().length) + selectedUris().length : selectedUris().length
    readonly property string home: Quickshell.env("HOME") || ""
    signal requested(var message)
    signal backRequested()
    signal focusRailRequested()
    signal actionRequested(string action)
    signal contextRequested(real x, real y, bool hasSelection)
    signal statusReported(string message, bool error)
    signal operationResult(string headline, string detail, bool error)
    signal countChanged(int count)
    anchors.fill: parent
    visible: opened
    onActiveFocusChanged: if (!activeFocus) root.trashArmedAt = 0

    function send(op, fields) {
        var message = fields || {}
        message.c = "trashbrowse"
        message.op = op
        message.id = ++requestId
        pendingOp = op
        if (op === "restore" || op === "delete") operationActive = true
        requested(message)
    }
    function open(action) {
        trashArmedAt = 0
        if (operationActive) { opened = true; forceActiveFocus(); return }
        opened = true; selected = ({}); selectionIdentities = ({}); allSelected = false; selectionToken = 0; selectionCount = 0; cursor = 0
        first = 0; rows = []; total = 0; errorText = ""; bytesReady = false
        confirming = false; confirmation.close(); refreshPending = false
        initialAction = action || ""
        send("list", {start: 0, count: windowRows, recover: true})
        forceActiveFocus()
    }
    function close() {
        trashArmedAt = 0
        opened = false; confirming = false; confirmation.close(); initialAction = ""
        if (!operationActive) send("cancel", {clearSelection: true})
        backRequested()
    }
    function refresh() {
        if (!opened) return
        if (busy) { refreshPending = true; return }
        if (confirming) { sourceChanged(); return }
        refreshPending = false
        bytesReady = false
        send("list", {start: first, count: windowRows, uris: Object.keys(selected)})
    }
    function sourceChanged() {
        if (!opened) return
        trashArmedAt = 0
        if (confirmation.opened) confirmation.close()
        if (busy) { refreshPending = true; return }
        refreshPending = false
        if (confirming) send("prepare", {all: confirmingSelectionAll, emptyTrash: confirmingAll, uris: confirmingUris, exclude: confirmingExcluded, identities: confirmingIdentities, selectionToken: confirmingSelectionToken, refreshToken: confirmingToken})
        else refresh()
    }
    function ensureWindow() {
        if (!opened || total === 0 || busy || confirmation.opened) return
        var top = Math.max(0, Math.floor((listing.contentY - listing.originY) / Theme.fileRowHeight))
        var end = Math.min(total, top + Math.ceil(listing.height / Theme.fileRowHeight))
        if (top < first || end > first + rows.length)
            send("window", {start: top, count: windowRows})
    }
    function rowAt(index) { return rows[index - first] || null }
    function selectedUris() { return Object.keys(selected).filter(function(uri) { return selected[uri] }) }
    function excludedUris() { return Object.keys(selected).filter(function(uri) { return selected[uri] === false }) }
    function isSelected(uri) {
        if (selected[uri] !== undefined) return selected[uri] === true
        if (!allSelected) return false
        for (var i = 0; i < rows.length; i++) if (rows[i].uri === uri) return rows[i].bulkSelected === true
        return false
    }
    function selectAll() {
        if (operationActive || confirming || pendingOp === "select" || total === 0) return
        allSelected = false; selectionToken = 0; selectionCount = 0
        selected = ({}); selectionIdentities = ({})
        send("select")
    }
    function choose(index, extend) {
        trashArmedAt = 0
        if (total === 0 || confirming || operationActive) return
        if (pendingOp === "select") send("cancel", {clearSelection: true})
        cursor = Math.max(0, Math.min(total - 1, index))
        listing.positionViewAtIndex(cursor, ListView.Contain)
        var item = rowAt(cursor)
        if (!item) {
            pendingSelection = cursor; pendingExtend = extend
            ensureWindow(); return
        }
        pendingSelection = -1
        var next = extend ? Object.assign({}, selected) : ({})
        var identities = extend ? Object.assign({}, selectionIdentities) : ({})
        next[item.uri] = extend === "add" ? true : extend ? !isSelected(item.uri) : true
        if (next[item.uri]) identities[item.uri] = item.identity
        else delete identities[item.uri]
        if (allSelected && extend && next[item.uri] && item.bulkSelected) {
            delete next[item.uri]
            delete identities[item.uri]
        }
        if (!extend) { allSelected = false; selectionToken = 0; selectionCount = 0 }
        selected = next
        selectionIdentities = identities
        forceActiveFocus()
    }
    function extendSelection(direction) {
        var previous = cursor
        choose(previous, "add")
        choose(previous + direction, "add")
    }
    function restore(all) {
        if (busy || (all ? total === 0 : selectedCount === 0)) return
        confirming = false
        var uris = selectedUris()
        send("restore", {all: all, selectionToken: !all && allSelected ? selectionToken : 0, uris: uris, exclude: all ? [] : excludedUris(), identities: uris.map(function(uri) { return root.selectionIdentities[uri] || "" })})
    }
    function prepare(all) {
        if (busy || (all ? total === 0 : selectedCount === 0)) return
        confirming = true
        confirmingToken = 0
        confirmingAll = all
        confirmingSelectionAll = all
        confirmingSelectionToken = !all && allSelected ? selectionToken : 0
        confirmingUris = selectedUris()
        confirmingExcluded = all ? [] : excludedUris()
        confirmingIdentities = confirmingUris.map(function(uri) { return root.selectionIdentities[uri] || "" })
        send("prepare", {all: confirmingSelectionAll, emptyTrash: all, uris: confirmingUris, exclude: confirmingExcluded, identities: confirmingIdentities, selectionToken: confirmingSelectionToken})
    }
    function armDelete() {
        if (selectedCount === 0 || busy || confirming) { trashArmedAt = 0; return }
        var now = Date.now()
        var paired = trashArmedAt > 0 && now - trashArmedAt < TrashKeys.ARM_MS
        trashArmedAt = paired ? 0 : now
        if (paired) prepare(false)
        else statusReported("Press d again to review permanent deletion, or Delete on its own.", false)
    }
    function receive(message) {
        if ((!opened && !operationActive) || message.id !== requestId || message.op !== pendingOp) return
        pendingOp = ""
        if (!message.ok) {
            if (message.stale && opened) { operationActive = false; confirming = true; sourceChanged(); return }
            errorText = message.error || "Trash operation failed."
            confirming = false; confirmation.close()
            statusReported(errorText, true)
            operationActive = false
            return
        }
        if (message.op === "list" || message.op === "window") {
            total = message.total
            countChanged(total)
            first = message.start
            rows = message.rows || []
            if (allSelected) {
                selectionCount = message.selectionCount || 0
                if (message.selectionStale || message.selectionToken !== selectionToken) {
                    allSelected = false; selectionToken = 0; selectionCount = 0
                    selected = ({}); selectionIdentities = ({})
                    statusReported("Trash selection changed; select the items again.", true)
                }
            }
            cursor = Math.max(0, Math.min(cursor, total - 1))
            if (message.op === "list") {
                if ((message.recoveryErrors || []).length > 0)
                    operationResult("Interrupted file operation needs attention", message.recoveryErrors.join("\n"), true)
                else if (message.recoveredCount > 0)
                    operationResult("Recovered " + message.recoveredCount + " interrupted file operations", "", false)
                errorText = ""
                var kept = ({})
                var keptIdentities = ({})
                var present = message.present || []
                for (var p = 0; p < present.length; p++) {
                    kept[present[p]] = selected[present[p]]
                    if (selectionIdentities[present[p]]) keptIdentities[present[p]] = selectionIdentities[present[p]]
                }
                selected = kept
                selectionIdentities = keptIdentities
                if (total === 0) allSelected = false
                var action = initialAction; initialAction = ""
                if (action === "emptyTrash") prepare(true)
                else if (action === "restoreAll") restore(true)
                else send("summary")
            }
            if (pendingSelection >= 0 && rowAt(pendingSelection)) choose(pendingSelection, pendingExtend)
        } else if (message.op === "select") {
            selectionToken = message.selectionToken || 0
            selectionCount = message.selectionCount || 0
            allSelected = selectionToken > 0
            send("window", {start: first, count: windowRows})
        } else if (message.op === "summary") {
            totalBytes = message.bytes
            bytesPartial = message.partial
            bytesReady = true
        } else if (message.op === "prepare" && confirming) {
            confirmingToken = message.token
            if (!refreshPending) confirmation.open(message)
        }
        else if (message.op === "check" && !message.valid) {
            sourceChanged()
        } else if (message.op === "cancel") {
            refresh()
        } else if (message.op === "restore" || message.op === "delete") {
            var failed = message.failed || 0
            var next = ({})
            var failures = message.failures || []
            var identities = ({})
            for (var i = 0; i < failures.length; i++) {
                next[failures[i].uri] = true
                if (failures[i].identity) identities[failures[i].uri] = failures[i].identity
            }
            selectionIdentities = identities
            allSelected = false; selectionToken = 0; selectionCount = 0; selected = next
            var text = (message.op === "restore" ? "Restored " : "Deleted ") + message.done + " of " + (message.done + failed)
            if (failed) text += " · " + failed + " failed"
            var detail = failures.map(function(item) { return (item.name || item.uri) + " failed: " + item.error }).join("\n")
            operationResult(text, detail, failed > 0)
            refresh()
            operationActive = false
        }
        if (refreshPending && !busy) sourceChanged()
        else if (!busy) ensureWindow()
    }
    Keys.onPressed: function(event) {
        if (root.railKeyHandler && root.railKeyHandler(event)) { event.accepted = true; return }
        var action = Keymap.lookup(event.key, event.text, event.modifiers, "listing", "gui")
        if (action === "trashArm" && event.isAutoRepeat) { event.accepted = true; return }
        if (action !== "trashArm") root.trashArmedAt = 0
        var unmodified = (event.modifiers & (Qt.ControlModifier | Qt.ShiftModifier | Qt.AltModifier | Qt.MetaModifier)) === 0
        if (action === "escape" && root.statusBar && root.statusBar.escapePressed()) { event.accepted = true; return }
        if (action === "escape" || (event.key === Qt.Key_Backspace && unmodified)) root.close()
        else if (action === "cursorDown") root.choose(root.cursor + 1, false)
        else if (action === "cursorUp") root.choose(root.cursor - 1, false)
        else if (action === "cursorFirst") root.choose(0, false)
        else if (action === "cursorLast") root.choose(root.total - 1, false)
        else if (action === "pageDown" || action === "pageUp") root.choose(root.cursor + (action === "pageDown" ? 1 : -1) * Math.max(1, root.windowRows - 2), false)
        else if (action === "toggleSelect") root.choose(root.cursor, true)
        else if (action === "extendDown" || action === "extendUp") root.extendSelection(action === "extendDown" ? 1 : -1)
        else if (action === "selectAll") root.selectAll()
        else if (action === "trashArm") root.armDelete()
        else if (action === "deletePermanently" || action === "trash") root.prepare(false)
        else if (action === "focusNext") root.focusRailRequested()
        else if (action === "settings" || action === "keymapSheet" || action === "quit") root.actionRequested(action)
        else if (event.key === Qt.Key_F5 && unmodified) root.refresh()
        else if (action === "menu") root.contextRequested(root.width / 2, Theme.chromeHeight, root.selectedCount > 0)
        // Trash owns this focus context; ordinary filesystem actions must never reach the covered pane.
        event.accepted = true
    }
    Rectangle { anchors.fill: parent; color: Theme.color.background }
    Column {
        anchors.fill: parent
        spacing: 0
        Rectangle {
            width: parent.width
            height: Theme.chromeHeight
            color: Theme.color.surface
            Row {
                anchors.fill: parent
                anchors.leftMargin: Theme.spacing.rowPaddingX
                anchors.rightMargin: Theme.spacing.rowPaddingX
                spacing: Theme.spacing.gap
                Flea.Glyph {
                    id: backButton
                    width: Theme.hitMin; height: parent.height; maxSize: Theme.chromeMarkSize
                    name: "arrow-left"; color: Theme.color.foreground
                    Accessible.role: Accessible.Button
                    Accessible.name: "Back"
                    Accessible.onPressAction: root.close()
                    TapHandler { onTapped: root.close() }
                }
                Flea.Glyph {
                    id: upButton
                    width: Theme.hitMin; height: parent.height; maxSize: Theme.chromeMarkSize
                    name: "arrow-up"; color: Theme.color.muted
                    Accessible.role: Accessible.Button
                    Accessible.name: "Up unavailable in Trash"
                    Accessible.ignored: false
                    enabled: false
                }
                Text {
                    width: Math.max(0, parent.width - 2 * Theme.hitMin - countLabel.width - emptyAction.width - 4 * parent.spacing)
                    anchors.verticalCenter: parent.verticalCenter
                    text: "Trash"
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    color: Theme.color.foreground
                    font { family: Theme.font.family; pixelSize: Theme.font.caption }
                }
                Text {
                    id: countLabel
                    width: Math.min(implicitWidth, parent.width / 2)
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.total + (root.total === 1 ? " item" : " items") + (root.bytesReady ? " · " + (root.bytesPartial ? "≥ " : "") + Format.size(root.totalBytes) : "")
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    color: Theme.color.foreground
                    font { family: Theme.font.family; pixelSize: Theme.font.caption }
                }
                // Emptying the Trash was reachable only by right-clicking the rail row. It addresses
                // the whole Trash, which is what the count beside it describes, so it belongs here.
                // It opens the confirmation the menu row opens: the boundary is unchanged and no key
                // is bound to it. Disabled exactly where ui/js/Menu.js disables the row.
                Flea.ChromeAction {
                    id: emptyAction
                    anchors.verticalCenter: parent.verticalCenter
                    label: "Empty Trash"
                    role: "error"
                    available: root.total > 0 && !root.busy
                    onActivated: root.prepare(true)
                }
            }
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: Theme.spacing.hairline; color: Theme.color.foreground; opacity: 0.12 }
        }
        Row {
            width: parent.width
            height: Theme.chromeHeight
            leftPadding: Theme.spacing.rowPaddingX
            rightPadding: Theme.spacing.rowPaddingX
            spacing: Theme.spacing.gap
            Item { width: Theme.markSize; height: 1 }
            Text {
                id: nameTitle
                width: Math.max(0, parent.width - Theme.markSize - locationTitle.width - deletedTitle.width - 3 * parent.spacing - parent.leftPadding - parent.rightPadding)
                anchors.verticalCenter: parent.verticalCenter
                text: "Name"
                textFormat: Text.PlainText
                elide: Text.ElideRight
                color: Theme.color.foreground
                font { family: Theme.font.family; pixelSize: Theme.font.caption }
            }
            // TrashSidebar's fixed columns are 210/110 at bodySmall 13, then clamp to preserve the name.
            Text {
                id: locationTitle
                width: Math.min(Math.round(210 * Theme.font.bodySmall / 13), root.width * 0.35)
                anchors.verticalCenter: parent.verticalCenter
                horizontalAlignment: Text.AlignRight
                text: "Original location"
                textFormat: Text.PlainText
                elide: Text.ElideRight
                color: Theme.color.foreground
                font { family: Theme.font.family; pixelSize: Theme.font.caption }
            }
            Text {
                id: deletedTitle
                width: Math.min(Math.round(110 * Theme.font.bodySmall / 13), root.width * 0.2)
                anchors.verticalCenter: parent.verticalCenter
                horizontalAlignment: Text.AlignRight
                text: "Deleted"
                textFormat: Text.PlainText
                elide: Text.ElideRight
                color: Theme.color.foreground
                font { family: Theme.font.family; pixelSize: Theme.font.caption }
            }
        }
        ListView {
            id: listing
            width: parent.width
            height: Math.max(0, parent.height - 2 * Theme.chromeHeight)
            model: root.opened ? root.total : 0
            clip: true
            reuseItems: true
            boundsBehavior: Flickable.StopAtBounds
            onContentYChanged: root.ensureWindow()
            onHeightChanged: root.ensureWindow()
            TapHandler {
                acceptedButtons: Qt.RightButton
                onTapped: function(point) {
                    if (listing.indexAt(point.position.x, point.position.y + listing.contentY) >= 0) return
                    var at = root.mapFromItem(listing, point.position.x, point.position.y)
                    root.contextRequested(at.x, at.y, root.selectedCount > 0)
                }
            }
            Flea.FastScrollHandler { flickable: listing }
            delegate: Rectangle {
                id: itemRow
                required property int index
                readonly property var item: root.rowAt(index)
                readonly property bool selected: item && root.isSelected(item.uri)
                width: listing.width
                height: Theme.fileRowHeight
                color: selected ? Qt.alpha(Theme.color.accent, 0.14) : hover.hovered ? Style.hoverFill : "transparent"
                Rectangle { width: Theme.spacing.hairline * 2; height: parent.height; visible: itemRow.selected; color: Theme.color.accent }
                Row {
                    anchors.fill: parent
                    anchors.leftMargin: Theme.spacing.rowPaddingX
                    anchors.rightMargin: Theme.spacing.rowPaddingX
                    spacing: Theme.spacing.gap
                    Flea.Glyph { width: Theme.markSize; height: parent.height; name: itemRow.item ? (itemRow.item.directory ? "folder" : Icons.glyphFor(itemRow.item.icon)) : "file"; color: Theme.color.foreground }
                    Text { width: Math.max(0, parent.width - Theme.markSize - original.width - deleted.width - 3 * parent.spacing); anchors.verticalCenter: parent.verticalCenter; text: itemRow.item ? itemRow.item.original.split("/").pop() : ""; textFormat: Text.PlainText; elide: Text.ElideRight; color: Theme.color.foreground; font { family: Theme.font.family; pixelSize: Theme.font.body } }
                    Text { id: original; width: locationTitle.width; anchors.verticalCenter: parent.verticalCenter; text: itemRow.item ? Trash.location(itemRow.item.original, root.home) : ""; textFormat: Text.PlainText; elide: Text.ElideLeft; horizontalAlignment: Text.AlignRight; color: Theme.color.foreground; font { family: Theme.font.family; pixelSize: Theme.font.body } }
                    Text { id: deleted; width: deletedTitle.width; anchors.verticalCenter: parent.verticalCenter; text: itemRow.item ? Trash.deleted(itemRow.item.deleted, Date.now()) : ""; textFormat: Text.PlainText; elide: Text.ElideRight; horizontalAlignment: Text.AlignRight; color: Theme.color.foreground; font { family: Theme.font.family; pixelSize: Theme.font.body } }
                }
                HoverHandler { id: hover }
                Accessible.role: Accessible.ListItem
                Accessible.name: item ? item.original.split("/").pop() : ""
                Accessible.selected: selected
                Accessible.onPressAction: root.choose(itemRow.index, false)
                MouseArea { anchors.fill: parent; acceptedButtons: Qt.LeftButton; onClicked: function(mouse) { root.choose(itemRow.index, mouse.modifiers & Qt.ControlModifier) } }
                TapHandler { acceptedButtons: Qt.RightButton; onTapped: function(point) { if (!itemRow.selected) root.choose(itemRow.index, false); var at = root.mapFromItem(itemRow, point.position.x, point.position.y); root.contextRequested(at.x, at.y, true) } }
            }
        }
    }
    Rectangle { y: 2 * Theme.chromeHeight - height; width: parent.width; height: Theme.spacing.hairline; color: Theme.color.foreground; opacity: 0.12 }
    Text { anchors.centerIn: parent; visible: root.total === 0; text: root.errorText || (root.busy ? "Reading Trash…" : "Trash is empty"); textFormat: Text.PlainText; color: root.errorText ? Theme.color.error : Theme.color.muted; font { family: Theme.font.family; pixelSize: Theme.font.body } }
    // GIO's monitor reports top-level changes; the active confirmation also checks nested identities.
    Timer { interval: 2500; repeat: true; running: root.opened && confirmation.opened && !root.busy; onTriggered: root.send("check", {token: confirmation.snapshot.token}) }
    Flea.TrashConfirm {
        id: confirmation
        parent: root.overlayParent || root
        z: 5
        onConfirmed: function(token) { root.confirming = false; root.send("delete", {token: token}); root.forceActiveFocus() }
        onCancelled: { root.confirming = false; root.send("cancel"); root.forceActiveFocus() }
    }
}
