import QtQuick
import qs.Commons

// One protocol chip. The picked one takes the accent for its border, its text and a tint behind it,
// which is the one place in this dialog a fill appears.
Item {
    id: root

    property string label: ""
    property bool picked: false

    signal activated()
    signal tabbed(var from, bool back)

    // The canvas's own tint strength for a picked chip.
    readonly property real tint: 0.14

    readonly property bool focused: root.activeFocus

    function takeFocus() {
        root.forceActiveFocus()
    }

    Keys.onTabPressed: root.tabbed(root, false)
    Keys.onBacktabPressed: root.tabbed(root, true)
    Keys.onReturnPressed: root.activated()
    Keys.onEnterPressed: root.activated()
    Keys.onSpacePressed: root.activated()

    implicitWidth: Math.max(Theme.hitMin, text.implicitWidth + 2 * Theme.spacing.gap + 2 * Theme.spacing.hairline)
    implicitHeight: Math.max(Theme.hitMin, text.implicitHeight + Theme.spacing.gap + 2 * Theme.spacing.hairline)

    Accessible.role: Accessible.Button
    Accessible.name: root.label
    Accessible.onPressAction: root.activated()

    Rectangle {
        anchors.fill: parent
        color: root.picked ? Theme.color.accent : "transparent"
        opacity: root.picked ? root.tint : 0
    }

    Rectangle {
        anchors.fill: parent
        color: "transparent"
        border.width: Theme.spacing.hairline
        border.color: root.picked ? Theme.color.accent
                                  : (root.focused ? Theme.color.foreground : Theme.color.muted)
    }

    Text {
        id: text
        anchors.centerIn: parent
        text: root.label
        color: root.picked ? Theme.color.accent : Theme.color.muted
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
    }

    HoverHandler { cursorShape: Qt.PointingHandCursor }

    TapHandler {
        acceptedButtons: Qt.LeftButton
        onTapped: root.activated()
    }
}
