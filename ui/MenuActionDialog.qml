import QtQuick
import qs.Commons
import "." as Flea
import "js/Format.js" as Format

// Menu-only actions share the existing card, field and button language.
FocusScope {
    id: root
    anchors.fill: parent
    visible: opened
    property bool opened: false
    property string action: ""
    property int requestId: 0
    property string folder: ""
    property var facts: ({})
    property string errorText: ""
    property bool busy: false
    property bool committing: false
    property bool checkPending: false
    property Item focusHolder: null
    readonly property bool inputAction: action === "newFile" || action === "moveTo" || action === "copyTo"
    readonly property bool deletionActive: action === "deletePermanently" && committing
    readonly property bool canSubmit: !busy && inputAction && field.text.length > 0
    readonly property string title: ({moveTo: "Move to", copyTo: "Copy to", properties: "Properties", newFile: "New File", deletePermanently: "Delete permanently"})[action] || ""
    readonly property var cardItem: card
    readonly property var confirmationItem: confirmation
    readonly property var closeItem: closeFocus
    readonly property var submitItem: submitFocus
    readonly property var fieldItem: field
    signal requested(var message)
    signal approved(var message)
    signal created(string path)
    signal deleted(var message)
    signal closed()

    function open(operation, identity, parentPath, holder) {
        action = operation
        requestId = identity
        folder = parentPath
        focusHolder = holder
        facts = ({})
        errorText = ""
        checkPending = false
        confirmation.close()
        field.text = operation === "newFile" ? "New File" : parentPath
        busy = operation === "properties" || operation === "deletePermanently"
        committing = false
        opened = true
        body.contentY = 0
        if (inputAction) { field.forceActiveFocus(); field.selectAll() }
        else closeFocus.forceActiveFocus()
        if (busy) requested({c: "menuaction", op: operation === "deletePermanently" ? "prepareDelete" : "properties", id: requestId})
    }
    // Two dialogs share the one request id, so this one answers only for the ops it asked for:
    // routing an Open with reply in here closed a Properties card that had just opened over it.
    readonly property var ownedOps: ["properties", "prepareDelete", "refreshDelete", "checkDelete",
                                     "delete", "validate", "newFile", ""]
    function receive(message) {
        if ((!opened && !deletionActive) || message.id !== requestId || message.op === "close") return
        if (root.ownedOps.indexOf(message.op === undefined ? "" : message.op) < 0) return
        if (deletionActive && message.op !== "delete") return
        busy = false
        committing = false
        if (!message.ok) {
            confirmation.close()
            opened = true
            closeFocus.forceActiveFocus()
            errorText = message.error || "The requested action failed."
            return
        }
        if (message.stale || message.op === "checkDelete" && !message.valid) { refreshDeletion(); return }
        if (message.op === "prepareDelete" || message.op === "refreshDelete") {
            facts = message
            if (checkPending) { checkPending = false; refreshDeletion() }
            else confirmation.open(message)
            return
        }
        if (message.op === "checkDelete") {
            if (checkPending) { checkPending = false; refreshDeletion() }
            return
        }
        if (message.op === "delete") { deleted(Object.assign({count: facts.count}, message)); finish(); return }
        if (message.op === "properties") { facts = message; return }
        if (message.op === "validate") { approved(message); close(); return }
        if (message.op === "newFile") created(message.path)
        close()
    }
    function close() {
        if (!opened || busy && committing && action === "newFile") return
        finish()
    }
    function finish() {
        requested({c: "menuaction", op: "close", id: requestId})
        confirmation.close()
        opened = false
        closed()
        if (focusHolder) focusHolder.forceActiveFocus()
    }
    function refreshDeletion() {
        confirmation.close()
        opened = true
        busy = true
        errorText = ""
        closeFocus.forceActiveFocus()
        requested({c: "menuaction", op: "refreshDelete", id: requestId})
    }
    function checkDeletion() {
        if (!opened || action !== "deletePermanently" || errorText) return
        if (busy) { checkPending = true; return }
        busy = true
        requested({c: "menuaction", op: "checkDelete", id: requestId, token: facts.token})
    }
    function sourceChanged() {
        if (!opened || action !== "deletePermanently" || errorText) return
        confirmation.close()
        if (busy) checkPending = true
        else refreshDeletion()
    }
    function submit() {
        if (!canSubmit) return
        if (action !== "newFile" && field.text.charAt(0) !== "/") {
            errorText = "Enter an absolute destination folder."
            return
        }
        if (action === "newFile" && (field.text === "." || field.text === ".." || field.text.indexOf("/") >= 0)) {
            errorText = "Enter one filename without a path separator."
            return
        }
        if (action === "newFile") body.forceActiveFocus()
        else closeFocus.forceActiveFocus()
        busy = true
        committing = true
        errorText = ""
        if (action === "newFile") requested({c: "newfile", op: "newFile", id: requestId, path: folder, name: field.text})
        else requested({c: "menuaction", op: "validate", id: requestId, action: action, dest: field.text})
    }
    // Each focusable child routes Tab here before Qt can move focus outside the card.
    function stepFocus(back) {
        var items = [field, closeFocus, submitFocus].filter(function(item) {
            return item.visible && item.enabled && item.activeFocusOnTab
        })
        if (!items.length) return
        var current = -1
        for (var i = 0; i < items.length; i++) if (items[i].activeFocus) current = i
        var next = current < 0 ? (back ? items.length - 1 : 0)
            : (current + (back ? -1 : 1) + items.length) % items.length
        items[next].forceActiveFocus()
    }
    Keys.onTabPressed: function(event) { root.stepFocus((event.modifiers & Qt.ShiftModifier) !== 0); event.accepted = true }
    Keys.onBacktabPressed: function(event) { root.stepFocus(true); event.accepted = true }
    Keys.onEscapePressed: function(event) { root.close(); event.accepted = true }
    Keys.onPressed: function(event) { event.accepted = true }
    Rectangle {
        anchors.fill: parent
        visible: !confirmation.opened
        color: Theme.color.background
        opacity: 0.5
        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onClicked: root.close()
            onWheel: function(wheel) { wheel.accepted = true }
        }
    }
    Rectangle {
        id: card
        visible: !confirmation.opened
        anchors.centerIn: parent
        width: Math.max(0, Math.min(Theme.space(420) * Theme.dialogWidthRatio, root.width - 2 * Theme.spacing.gap))
        height: Math.max(0, Math.min(body.wanted + 2 * Theme.spacing.rowPaddingX, root.height - 2 * Theme.spacing.gap))
        color: Theme.color.surface
        border.color: Theme.color.muted
        border.width: Theme.spacing.hairline
        radius: Style.cornerRadius
        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onWheel: function(wheel) { wheel.accepted = true }
        }
        Flea.CardScroll {
            id: body
            anchors.fill: parent
            anchors.margins: Theme.spacing.rowPaddingX
            Column {
                width: body.width
                spacing: Theme.spacing.gap
                Text {
                    width: parent.width
                    text: root.title
                    textFormat: Text.PlainText
                    color: Theme.color.foreground
                    font { family: Theme.font.family; pixelSize: Theme.font.body; bold: true }
                }
                Rectangle { width: parent.width; height: Theme.spacing.hairline; color: Theme.color.muted; opacity: 0.4 }
                Text {
                    width: parent.width
                    visible: root.inputAction
                    text: root.action === "newFile" ? "Name" : "Destination folder"
                    textFormat: Text.PlainText
                    color: Theme.color.foreground
                    font { family: Theme.font.family; pixelSize: Theme.font.body }
                }
                Rectangle {
                    visible: root.inputAction
                    width: parent.width
                    height: Theme.rowHeight
                    color: Theme.color.background
                    border.color: field.activeFocus ? Theme.color.accent : Theme.color.muted
                    border.width: Theme.spacing.hairline
                    TextInput {
                        id: field
                        anchors.fill: parent
                        anchors.margins: Theme.spacing.gap
                        color: Theme.color.foreground
                        selectionColor: Theme.color.accent
                        selectedTextColor: Theme.color.background
                        font { family: Theme.font.family; pixelSize: Theme.font.body }
                        activeFocusOnTab: root.inputAction
                        enabled: !root.busy
                        clip: true
                        selectByMouse: true
                        Accessible.name: root.action === "newFile" ? "Filename" : "Destination folder"
                        Keys.onTabPressed: function(event) { root.stepFocus((event.modifiers & Qt.ShiftModifier) !== 0) }
                        Keys.onBacktabPressed: root.stepFocus(true)
                        Keys.onReturnPressed: root.submit()
                        Keys.onEnterPressed: root.submit()
                    }
                }
                Repeater {
                    model: root.action === "properties" && root.facts.ok ? [
                        ["Path", root.facts.path],
                        ["Type", root.facts.kind],
                        ["Size", Format.size(root.facts.bytes) + " (" + root.facts.bytes + " bytes)"],
                        ["Modified", Format.date(root.facts.modified, Date.now())],
                        ["Permissions", root.facts.mode],
                        ["Owner", (root.facts.owner || "Unknown") + " (uid " + root.facts.uid + ")"],
                        ["Group", "gid " + root.facts.gid]
                    ].concat(root.facts.symlink ? [["Link target", root.facts.target]] : []) : []
                    Column {
                        required property var modelData
                        width: body.width
                        Text { width: parent.width; text: parent.modelData[0]; textFormat: Text.PlainText; color: Theme.color.muted; font { family: Theme.font.family; pixelSize: Theme.font.caption } }
                        Text { width: parent.width; text: parent.modelData[1]; textFormat: Text.PlainText; wrapMode: Text.WrapAnywhere; color: Theme.color.foreground; font { family: Theme.font.family; pixelSize: Theme.font.body } }
                    }
                }
                Text {
                    width: parent.width
                    visible: text.length > 0
                    text: root.errorText || (root.busy ? "Working..." : "")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: root.errorText ? Theme.color.error : Theme.color.muted
                    font { family: Theme.font.family; pixelSize: Theme.font.caption }
                }
                Row {
                    anchors.right: parent.right
                    spacing: Theme.spacing.gap
                    FocusScope {
                        id: closeFocus
                        width: closeButton.implicitWidth
                        height: closeButton.implicitHeight
                        activeFocusOnTab: !(root.busy && root.committing && root.action === "newFile")
                        Keys.onTabPressed: function(event) { root.stepFocus((event.modifiers & Qt.ShiftModifier) !== 0) }
                        Keys.onBacktabPressed: root.stepFocus(true)
                        Keys.onReturnPressed: root.close()
                        Keys.onSpacePressed: root.close()
                        Flea.DialogButton { id: closeButton; label: root.action === "properties" ? "Close" : "Cancel"; primary: parent.activeFocus; available: parent.activeFocusOnTab; onActivated: root.close() }
                    }
                    FocusScope {
                        id: submitFocus
                        width: submitButton.implicitWidth
                        height: submitButton.implicitHeight
                        visible: root.action !== "properties" && root.action !== "deletePermanently"
                        activeFocusOnTab: root.canSubmit
                        Keys.onTabPressed: function(event) { root.stepFocus((event.modifiers & Qt.ShiftModifier) !== 0) }
                        Keys.onBacktabPressed: root.stepFocus(true)
                        Keys.onReturnPressed: root.submit()
                        Keys.onSpacePressed: root.submit()
                        Flea.DialogButton { id: submitButton; label: root.action === "newFile" ? "Create" : root.action === "moveTo" ? "Move" : "Copy"; primary: parent.activeFocus; available: root.canSubmit; onActivated: root.submit() }
                    }
                }
            }
        }
    }
    // Match Trash's identity-check cadence only while the destructive strip is visible.
    Timer { interval: 2500; repeat: true; running: confirmation.opened && !root.busy; onTriggered: root.checkDeletion() }
    Flea.TrashConfirm {
        id: confirmation
        scopeName: "items"
        onCancelled: root.close()
        onConfirmed: function(token) {
            root.busy = true
            root.committing = true
            root.opened = false
            if (root.focusHolder) root.focusHolder.forceActiveFocus()
            root.requested({c: "menuaction", op: "delete", id: root.requestId, token: token})
        }
    }
}
