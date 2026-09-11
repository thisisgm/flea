import QtQuick
import qs.Commons
import "." as Flea
import "js/Format.js" as Format
import "js/Keymap.js" as Keymap

// The destructive choice must be reached deliberately; a reflexive Enter activates Cancel.
FocusScope {
    id: root
    anchors.fill: parent
    visible: opened
    property bool opened: false
    property var snapshot: ({})
    property bool destructiveFocus: false
    property string scopeName: "Trash items"
    readonly property real referenceScale: Theme.font.bodySmall / 13
    readonly property real cardPadding: Math.round(16 * referenceScale)
    readonly property real cardBottomPadding: Theme.spacing.rowPaddingX
    signal confirmed(int token)
    signal cancelled()
    readonly property var cardItem: card
    readonly property var cancelItem: cancelButton
    readonly property var dangerItem: dangerButton
    readonly property string titleText: title.text
    function open(value) { snapshot = value; destructiveFocus = false; body.contentY = 0; opened = true; forceActiveFocus() }
    function close() { opened = false }
    function cancel() { close(); cancelled() }
    function activate() {
        if (!destructiveFocus) { cancel(); return }
        var token = snapshot.token
        close()
        confirmed(token)
    }
    Keys.onPressed: function(event) {
        if (event.key === Qt.Key_Escape) root.cancel()
        else if (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)) { event.accepted = true; return }
        else if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) root.destructiveFocus = !root.destructiveFocus
        else if (event.key === Qt.Key_L || event.key === Qt.Key_Right) root.destructiveFocus = true
        else if (event.key === Qt.Key_H || event.key === Qt.Key_Left) root.destructiveFocus = false
        else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) root.activate()
        if (root.opened) body.reveal(root.destructiveFocus ? dangerButton : cancelButton)
        event.accepted = true
    }
    Rectangle {
        anchors.fill: parent
        color: Theme.color.background
        opacity: 0.5
        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onClicked: root.cancel()
            onWheel: function(wheel) { wheel.accepted = true }
        }
    }
    Rectangle {
        id: card
        anchors.centerIn: parent
        // TrashSidebar's 340px content width excludes its two 16px paddings and hairlines.
        width: Math.max(0, Math.min(Math.round(340 * root.referenceScale * Theme.dialogWidthRatio) + 2 * root.cardPadding + 2 * Theme.spacing.hairline, root.width - 2 * Theme.spacing.gap))
        height: Math.max(0, Math.min(body.wanted + root.cardPadding + root.cardBottomPadding + 2 * Theme.spacing.hairline, root.height - 2 * Theme.spacing.gap))
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
            anchors.leftMargin: root.cardPadding + Theme.spacing.hairline
            anchors.rightMargin: root.cardPadding + Theme.spacing.hairline
            anchors.topMargin: root.cardPadding + Theme.spacing.hairline
            anchors.bottomMargin: root.cardBottomPadding + Theme.spacing.hairline
            Column {
                width: body.width
                spacing: Theme.spacing.gap
                Row {
                    width: parent.width
                    spacing: Theme.spacing.gap
                    Flea.Glyph { id: alertMark; width: Theme.font.bodySmall * 1.3; height: title.height; name: "alert"; color: Theme.color.error }
                    Text {
                        id: title
                        width: parent.width - alertMark.width - parent.spacing
                        text: root.snapshot.all ? "Empty Trash?" : "Delete " + root.snapshot.count + (root.snapshot.count === 1 ? " item" : " items") + " permanently?"
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        color: Theme.color.foreground
                        font { family: Theme.font.family; pixelSize: Theme.font.body; bold: true }
                    }
                }
                Text {
                    width: parent.width
                    text: root.snapshot.all
                        ? root.snapshot.count + (root.snapshot.count === 1 ? " item, " : " items, ") + Format.size(root.snapshot.bytes || 0) + ". This deletes them from disk. " + Keymap.hintFor("undo") + " cannot undo it and the undo journal does not cover it."
                        : "These " + root.scopeName + " are deleted from disk. This cannot be undone."
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Theme.color.foreground
                    font { family: Theme.font.family; pixelSize: Theme.font.body }
                }
                Flow {
                    width: Math.min(parent.width, cancelButton.implicitWidth + dangerButton.implicitWidth + spacing)
                    anchors.right: parent.right
                    spacing: Theme.spacing.gap
                    Rectangle { width: cancelButton.width; height: cancelButton.height; color: root.destructiveFocus ? "transparent" : Qt.alpha(Theme.color.accent, 0.14); Flea.DialogButton { id: cancelButton; label: "Cancel"; primary: !root.destructiveFocus; onActivated: root.cancel() } }
                    Item {
                        id: dangerButton
                        implicitWidth: dangerText.implicitWidth + 2 * Theme.spacing.gap
                        height: Math.max(Theme.hitMin, dangerText.implicitHeight + Theme.spacing.gap)
                        Rectangle { anchors.fill: parent; color: "transparent"; border.width: Theme.spacing.hairline; border.color: root.destructiveFocus ? Theme.color.error : Theme.color.muted }
                        Text {
                            id: dangerText
                            anchors.centerIn: parent
                            text: root.snapshot.all ? "Empty Trash" : "Delete"
                            textFormat: Text.PlainText
                            color: Theme.color.error
                            font { family: Theme.font.family; pixelSize: Theme.font.body }
                        }
                        Accessible.role: Accessible.Button
                        Accessible.name: dangerText.text
                        Accessible.onPressAction: { root.destructiveFocus = true; root.activate() }
                        TapHandler { onTapped: { root.destructiveFocus = true; root.activate() } }
                    }
                }
            }
        }
    }
}
