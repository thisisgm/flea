import QtQuick
import qs.Commons
import "js/Motion.js" as Motion

// A text row with a hairline frame, not filled button chrome: the canvas draws every dialog button
// this way, and the accent one is the action the dialog is for.
Item {
    id: root

    property string label: ""
    property bool primary: false

    signal activated()

    readonly property color ink: root.primary ? Theme.color.accent : Theme.color.muted

    implicitWidth: Math.max(Theme.hitMin, text.implicitWidth + 2 * Theme.spacing.gap + 2 * Theme.spacing.hairline)
    implicitHeight: Math.max(Theme.hitMin, text.implicitHeight + Theme.spacing.gap + 2 * Theme.spacing.hairline)
    scale: tap.pressed && !Motion.reduced ? 0.96 : 1

    Accessible.role: Accessible.Button
    Accessible.name: root.label
    Accessible.onPressAction: root.activated()

    Behavior on scale {
        enabled: !Motion.reduced
        NumberAnimation { duration: 150; easing.type: Easing.OutQuad }
    }

    Rectangle {
        anchors.fill: parent
        color: "transparent"
        border.width: Theme.spacing.hairline
        border.color: root.ink
    }

    Text {
        id: text
        anchors.centerIn: parent
        text: root.label
        color: root.ink
        font.family: Theme.font.family
        font.pixelSize: Theme.font.bodySmall
        textFormat: Text.PlainText
    }

    HoverHandler { cursorShape: Qt.PointingHandCursor }

    TapHandler {
        id: tap
        acceptedButtons: Qt.LeftButton
        onTapped: root.activated()
    }
}
