import QtQuick
import "js/Picker.js" as Picker

// The footer: what is checked on the left, the keys that act on it on the right. A message from
// the window takes the left slot in the accent while it lives.
Item {
    id: root

    property var picker: null

    height: Theme.chromeHeight

    // The footer takes the chrome plane, the same strip the ask above it stands on.
    Rectangle {
        anchors.fill: parent
        color: Theme.color.surface
    }

    Rectangle {
        anchors.top: parent.top
        width: parent.width
        height: Theme.spacing.hairline
        color: root.picker.edge
    }

    Text {
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        text: root.picker.message.length > 0 ? root.picker.message
                                             : Picker.statusLine(root.picker.marks.length, Picker.totalBytes(root.picker.marks))
        color: root.picker.message.length > 0 ? Theme.color.accent : Theme.color.foreground
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
    }

    Text {
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        text: Picker.hints(root.picker.req)
        color: Theme.color.foreground
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
    }
}
