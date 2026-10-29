import QtQuick
import "." as Flea

// Built on request: one bounded, read-only document for selection across lines.
Rectangle {
    id: root

    property string text: ""
    property string notice: ""
    readonly property alias editor: editor
    readonly property alias viewport: scroll
    color: Theme.color.surface

    Flickable {
        id: scroll
        anchors.fill: parent
        anchors.margins: Theme.spacing.rowPaddingX
        clip: true
        acceptedButtons: Qt.NoButton
        contentWidth: width
        contentHeight: editor.height
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.VerticalFlick

        TextEdit {
            id: editor
            width: scroll.width
            height: Math.max(scroll.height, contentHeight)
            text: root.notice !== "" ? root.notice : root.text
            textFormat: TextEdit.PlainText
            enabled: root.notice === ""
            readOnly: true
            selectByMouse: true
            selectByKeyboard: true
            persistentSelection: true
            wrapMode: TextEdit.Wrap
            color: Theme.color.foreground
            selectionColor: Theme.color.accent
            selectedTextColor: Theme.color.background
            font.family: Theme.font.family
            font.pixelSize: Theme.font.body
            // Read-only keys must not turn a Space or a letter into file navigation.
            Keys.priority: Keys.AfterItem
            Keys.onPressed: function(event) {
                if (event.key === Qt.Key_Escape || event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab)
                    event.accepted = false
                else
                    event.accepted = true
            }
            Component.onCompleted: if (enabled) forceActiveFocus()
            onEnabledChanged: if (enabled) forceActiveFocus()
            onCursorRectangleChanged: {
                var top = cursorRectangle.y
                if (top < scroll.contentY) scroll.contentY = top
                else if (top + cursorRectangle.height > scroll.contentY + scroll.height)
                    scroll.contentY = top + cursorRectangle.height - scroll.height
            }
        }
    }

    Flea.FastScrollHandler { flickable: scroll }
    Flea.ViewportScrollBar {
        anchors { top: parent.top; right: parent.right }
        flickable: scroll
    }
}
