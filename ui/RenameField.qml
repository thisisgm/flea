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
    property var pane: null
    readonly property string errorText: pane ? pane.renameError : ""
    readonly property bool pending: pane ? pane.renamePending : false
    readonly property real errorHeight: errorText.length > 0 ? errorLabel.implicitHeight + Theme.spacing.gap : 0
    readonly property real fieldHeight: height - errorHeight
    readonly property alias inputItem: field
    implicitHeight: Theme.rowHeight - 2 * Theme.spacing.rowPaddingY + errorHeight

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

    // File validation belongs to the pane; rail labels retain their existing empty-submit behavior.
    function commit() {
        if (root.pending) return false
        var next = field.text.trim()
        if ((!root.pane && next.length === 0) || field.text === root.name || next === root.name) {
            root.abandoned()
            return false
        }
        root.committed(next)
        return true
    }

    function revealError() {
        if (!root.visible || !root.pane || root.errorText.length === 0) return
        root.pane.setCursor(root.pane.renamingIndex)
        field.forceActiveFocus()
    }
    // Let the expanded row and Grid cell height settle before containing the complete error editor.
    onErrorTextChanged: if (root.visible && root.errorText.length > 0) Qt.callLater(root.revealError)

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
        // Escape and focus loss both refuse to abandon an in-flight rename; a row scrolled out of the
        // cache buffer or a view switch is the same case, and it was throwing the draft away.
        if (root.pending) return
        root.abandoned()
    }

    Rectangle {
        width: parent.width
        height: root.fieldHeight
        color: Theme.color.background
        border.width: Theme.spacing.hairline
        border.color: root.errorText.length > 0 ? Theme.color.error : Theme.color.accent
    }

    TextInput {
        id: field
        anchors { left: parent.left; right: parent.right; top: parent.top }
        height: root.fieldHeight
        anchors.leftMargin: Theme.spacing.gap
        anchors.rightMargin: Theme.spacing.gap
        verticalAlignment: TextInput.AlignVCenter
        color: Theme.color.foreground
        selectionColor: Theme.color.accent
        selectedTextColor: Theme.color.background
        font.family: Theme.font.family
        font.pixelSize: Theme.font.body
        clip: true
        readOnly: root.pending

        // Both keys are handled and accepted here rather than through onAccepted, because an
        // unaccepted Return goes on to the list's own Keys handler, which reads it as "open" and
        // tries to open the row under a name the rename has just taken away.
        Keys.onPressed: function (event) {
            if (event.key === Qt.Key_Escape) {
                if (!root.pending) root.abandoned()
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
        onActiveFocusChanged: if (!activeFocus && root.visible && !root.pending) root.abandoned()
    }

    Text {
        id: errorLabel
        anchors { top: field.bottom; left: parent.left; right: parent.right }
        anchors.topMargin: Theme.spacing.gap
        visible: root.errorText.length > 0
        text: root.errorText
        color: Theme.color.error
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
    }

    // The extension, painted over the field's own copy of it. It stands down only when a selection
    // actually reaches into the extension, because a patch over selected text would hide the
    // selection; the usual case, the stem selected and the extension not, keeps the muted run.
    Item {
        id: mutedExtension
        visible: root.extension.length > 0 && field.selectionEnd <= root.stemEnd
        // contentWidth is read so this re-evaluates once the field has laid the new text out.
        // positionToRectangle is a method, so nothing re-runs it on its own, and begin() assigns the
        // text and selects the stem in one go: the boundary was measured against the layout before
        // that text existed, came back 0, and the patch covered the stem instead of the extension.
        // Every view drew a rename as a bare ".txt" until the first keystroke moved the selection.
        x: field.contentWidth >= 0 ? field.x + field.positionToRectangle(root.stemEnd).x : field.x
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
