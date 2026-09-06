import QtQuick
import qs.Commons

// A text row with a hairline frame, not filled button chrome: the canvas draws every dialog button
// this way, and the accent one is the action the dialog is for.
Item {
    id: root

    property string label: ""
    property bool primary: false
    // An action this dialog cannot take right now. ui/PickerChrome.qml's Framed is the in-tree model:
    // the ink says so and the press does nothing, rather than a live control that answers nothing.
    property bool available: true

    signal activated()

    // The canvas draws a secondary button as a hairline rule carrying live text, so only the frame
    // takes muted, the role ThemeRoles.html gives borders and inactive controls; the label is alive.
    readonly property color frame: root.primary ? Theme.color.accent : Theme.color.muted
    readonly property color ink: !root.available ? Theme.color.muted
                               : root.primary ? Theme.color.accent : Theme.color.foreground

    implicitWidth: Math.max(Theme.hitMin, text.implicitWidth + 2 * Theme.spacing.gap + 2 * Theme.spacing.hairline)
    implicitHeight: Math.max(Theme.hitMin, text.implicitHeight + Theme.spacing.gap + 2 * Theme.spacing.hairline)
    scale: tap.pressed && root.available && !Theme.reducedMotion ? 0.96 : 1

    Accessible.role: Accessible.Button
    Accessible.name: root.label
    Accessible.onPressAction: if (root.available) root.activated()

    Behavior on scale {
        enabled: !Theme.reducedMotion
        NumberAnimation { duration: 150; easing.type: Easing.OutQuad }
    }

    Rectangle {
        anchors.fill: parent
        color: "transparent"
        border.width: Theme.spacing.hairline
        border.color: root.frame
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

    HoverHandler { cursorShape: root.available ? Qt.PointingHandCursor : Qt.ArrowCursor }

    TapHandler {
        id: tap
        acceptedButtons: Qt.LeftButton
        onTapped: if (root.available) root.activated()
    }
}
