import QtQuick
import "." as Flea
import "js/Format.js" as Format
import "js/Picker.js" as Picker

// SaveFile's own strip: the name, where that name lands, and a warning when this directory already
// holds it. The board's rule is that the caller owns the WRITE, which is why a collision is said
// rather than refused; it says nothing about where this window may point, and answering with a path
// outside the folder the user was shown is not the caller's to license, so such a name is refused.
Item {
    id: root

    property var picker: null

    signal nameEdited(string text)
    signal accepted()

    readonly property string outPath: Picker.join(root.picker.path, root.picker.saveName)
    // Which name this strip is talking about: what is in the field, or the caller's own suggestion
    // when the field is empty because ui/picker.qml refused that suggestion instead of adopting it.
    readonly property string askedName: root.picker.saveName.length > 0 ? root.picker.saveName
                                                                        : root.picker.req.name
    // A name nobody has supplied yet is not a refusal, so the accept path is what asks for one.
    readonly property bool refused: root.askedName.length > 0 && !Picker.validName(root.askedName)
    // Only the rows the listing has actually sent can be compared, so a name past the held window
    // goes unwarned. The alternative is a stat this window has no request for, and the callback
    // hands the caller a URI either way.
    readonly property bool collides: {
        if (root.picker.saveName.length === 0)
            return false
        for (var i = 0; i < root.picker.rows.length; i++) {
            if (root.picker.rows[i].n === root.picker.saveName)
                return true
        }
        return false
    }

    visible: root.picker.saving
    implicitHeight: root.visible ? column.implicitHeight + 2 * Theme.spacing.rowPaddingX : 0

    Rectangle {
        anchors.fill: parent
        color: Theme.color.surface
    }

    Column {
        id: column
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        spacing: Theme.spacing.hairline * 4

        Flea.DialogField {
            id: field
            width: parent.width
            label: "Filename"
            text: root.picker.saveName
            onTextChanged: root.nameEdited(field.text)
            onAccepted: root.accepted()
        }

        // Where a name that is accepted lands. The URI elides its HEAD, because the tail is the
        // part that says where the file goes; eliding the middle cut that out of a long name and
        // hid a traversal from the person approving the answer. The label is its own item so that
        // elision eats the path and never the word saying what the path is.
        Row {
            width: parent.width
            visible: !root.refused

            Text {
                id: uriLabel
                text: "Output URI · "
                color: Theme.color.muted
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                textFormat: Text.PlainText
            }

            Text {
                width: parent.width - uriLabel.width
                text: Format.fileUri(root.outPath)
                color: Theme.color.muted
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                elide: Text.ElideLeft
                textFormat: Text.PlainText
            }
        }

        // Why there is no URI to show. This one elides its TAIL: a name that leaves the folder says
        // so at its front, so the front is the part that has to survive a long name.
        Text {
            width: parent.width
            visible: root.refused
            text: "Refused · " + root.askedName + " · " + Picker.NAME_REFUSED
            color: Theme.color.error
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            elide: Text.ElideRight
            textFormat: Text.PlainText
        }

        Text {
            width: parent.width
            visible: root.collides
            text: root.picker.saveName + " already exists here · review before continuing"
            color: Theme.color.error
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            elide: Text.ElideRight
            textFormat: Text.PlainText
        }
    }
}
