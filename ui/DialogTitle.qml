import QtQuick
import "." as Flea

// A dialog's header line: what the dialog is doing on the left, and the word that closes it on the
// right. Dialogs rule 7 puts that word in the same corner on every dialog, which is the corner
// ui/SettingsPanel.qml already keeps it in, so one surface cannot spell the way out differently.
Item {
    id: root

    property string text: ""
    property int elide: Text.ElideRight
    property bool showsEscape: true

    readonly property bool truncated: label.truncated
    implicitHeight: label.implicitHeight

    Text {
        id: label
        anchors.left: parent.left
        anchors.right: escHint.left
        anchors.rightMargin: Theme.spacing.gap
        text: root.text
        color: Theme.color.foreground
        font.family: Theme.font.family
        font.pixelSize: Theme.card.title
        font.bold: true
        textFormat: Text.PlainText
        elide: root.elide
    }

    Flea.EscapeHint {
        id: escHint
        anchors.right: parent.right
        anchors.baseline: label.baseline
        visible: root.showsEscape
    }
}
