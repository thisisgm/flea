import QtQuick
import qs.Commons

// The row becoming its own editor, per the States artboard: an accent frame around the name, the
// extension muted inside that frame, enter commits and escape abandons.
//
// A TextInput cannot colour part of its own text, and splitting the extension into a second,
// non-editable Text would stop the operator renaming a.txt to a.md. So the whole name stays editable
// and the extension is painted over it: positionToRectangle gives the exact x of the boundary, an
// opaque patch covers what the field drew there, and the muted copy goes on top. The patch yields
// whenever there is a selection to render, so selection is never hidden by it.
Item {
    id: root

    property string name: ""

    signal committed(string newName)
    signal abandoned()

    // Read off what is in the field right now, not off the name it opened with, so the muted run
    // follows an edit that changes where the extension starts.
    readonly property string current: field.text
    readonly property int dot: root.current.lastIndexOf(".")
    readonly property int stemEnd: root.dot > 0 ? root.dot : root.current.length
    readonly property string extension: root.current.substring(root.stemEnd)

    function begin() {
        field.text = root.name
        field.forceActiveFocus()
        // The stem alone, which is the part a rename usually changes.
        var cut = root.name.lastIndexOf(".")
        field.select(0, cut > 0 ? cut : root.name.length)
    }

    // A commit that changes nothing, or empties the name, is an abandon: the backend would answer
    // "not work" for the first and refuse the second, and neither is worth a round trip. The name is
    // trimmed the way ui/Sidebar.qml already trims the rail's, because Enter on three spaces
    // otherwise creates a file named three spaces. Answers whether a commit actually happened.
    function commit() {
        var next = field.text.trim()
        // The untrimmed text is compared too, so Enter on an unmodified padded name is still the
        // abandon this says it is rather than a silent rename to the trimmed form.
        if (next.length === 0 || field.text === root.name || next === root.name) {
            root.abandoned()
            return false
        }
        root.committed(next)
        return true
    }

    // The editor arms itself rather than leaving it to each row that draws one: an Item built with
    // visible already true writes true over true and emits no visibleChanged, so a delegate
    // constructed mid-rename came up empty with nothing holding the caret.
    Component.onCompleted: if (root.visible) root.begin()

    // Hiding is abandoning. Qt drops effective visibility before it emits this, so the focus handler
    // below can never see the case, and a hidden editor left renamingIndex set with nothing alive to
    // clear it, which killed the whole window's keyboard, escape included.
    onVisibleChanged: {
        if (root.visible) {
            root.begin()
            return
        }
        // The enclosing ListView is a focus scope and remembers this field as its focused child, so
        // giving up what begin() took is what lets the scope itself take the keys again.
        field.focus = false
        root.abandoned()
    }

    Rectangle {
        anchors.fill: parent
        color: Theme.color.background
        border.width: Theme.spacing.hairline
        border.color: Theme.color.accent
    }

    TextInput {
        id: field
        anchors.fill: parent
        anchors.leftMargin: Theme.spacing.gap
        anchors.rightMargin: Theme.spacing.gap
        verticalAlignment: TextInput.AlignVCenter
        color: Theme.color.foreground
        selectionColor: Theme.color.accent
        selectedTextColor: Theme.color.background
        font.family: Theme.font.family
        font.pixelSize: Theme.font.body
        clip: true

        // Both keys are handled and accepted here rather than through onAccepted, because an
        // unaccepted Return goes on to the list's own Keys handler, which reads it as "open" and
        // tries to open the row under a name the rename has just taken away.
        Keys.onPressed: function (event) {
            if (event.key === Qt.Key_Escape) {
                root.abandoned()
                event.accepted = true
                return
            }
            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                root.commit()
                event.accepted = true
            }
        }

        // Losing Qt focus while the editor is up abandons, which is the rail's own field and the
        // context menu, which takes focus as it opens. A click on another row never lands here,
        // because a TapHandler moves no focus; ui/js/Tap.js commits that case explicitly.
        onActiveFocusChanged: if (!activeFocus && root.visible) root.abandoned()
    }

    // The extension, painted over the field's own copy of it. It stands down only when a selection
    // actually reaches into the extension, because a patch over selected text would hide the
    // selection; the usual case, the stem selected and the extension not, keeps the muted run.
    Item {
        id: mutedExtension
        visible: root.extension.length > 0 && field.selectionEnd <= root.stemEnd
        x: field.x + field.positionToRectangle(root.stemEnd).x
        y: field.y
        width: Math.max(0, field.width - (x - field.x))
        height: field.height
        clip: true

        Rectangle {
            anchors.fill: parent
            color: Theme.color.background
        }

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: root.extension
            color: Theme.color.muted
            font.family: field.font.family
            font.pixelSize: field.font.pixelSize
            textFormat: Text.PlainText
        }
    }
}
