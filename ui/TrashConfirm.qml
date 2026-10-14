import QtQuick
import qs.Commons
import "." as Flea
import "js/Trash.js" as Trash

// The destructive choice must be reached deliberately; a reflexive Enter activates Cancel.
FocusScope {
    id: root
    anchors.fill: parent
    visible: opened || opacity > 0
    // Closed means no input, even while the fade-out is still drawn.
    enabled: opened
    opacity: 0
    property bool opened: false
    property var snapshot: ({})
    property bool destructiveFocus: false
    property string scopeName: "Trash items"
    readonly property real cardPadding: Theme.card.padding
    readonly property real cardBottomPadding: Theme.card.padding
    signal confirmed(int token)
    signal cancelled()
    readonly property var cardItem: card
    readonly property var cancelItem: cancelButton
    readonly property var dangerItem: dangerButton
    readonly property string titleText: title.text
    readonly property string bodyText: bodyLabel.text
    function open(value) { snapshot = value; destructiveFocus = false; body.contentY = 0; opened = true; forceActiveFocus() }
    function close() { opened = false }
    function cancel() { close(); cancelled() }
    function activate() {
        if (!destructiveFocus) { cancel(); return }
        var token = snapshot.token
        close()
        confirmed(token)
    }
    Flea.Reveal { target: root; shown: root.opened; lift: card }
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
        x: Theme.cardOrigin(root.width, width)
        y: Theme.cardOrigin(root.height, height)
        width: Theme.cardSpan(Theme.card.narrow, root.width - 2 * Theme.card.inset)
        height: Theme.cardSpan(body.wanted + root.cardPadding + root.cardBottomPadding + 2 * Theme.spacing.hairline, root.height - 2 * Theme.card.inset)
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
            anchors.leftMargin: root.cardPadding + Theme.spacing.hairline - Theme.focusClearance
            anchors.rightMargin: root.cardPadding + Theme.spacing.hairline - Theme.focusClearance
            anchors.topMargin: root.cardPadding + Theme.spacing.hairline - Theme.focusClearance
            anchors.bottomMargin: root.cardBottomPadding + Theme.spacing.hairline - Theme.focusClearance
            title: Row {
                width: parent.width
                spacing: Theme.spacing.gap
                Flea.Glyph { id: alertMark; width: Theme.font.bodySmall * 1.3; height: title.height; name: "alert"; color: Theme.color.error }
                Text {
                    id: title
                    width: parent.width - alertMark.width - parent.spacing
                    text: Trash.confirmTitle(root.snapshot)
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Theme.color.foreground
                    font { family: Theme.font.family; pixelSize: Theme.font.body; bold: true }
                }
            }
            buttons: Flow {
                width: Math.min(parent.width, cancelButton.implicitWidth + dangerButton.implicitWidth + spacing)
                anchors.right: parent.right
                spacing: Theme.spacing.gap
                Flea.DialogButton {
                    id: cancelButton
                    label: "Cancel"
                    primary: true
                    focused: !root.destructiveFocus
                    onActivated: root.cancel()
                }
                Flea.DialogButton {
                    id: dangerButton
                    label: root.snapshot.all ? "Empty Trash" : "Delete"
                    destructive: true
                    focused: root.destructiveFocus
                    onActivated: { root.destructiveFocus = true; root.activate() }
                }
            }
            Text {
                id: bodyLabel
                width: parent.width
                text: Trash.confirmBody(root.snapshot, root.scopeName)
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                color: Theme.color.foreground
                font { family: Theme.font.family; pixelSize: Theme.font.body }
            }
        }
    }
}
