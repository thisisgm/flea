import QtQuick
import qs.Commons
import "." as Flea
import "js/Format.js" as Format
import "js/PropertyFacts.js" as PropertyFacts
import "js/Tags.js" as Tags

// Menu-only actions share the existing card, field and button language.
FocusScope {
    id: root
    anchors.fill: parent
    visible: opened || opacity > 0
    // Closed means no input, even while the fade-out is still drawn.
    enabled: opened
    opacity: 0
    Flea.Reveal { target: root; shown: root.opened; lift: card }
    property bool opened: false
    property string action: ""
    property int requestId: 0
    property string folder: ""
    property var facts: ({})
    // A folder's walk as the card knows it, null for anything else; the Size and Contains lines read it as it moves.
    property var walk: null
    // A file's codec, bit rate or photo facts as the facts reply sent them, empty until it lands; the pairs between Size and Modified read it.
    property var extra: ({})
    property string errorText: ""
    property bool busy: false
    property bool committing: false
    property bool checkPending: false
    property Item focusHolder: null
    readonly property bool inputAction: action === "newFile" || action === "moveTo" || action === "copyTo"
    readonly property bool deletionActive: action === "deletePermanently" && committing
    readonly property bool canSubmit: !busy && inputAction && field.text.length > 0
    // The lines that change in place while the folder counts; the model names them and is not rebuilt as the count moves.
    readonly property var liveLines: facts.ok ? ({size: PropertyFacts.size(facts.bytes, walk), contains: PropertyFacts.contains(walk)}) : ({})
    readonly property var extraPairs: PropertyFacts.extraPairs(extra)
    readonly property string title: ({moveTo: "Move to", copyTo: "Copy to", properties: "Properties", newFile: "New file", deletePermanently: "Delete permanently"})[action] || ""
    // The card and its scrim step aside for the confirmation, and stay aside while it fades out with a closing dialog.
    readonly property bool cardShown: !confirmation.opened && (root.opened || !confirmation.visible)
    // Cleanup a closing reply defers until the fade ended, so the card holds still while it fades out.
    property var settle: null
    readonly property var cardItem: card
    readonly property var confirmationItem: confirmation
    readonly property var closeItem: closeButton
    readonly property var submitItem: submitButton
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
        walk = null
        extra = ({})
        facts = ({})
        errorText = ""
        checkPending = false
        confirmation.close()
        settle = null
        field.text = operation === "newFile" ? "New File" : parentPath
        busy = operation === "properties" || operation === "deletePermanently"
        committing = false
        opened = true
        body.contentY = 0
        if (inputAction) { field.forceActiveFocus(); field.selectAll() }
        else closeButton.forceActiveFocus()
        if (busy) requested({c: "menuaction", op: operation === "deletePermanently" ? "prepareDelete" : "properties", id: requestId})
    }
    // Two dialogs share the one request id, so this one answers only for the ops it asked for:
    // routing an Open with reply in here closed a Properties card that had just opened over it.
    readonly property var ownedOps: ["properties", "prepareDelete", "refreshDelete", "checkDelete",
                                     "delete", "validate", "newFile", "contents", "facts", ""]
    function receive(message) {
        if ((!opened && !deletionActive) || message.id !== requestId || message.op === "close") return
        if (root.ownedOps.indexOf(message.op === undefined ? "" : message.op) < 0) return
        if (deletionActive && message.op !== "delete") return
        // A late count after the card left Properties changes nothing; a refused one ends as the floor already counted.
        if (message.op === "contents") { if (walk) walk = PropertyFacts.next(walk, message); return }
        // A refused or late facts reply adds no pair; the card already says everything else.
        if (message.op === "facts") { if (message.ok) extra = message; return }
        // A reply that ends the card keeps its busy look until the fade ended.
        var ends = message.ok && !message.stale && (message.op === undefined || message.op === "" || message.op === "validate" || message.op === "newFile")
        if (!ends) { busy = false; committing = false }
        if (!message.ok) {
            confirmation.close()
            opened = true
            closeButton.forceActiveFocus()
            errorText = message.error || "That action could not be completed."
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
        if (message.op === "properties") {
            walk = message.directory ? PropertyFacts.begin(message) : null
            facts = message
            if (walk) requested({c: "menuaction", op: "contents", id: requestId})
            else if (message.kind === "File") requested({c: "menuaction", op: "facts", id: requestId})
            return
        }
        if (message.op === "validate") { approved(message); closeSettled(); return }
        if (message.op === "newFile") created(message.path)
        closeSettled()
    }
    // The close of a finished reply: the busy look it left is cleared once the card is gone.
    function closeSettled() {
        settle = function () { busy = false; committing = false }
        finish()
        settleNow()
    }
    function settleNow() {
        if (opacity !== 0 || settle === null) return
        var run = settle
        settle = null
        run()
    }
    onOpacityChanged: root.settleNow()
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
        closeButton.forceActiveFocus()
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
            errorText = "Enter a full folder path, starting with /."
            return
        }
        if (action === "newFile" && (field.text === "." || field.text === ".." || field.text.indexOf("/") >= 0)) {
            errorText = "Enter a name without a /."
            return
        }
        if (action === "newFile") body.forceActiveFocus()
        else closeButton.forceActiveFocus()
        busy = true
        committing = true
        errorText = ""
        if (action === "newFile") requested({c: "newfile", op: "newFile", id: requestId, path: folder, name: field.text})
        else requested({c: "menuaction", op: "validate", id: requestId, action: action, dest: field.text})
    }
    // Each focusable child routes Tab here before Qt can move focus outside the card.
    function stepFocus(back) {
        var items = [field, closeButton, submitButton].filter(function(item) {
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
    Flea.DialogScrim { visible: root.cardShown; onDismissed: root.close() }
    Rectangle {
        id: card
        visible: root.cardShown
        x: Theme.cardOrigin(root.width, width)
        y: Theme.cardOrigin(root.height, height)
        width: Theme.cardSpan(Theme.card.form, root.width - 2 * Theme.card.inset)
        height: Theme.cardSpan(body.wanted + 2 * Theme.card.padding, root.height - 2 * Theme.card.inset)
        color: Theme.color.surface
        border.color: Theme.color.mutedFrame
        border.width: Theme.spacing.hairline
        radius: Style.cornerRadius
        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onWheel: function(wheel) { wheel.accepted = true }
        }
        Flea.CardFrame {
            id: body
            bleed: Theme.focusClearance
            anchors.fill: parent
            anchors.margins: Theme.card.padding - Theme.focusClearance
            title: Flea.DialogHeader { width: parent.width; text: root.title }
            buttons: Row {
                anchors.right: parent.right
                spacing: Theme.spacing.gap
                Flea.DialogButton {
                    id: closeButton
                    label: root.action === "properties" ? "Close" : "Cancel"
                    available: !(root.busy && root.committing && root.action === "newFile")
                    tabHandle: true
                    tabStop: true
                    onTabbed: function(from, back) { root.stepFocus(back) }
                    onActivated: root.close()
                }
                Flea.DialogButton {
                    id: submitButton
                    visible: root.action !== "properties" && root.action !== "deletePermanently"
                    label: root.action === "newFile" ? "Create" : root.action === "moveTo" ? "Move" : "Copy"
                    primary: true
                    available: root.canSubmit
                    tabHandle: true
                    tabStop: true
                    onTabbed: function(from, back) { root.stepFocus(back) }
                    onActivated: root.submit()
                }
            }
            Column {
                width: parent.width
                spacing: Theme.spacing.gap
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
                    height: Theme.rowHeight - Theme.spacing.rowPaddingY
                    color: Theme.color.background
                    border.color: field.activeFocus ? Theme.color.accent : Theme.color.mutedFrame
                    border.width: Theme.spacing.hairline
                    TextInput {
                        id: field
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.leftMargin: Theme.spacing.gap
                        anchors.rightMargin: Theme.spacing.gap
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
                        ["Size", "", "size"],
                        ...(root.facts.directory ? [["Contains", "", "contains"]] : []),
                        ...(root.extraPairs || []),
                        // A foreign word stays untouched and shows here, so a tag another app set is visible.
                        ...((root.facts.tags || []).length > 0 ? [["Tags", (root.facts.tags || []).join(", ")]] : []),
                        ["Modified", Format.date(root.facts.modified)],
                        ["Permissions", root.facts.mode],
                        ["Owner", (root.facts.owner || "Unknown") + " (uid " + root.facts.uid + ")"],
                        ["Group", "gid " + root.facts.gid],
                        ...(root.facts.symlink ? [["Link target", root.facts.target]] : [])
                    ] : []
                    Column {
                        id: pair
                        required property var modelData
                        // The Tags value is drawn as swatch and name per tag, so its plain string stays only as the model's text.
                        readonly property bool tagged: modelData[0] === "Tags"
                        width: body.holderWidth
                        Text { width: parent.width; text: parent.modelData[0]; textFormat: Text.PlainText; color: Theme.color.muted; font { family: Theme.font.family; pixelSize: Theme.font.caption } }
                        Text {
                            readonly property var line: parent.modelData.length > 2 ? root.liveLines[parent.modelData[2]] : null
                            visible: !pair.tagged
                            width: parent.width
                            text: line ? PropertyFacts.styled(line, Theme.color.muted) : parent.modelData[1]
                            textFormat: line ? Text.StyledText : Text.PlainText
                            wrapMode: Text.WrapAnywhere
                            color: Theme.color.foreground
                            font { family: Theme.font.family; pixelSize: Theme.font.body }
                        }
                        Flow {
                            visible: pair.tagged
                            width: parent.width
                            spacing: Theme.spacing.tagPairGap
                            Repeater {
                                model: pair.tagged ? root.facts.tags : []
                                Row {
                                    id: tagItem
                                    required property string modelData
                                    spacing: Theme.spacing.tagNameGap
                                    // A name Flea does not know is plain text with no swatch; a digit fallback keeps the 14 px square its digit needs.
                                    Flea.TagSwatch {
                                        size: Theme.tag.fallback ? Tags.SWATCH_SIZE : Tags.PAIR_SWATCH_SIZE
                                        visible: Tags.NAMES.indexOf(tagItem.modelData) >= 0
                                        anchors.verticalCenter: parent.verticalCenter
                                        swatch: tagItem.modelData
                                        fallback: Theme.tag.fallback
                                        digit: Tags.keyFor(tagItem.modelData)
                                        tagColors: Theme.tag.menuColors
                                    }
                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: tagItem.modelData
                                        textFormat: Text.PlainText
                                        color: Theme.color.foreground
                                        font { family: Theme.font.family; pixelSize: Theme.font.body }
                                    }
                                }
                            }
                        }
                    }
                }
                Text {
                    width: parent.width
                    visible: text.length > 0
                    text: root.errorText || (root.busy ? "Working…" : "")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: root.errorText ? Theme.color.errorOnSurface : Theme.color.muted
                    font { family: Theme.font.family; pixelSize: Theme.font.caption }
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
