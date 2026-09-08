import QtQuick
import "." as Flea

// The open modes' location strip, the chooser's counterpart of the path bar ui/ChromeBar.qml draws
// inline: a path or URL typed here is reported on Return, and what the text means is
// ui/js/PickerEntry.js's call, never this strip's. Save mode keeps its own Filename field in
// ui/PickerSave.qml, so this one stands down there.
Item {
    id: root

    property var picker: null

    // Return. The text stays in the field, because the window may refuse it and the user then edits
    // rather than retypes.
    signal entered(string text)
    // Escape. The keyboard goes back to the list and the text stays too; see the key handler.
    signal dismissed()

    property alias text: field.text
    readonly property bool focused: field.focused

    function takeFocus() {
        field.takeFocus()
    }

    visible: !root.picker.saving
    implicitHeight: root.visible ? field.implicitHeight + 2 * Theme.spacing.rowPaddingX : 0

    Rectangle {
        anchors.fill: parent
        color: Theme.color.surface
    }

    Flea.DialogField {
        id: field
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        label: "File or location"
        placeholder: "Path or URL"
        onAccepted: root.entered(field.text)
    }

    // Both keys are accepted here, as ui/ChromeBar.qml's path bar accepts its own. Escape has to
    // be: unaccepted it climbs to the window's handler, which cancels the dialog, and Escape in
    // this field is not a cancel. The user is editing a location, not leaving the chooser, so the
    // text stays and only the keyboard moves back to the list. Return is accepted so it ends here:
    // a TextInput ignores it after emitting accepted, and nothing above should read it twice. The
    // strip stays up on blur, unlike the path bar, because it is a field of the window and not an
    // editor raised over one; the text is kept for the same reason.
    Keys.onPressed: function (event) {
        if (event.key === Qt.Key_Escape) {
            root.dismissed()
            event.accepted = true
        } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
            event.accepted = true
        }
    }
}
