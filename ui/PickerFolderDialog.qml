import QtQuick
import "." as Flea
import "js/Format.js" as Format
import "js/Picker.js" as Picker

// A lazy modal card; its Tab/Escape/Enter never act on the save dialog underneath.
FocusScope {
    id: root
    required property var controller
    readonly property var fieldItem: field.input
    anchors.fill: parent
    focus: true
    Keys.onEscapePressed: function(event) { root.controller.close(); event.accepted = true }
    function focusName() { field.takeFocus(); body.reveal(field) }
    function step(from, back) {
        var items = [field.input, cancelButton, createButton].filter(function(item) { return item.enabled })
        if (!items.length) return
        var at = items.indexOf(from)
        var next = items[(at + (back ? -1 : 1) + items.length) % items.length]
        next.forceActiveFocus(Qt.TabFocusReason)
        body.reveal(next)
    }
    // Destruction cancels the owned timer, so closing a lazy card cannot run a stale JS closure.
    Timer { interval: 0; running: true; onTriggered: root.focusName() }

    Flea.DialogScrim { onDismissed: root.controller.close() }
    Rectangle {
        id: card
        objectName: "pickerFolderCard"
        anchors.centerIn: parent
        width: Math.max(0, Math.min(parent.width - 2 * Theme.spacing.rowPaddingX, Theme.space(420)))
        height: Math.max(0, Math.min(parent.height - 2 * Theme.spacing.rowPaddingX, Math.ceil(body.contentHeight + 2 * Theme.spacing.rowPaddingX)))
        color: Theme.color.surface
        border.width: Theme.spacing.hairline
        border.color: Theme.color.mutedFrame
        // Own all pointer input in the card, including the spaces between its controls.
        MouseArea { anchors.fill: parent; acceptedButtons: Qt.LeftButton | Qt.RightButton; onWheel: function(event) { event.accepted = true } }
        Flea.CardScroll {
            id: body
            objectName: "pickerFolderViewport"
            anchors.fill: parent
            anchors.margins: Theme.spacing.rowPaddingX
            bleed: Theme.focusClearance
            Column {
                width: parent.width
                spacing: Theme.spacing.gap
                Flea.DialogHeader { width: parent.width; text: "New folder" }
                Text {
                    width: parent.width
                    text: "Create in " + Format.tilde(root.controller.folder, root.controller.picker.home)
                    color: Theme.color.foreground
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    textFormat: Text.PlainText
                    wrapMode: Text.WrapAnywhere
                }
                Flea.DialogField {
                    id: field
                    width: parent.width
                    label: "Name"
                    text: root.controller.name
                    enabled: !root.controller.pending && !root.controller.committed
                    error: root.controller.error.length > 0
                    input.Accessible.name: "Folder name"
                    onEdited: { root.controller.name = text; if (root.controller.ready) root.controller.error = "" }
                    onAccepted: root.controller.submit()
                    onTabbed: function(from, back) { root.step(field.input, back) }
                }
                Text {
                    width: parent.width
                    visible: root.controller.error.length > 0
                    text: root.controller.error
                    color: Theme.color.errorOnSurface
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }
                Text {
                    width: parent.width
                    text: "Created folders remain if you cancel the save dialog."
                    color: Theme.color.foreground
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }
                Flow {
                    width: parent.width
                    spacing: Theme.spacing.gap
                    layoutDirection: Qt.RightToLeft
                    Flea.DialogButton {
                        id: createButton
                        objectName: "pickerFolderCreate"
                        label: root.controller.pending ? "Creating…" : root.controller.preparing ? "Checking…" : root.controller.ready ? "Create folder" : "Try again"
                        primary: true
                        tabStop: true
                        tabHandle: true
                        available: !root.controller.pending && !root.controller.preparing && !root.controller.committed
                            && !root.controller.picker.backendUnavailable && (root.controller.ready ? Picker.validName(field.text) : root.controller.error.length > 0)
                        onTabbed: function(from, back) { root.step(from, back) }
                        onActivated: root.controller.submit()
                    }
                    Flea.DialogButton {
                        id: cancelButton
                        objectName: "pickerFolderCancel"
                        label: "Cancel"
                        tabStop: true
                        tabHandle: true
                        available: !root.controller.pending
                        onTabbed: function(from, back) { root.step(from, back) }
                        onActivated: root.controller.close()
                    }
                }
            }
        }
    }
}
