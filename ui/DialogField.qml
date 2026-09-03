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

    signal accepted()
    // The form owns the order; a field only reports that Tab happened inside it.
    signal tabbed(var from, bool back)

    readonly property alias input: field
    readonly property bool focused: field.activeFocus

    function takeFocus() {
        field.forceActiveFocus()
    }
    readonly property real labelSpacing: 0.1

    implicitHeight: caption.implicitHeight + Theme.spacing.hairline * 4 + box.height

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
        anchors.topMargin: Theme.spacing.hairline * 4
        anchors.left: parent.left
        anchors.right: parent.right
        height: Theme.rowHeight - Theme.spacing.rowPaddingY
        color: Theme.color.background
        border.width: Theme.spacing.hairline
        border.color: field.activeFocus ? Theme.color.accent : Theme.color.muted

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
            font.pixelSize: Theme.font.bodySmall
            clip: true
            // Revealed only while the eye is held, so a shoulder never reads it from a stuck toggle.
            echoMode: root.secret && !reveal.pressed ? TextInput.Password : TextInput.Normal
            onAccepted: root.accepted()
            Keys.onTabPressed: root.tabbed(root, false)
            Keys.onBacktabPressed: root.tabbed(root, true)
        }

        Text {
            anchors.left: field.left
            anchors.verticalCenter: parent.verticalCenter
            visible: field.text.length === 0 && !field.activeFocus
            text: root.placeholder
            color: Theme.color.muted
            font.family: Theme.font.family
            font.pixelSize: Theme.font.bodySmall
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
                color: reveal.pressed ? Theme.color.accent : Theme.color.muted
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
