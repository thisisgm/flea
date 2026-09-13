import QtQuick
import qs.Commons

// A control in a chrome strip, drawn under GM's 2026-09-11 ruling. Two inks: a STRUCTURAL RULE
// recedes at the hairline, and a CONTROL FRAME advances in the control's own role. Drawing both in
// one ink, which is what ui/picker.qml's comment admits the boards did, makes a control's edge and a
// strip's edge the same line, and the control dissolves into the chrome as the scale drops. The
// frame carries the role, the wash inside it carries the state. ui/DialogButton.qml always did this.
Item {
    id: root

    // A chrome strip's own controls are marks alone; a control with a LABEL is a word in a text
    // region and stays one. There is deliberately no mark here: one beside the word was a third
    // species, and at the chrome mark size it outweighed the caption it stood next to.
    property string label: ""
    // plain is the neutral control, accent the one action a surface is asking for, error a permanent
    // deletion. The role is the ink, at every state; only the wash under it moves.
    property string role: "plain"
    property bool available: true
    // A primary control carries its wash at rest, because it is the thing the surface wants read
    // first. Every other control earns one only under the pointer or the keyboard.
    property bool primary: false

    signal activated()

    readonly property color ink: !root.available ? Theme.color.muted
        : root.role === "accent" ? Theme.color.accent
        : root.role === "error" ? Theme.color.error
        : Theme.color.foreground
    // Every control rests in a muted frame, the role ThemeRoles.html gives an inactive control, and
    // the frame rises to the control's own ink under the pointer or the keyboard. The exception is
    // the primary control, which frames in accent at rest because it is the one action a surface is
    // asking for. A destructive control therefore rests exactly as the confirm dialog draws it, a
    // plain frame carrying error text, and earns the error frame only when it is reached for: an
    // error frame at rest is louder than anything else Flea draws, and it was.
    readonly property color frame: !root.available ? Theme.color.muted
        : root.role === "accent" ? Theme.color.accent
        : (root.activeFocus || hover.hovered || tap.pressed) ? root.ink
        : Theme.color.muted
    readonly property real wash: !root.available ? 0
        : (activeFocus || tap.pressed) ? Theme.washActive
        : hover.hovered ? Theme.washHover
        : root.primary ? Theme.washActive : 0

    enabled: root.available
    activeFocusOnTab: root.available
    implicitWidth: Math.max(Theme.hitMin, content.implicitWidth + 2 * Theme.spacing.gap)
    implicitHeight: Theme.chromeHeight

    Accessible.role: Accessible.Button
    Accessible.name: root.label
    Accessible.onPressAction: if (root.available) root.activated()
    Keys.onReturnPressed: if (root.available) root.activated()
    Keys.onEnterPressed: if (root.available) root.activated()
    Keys.onSpacePressed: if (root.available) root.activated()

    // The item fills the strip so the press clears hitMin, while what is DRAWN is the frame, centred
    // in the strip LESS its own rule: centring in the whole strip leaves the margin above the frame
    // and the margin below it unequal by exactly the rule's width, which is what it looked like.
    Rectangle {
        id: box
        anchors.horizontalCenter: parent.horizontalCenter
        y: Math.round((parent.height - Theme.spacing.hairline - height) / 2)
        width: parent.width
        height: Theme.chromeControlHeight
        color: Qt.alpha(root.ink, root.wash)
        border.width: Theme.spacing.hairline
        border.color: root.frame

        // Centred in the FRAME rather than in the strip, so the word sits where the box is and not
        // half a rule below it.
        Text {
            id: content
            anchors.centerIn: parent
            text: root.label
            color: root.ink
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            textFormat: Text.PlainText
        }
    }

    HoverHandler {
        id: hover
        cursorShape: root.available ? Qt.PointingHandCursor : Qt.ArrowCursor
    }

    TapHandler {
        id: tap
        acceptedButtons: Qt.LeftButton
        gesturePolicy: TapHandler.ReleaseWithinBounds
        onTapped: if (root.available) root.activated()
    }
}
