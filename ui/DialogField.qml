import QtQuick
import qs.Commons
import "." as Flea

// One labelled field, as the canvas's Network board draws it: a small uppercase label above a
// bordered box that takes the accent while it has the caret.
Item {
    id: root

    property string label: ""
    // An alias, not a plain property assigned from onTextChanged: writing to a property that carries
    // a binding destroys that binding, so a later programmatic set (switching protocol resets the
    // port) would silently never reach the field.
    property alias text: field.text
    property string placeholder: ""
    // A password box hides what it holds, and the eye beside it reveals while held.
    property bool secret: false
    // The field that holds a refused answer frames itself in the error role; off everywhere else.
    property bool error: false
    // Bullets that stand for a refused answer the field no longer holds; the first key replaces them.
    property int heldMask: 0

    signal accepted()
    // Keys changed the text, so a card can drop the sentence the error frame stands for.
    signal edited()
    // The form owns the order; a field only reports that Tab happened inside it.
    signal tabbed(var from, bool back)

    readonly property alias input: field
    readonly property bool focused: field.activeFocus
    // Read by the capture harness: whether the eye is held, and its centre in window coordinates.
    readonly property bool revealed: reveal.pressed
    function eyeCentre() {
        var at = eye.mapToItem(null, eye.width / 2, eye.height / 2)
        return Math.round(at.x) + " " + Math.round(at.y)
    }

    function takeFocus() {
        field.forceActiveFocus()
    }
    readonly property real labelSpacing: 0.1

    implicitHeight: caption.implicitHeight + Theme.card.labelGap + box.height

    Text {
        id: caption
        anchors.top: parent.top
        anchors.left: parent.left
        text: root.label.toUpperCase()
        color: Theme.color.muted
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        font.letterSpacing: Theme.font.caption * root.labelSpacing
        textFormat: Text.PlainText
    }

    Rectangle {
        id: box
        anchors.top: caption.bottom
        anchors.topMargin: Theme.card.labelGap
        anchors.left: parent.left
        anchors.right: parent.right
        height: Theme.rowHeight - Theme.spacing.rowPaddingY
        color: Theme.color.background
        border.width: Theme.spacing.hairline
        border.color: root.error ? Theme.color.error : (field.activeFocus ? Theme.color.accent : Theme.color.mutedFrame)

        TextInput {
            id: field
            anchors.left: parent.left
            anchors.right: eye.left
            anchors.verticalCenter: parent.verticalCenter
            anchors.leftMargin: Theme.spacing.gap
            anchors.rightMargin: Theme.spacing.gap
            color: Theme.color.foreground
            selectionColor: Theme.color.accent
            selectedTextColor: Theme.color.background
            font.family: Theme.font.family
            font.pixelSize: Theme.font.body
            clip: true
            leftPadding: heldMarks.visible ? heldMarks.width : 0
            // Revealed only while the eye is held, so a shoulder never reads it from a stuck toggle.
            echoMode: root.secret && !reveal.pressed ? TextInput.Password : TextInput.Normal
            // The boards draw a masked value as U+2022 bullets, never Qt's larger default circle.
            passwordCharacter: "\u2022"
            onAccepted: root.accepted()
            onTextEdited: { root.heldMask = 0; root.edited() }
            Keys.onPressed: function(event) {
                if ((event.key === Qt.Key_Backspace || event.key === Qt.Key_Delete) && field.text.length === 0 && root.heldMask > 0) { root.heldMask = 0; root.edited() }
            }
            Keys.onTabPressed: function(event) { root.tabbed(root, (event.modifiers & Qt.ShiftModifier) !== 0) }
            Keys.onBacktabPressed: root.tabbed(root, true)
        }

        Text {
            id: heldMarks
            anchors.left: field.left
            anchors.verticalCenter: field.verticalCenter
            visible: root.heldMask > 0 && field.text.length === 0 && !reveal.pressed
            text: field.passwordCharacter.repeat(Math.max(0, root.heldMask))
            color: Theme.color.foreground
            font: field.font
            textFormat: Text.PlainText
        }

        Text {
            anchors.left: field.left
            anchors.verticalCenter: parent.verticalCenter
            visible: field.text.length === 0 && !field.activeFocus && !heldMarks.visible
            text: root.placeholder
            color: Theme.color.muted
            font.family: Theme.font.family
            font.pixelSize: Theme.font.body
            textFormat: Text.PlainText
            elide: Text.ElideRight
        }

        Item {
            id: eye
            anchors.right: parent.right
            anchors.rightMargin: Theme.spacing.gap
            anchors.verticalCenter: parent.verticalCenter
            width: root.secret ? Math.max(Theme.hitMin, Theme.font.bodySmall) : 0
            height: root.secret ? Math.max(Theme.hitMin, Theme.font.bodySmall) : 0

            Accessible.role: Accessible.Button
            Accessible.name: "Show password"
            Accessible.ignored: !root.secret

            Flea.Glyph {
                anchors.centerIn: parent
                width: Theme.font.bodySmall
                height: Theme.font.bodySmall
                visible: root.secret
                name: "eye"
                color: reveal.pressed ? Theme.color.accent : Theme.color.mutedFrame
            }

            MouseArea {
                id: reveal
                anchors.fill: parent
                enabled: root.secret
                cursorShape: Qt.PointingHandCursor
            }
        }
    }
}
