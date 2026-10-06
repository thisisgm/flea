import QtQuick
import qs.Commons
import "js/Format.js" as Format
import "js/Picker.js" as Picker
import "js/PathBar.js" as PathBar

// The path slot of the picker's nav strip: the location the strip stands in, and, on Ctrl+L or ":",
// the field that types a new one. Both fill the same room between the nav marks and the filter
// chips, so the owner sets its anchors and this owns only what the slot draws. Lifted out of
// ui/PickerChrome.qml at its 400-line hard cap, the same way ui/ResizeHandle.qml came out of Header.
Item {
    id: root

    property var picker: null

    Text {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        visible: !root.picker.editingPath
        // Recent is a location and not a path, so the strip says the location's own name; a
        // tilde form of the token would be a path the window is not standing in.
        text: root.picker.recent ? Picker.RECENT_LABEL : Format.tilde(root.picker.path, root.picker.home)
        color: Theme.color.foreground
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        elide: Text.ElideLeft
        textFormat: Text.PlainText
    }

    // ui/ChromeBar.qml's field in miniature: every key it needs is accepted here, so Return
    // and Tab never fall through to the list under it. A file path lists nothing, the same
    // as the browser's own path bar, so this navigates and never selects.
    TextInput {
        id: pathField
        anchors.fill: parent
        visible: root.picker.editingPath
        enabled: root.picker.editingPath
        verticalAlignment: TextInput.AlignVCenter
        color: Theme.color.foreground
        selectionColor: Theme.color.accent
        selectedTextColor: Theme.color.background
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        clip: true

        onVisibleChanged: {
            if (!visible) return
            var path = root.picker.path
            pathField.text = root.picker.recent ? "" : (path === "/" ? "/" : path + "/")
            pathField.forceActiveFocus()
            pathField.selectAll()
        }
        // A click on the rail, a chip or the list closes the field rather than stranding it
        // open with no way back to the keyboard, the same close ui/ChromeBar.qml makes.
        onActiveFocusChanged: if (!activeFocus && root.picker.editingPath) root.picker.cancelPathEdit()

        Keys.onPressed: function (event) {
            if (event.key === Qt.Key_Escape) {
                root.picker.cancelPathEdit()
                event.accepted = true
            } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                root.picker.commitPath(pathField.text, PathBar.resolve(pathField.text, root.picker.path, root.picker.home))
                event.accepted = true
            }
        }
    }

    Rectangle {
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        visible: root.picker.editingPath
        height: Theme.spacing.hairline
        color: Theme.color.accent
    }
}
