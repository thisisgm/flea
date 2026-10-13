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
    property var viewport: null
    property Item editorHost: null
    property bool containOnBegin: false
    // GridTile supplies the first measurement later; zero would collapse an expanded predecessor.
    property real extraHeight: -1
    readonly property string errorText: pane ? pane.renameError : ""
    readonly property bool pending: pane ? pane.renamePending : false
    readonly property real errorHeight: errorText.length > 0 ? errorLabel.implicitHeight + Theme.spacing.gap : 0
    readonly property real fieldHeight: height - errorHeight
    readonly property alias inputItem: field
    // The editor's frame and the extension's patch, so a test measures the patch against the frame's four sides.
    readonly property alias frame: editFrame
    readonly property alias extensionPatch: mutedExtension
    // The selection drawn as one box that fills the frame's interior, so a test measures it against the frame.
    readonly property alias selectionBox: selectionFill
    // The error line's width where the host gives it more than the field, or -1 for the field's own.
    property real errorSpan: -1
    // The one height every host draws: the row's line box, or the typed line and two hairlines where a small stop's is shorter.
    readonly property real lineBox: Math.max(Theme.rowHeight - 2 * Theme.spacing.rowPaddingY,
                                             Math.ceil(typedLine.height) + 2 * Theme.spacing.hairline)
    implicitHeight: root.lineBox + errorHeight

    signal committed(string newName)
    signal abandoned()

    // Read off what is in the field right now, not off the name it opened with, so the muted run
    // follows an edit that changes where the extension starts.
    readonly property string current: field.text
    readonly property int dot: root.current.lastIndexOf(".")
    readonly property int stemEnd: root.dot > 0 ? root.dot : root.current.length
    readonly property string extension: root.current.substring(root.stemEnd)

    // Set by begin(), so a hide can only abandon an edit that began; see onVisibleChanged below.
    property bool begun: false
    property bool beginning: false
    property int editIndex: -1
    property string editName: ""
    property var editPane: null
    property var editViewport: null
    property bool containmentQueued: false
    property bool containing: false
    onExtraHeightChanged: if (root.containOnBegin && root.viewport && root.viewport.queueRenameLayout)
        root.viewport.queueRenameLayout()

    function ownsEdit() {
        return root.begun && root.name === root.editName
            && root.pane === root.editPane && root.viewport === root.editViewport
            && (!root.pane || root.pane.renamingIndex === root.editIndex)
            && (!root.containOnBegin || !root.viewport || root.viewport.renameEditor === root)
    }

    function viewportOwnsFocus() {
        return root.containOnBegin && root.viewport && root.viewport.activeFocus
            && root.viewport.visible && !root.viewport.hiddenHeld
    }

    // A retained delegate can be visible yet culled from Grid's scene graph. Resolve its live slot.
    function layoutHost() {
        if (!root.containOnBegin || !root.viewport || !root.editorHost) return root.editorHost
        var current = root.viewport.currentItem
        var slot = current && current.renaming ? current : root.editorHost
        var host = root.viewport.itemAt(slot.x + slot.width / 2, slot.y + slot.height / 2)
        var editor = host ? host.editorField : null
        return editor && host.renaming && editor.pane === root.pane && editor.viewport === root.viewport
            && editor.name === root.name ? host : null
    }

    function begin(fromLayout) {
        if (root.beginning) return
        if (root.begun && root.containOnBegin && root.viewport && root.viewport.renameEditor
                && root.viewport.renameEditor !== root
                && !(fromLayout && root.layoutHost() === root.editorHost)) return
        var continuing = root.ownsEdit()
        var handoff = null
        var previous = root.containOnBegin && root.viewport ? root.viewport.renameEditor : null
        var retirement = root.containOnBegin && root.viewport ? root.viewport.renameRetirement : null
        if (!continuing && previous && previous !== root
                && root.pane && previous.editPane === root.pane && previous.editViewport === root.viewport
                && previous.editIndex === root.pane.renamingIndex && previous.editName === root.name
                && previous.ownsEdit()) {
            var input = previous.inputItem
            handoff = {text: input.text, cursor: input.cursorPosition,
                anchor: input.cursorPosition === input.selectionStart ? input.selectionEnd : input.selectionStart,
                focused: input.activeFocus || root.viewportOwnsFocus()}
        }
        if (!continuing && !previous && retirement && root.pane === retirement.pane
                && root.viewport === retirement.viewport && root.pane.renamingIndex === retirement.index
                && root.name === retirement.name && root.pane.renameRequest === retirement.request) handoff = retirement
        root.beginning = true
        lifecycle.stop()
        root.editIndex = root.pane ? root.pane.renamingIndex : -1
        root.editName = root.name
        root.editPane = root.pane
        root.editViewport = root.viewport
        root.containmentQueued = false
        root.begun = true
        // A recycled same-row delegate can begin before its predecessor has lost focus or died.
        if (root.containOnBegin && root.viewport) root.viewport.renameEditor = root
        if (!continuing) field.text = handoff ? handoff.text : root.name
        // A menu or rail that already took focus keeps it; the lifecycle timer judges that departure.
        if (!handoff || (handoff.focused && root.viewportOwnsFocus())) field.forceActiveFocus()
        // The stem alone, which is the part a rename usually changes.
        var cut = root.name.lastIndexOf(".")
        if (handoff) field.select(handoff.anchor, handoff.cursor)
        else if (!continuing) field.select(0, cut > 0 ? cut : root.name.length)
        if (root.containOnBegin || root.errorText.length > 0) root.queueContainment()
        root.beginning = false
    }

    // File validation belongs to the pane; rail labels retain their existing empty-submit behavior.
    function commit() {
        if (!root.ownsEdit() || !root.visible || root.pending) return false
        var next = field.text.trim()
        if ((!root.pane && next.length === 0) || field.text === root.name || next === root.name) {
            root.abandon()
            return false
        }
        root.committed(next)
        return true
    }

    function queueContainment() {
        if (!root.begun || root.containmentQueued) return
        root.containmentQueued = true
        lifecycle.restart()
    }
    function revealEditor() {
        if (!root.ownsEdit() || !root.visible || !root.pane || !field.activeFocus) return
        if (root.viewport && (!root.viewport.visible || root.viewport.hiddenHeld)) return
        root.containing = true
        // Grid height changes reposition every tile. Contain its final position, not the old layout.
        if (root.containOnBegin && root.viewport) root.viewport.forceLayout()
        if (root.ownsEdit() && root.visible && field.activeFocus) root.pane.setCursor(root.editIndex, 0)
        root.containing = false
    }
    // Let the expanded row and Grid cell height settle before containing the complete error editor.
    onErrorTextChanged: if (root.visible && root.errorText.length > 0) root.queueContainment()

    // The editor arms itself rather than leaving it to each row that draws one: an Item built with
    // visible already true writes true over true and emits no visibleChanged, so a delegate
    // constructed mid-rename came up empty with nothing holding the caret.
    Component.onCompleted: if (root.visible && !root.begun) root.begin()
    Component.onDestruction: {
        lifecycle.stop()
        if (root.containOnBegin && root.editViewport && root.editViewport.retireRenameEditor) {
            // The viewport retains values only, never a method or context belonging to this field.
            var retirement = root.ownsEdit() ? {pane: root.editPane, viewport: root.editViewport,
                index: root.editIndex, name: root.editName, text: field.text, cursor: field.cursorPosition,
                anchor: field.cursorPosition === field.selectionStart ? field.selectionEnd : field.selectionStart,
                focused: field.activeFocus || root.viewportOwnsFocus(),
                request: root.editPane ? root.editPane.renameRequest : null} : null
            root.begun = false
            root.editViewport.retireRenameEditor(root, retirement)
        } else {
            root.abandon()
            if (root.containOnBegin && root.editViewport && root.editViewport.renameEditor === root)
                root.editViewport.renameEditor = null
        }
    }

    // Hiding is abandoning. Qt drops effective visibility before it emits this, so the focus handler
    // below can never see the case, and a hidden editor left renamingIndex set with nothing alive to
    // clear it, which killed the whole window's keyboard, escape included.
    onVisibleChanged: {
        if (root.visible) {
            root.begin()
            return
        }
        if (root.begun) lifecycle.restart()
    }

    function abandon() {
        if (!root.ownsEdit() || root.pending) return
        root.begun = false
        // The enclosing ListView is a focus scope and remembers this field as its focused child, so
        // giving up what begin() took is what lets the scope itself take the keys again.
        field.focus = false
        root.abandoned()
    }

    // Child timers recheck ownership after layout and die with the field, so stale editors cannot cancel or refocus replacements.
    Timer {
        id: lifecycle
        interval: 0
        onTriggered: {
            var contain = root.containmentQueued
            root.containmentQueued = false
            if (!root.ownsEdit()) return
            if (root.containOnBegin && root.editorHost && root.viewport
                    && root.viewport.visible && !root.viewport.hiddenHeld) {
                var host = root.layoutHost()
                if (host && host !== root.editorHost) {
                    host.editorField.begin(true)
                    return
                }
            }
            if (!root.visible || !field.activeFocus) {
                if (!root.visible) field.focus = false
                root.abandon()
                return
            }
            if (contain) root.revealEditor()
            if (!root.ownsEdit() || root.pending || !root.viewport || !root.viewport.visible
                    || root.viewport.hiddenHeld) return
            var top = root.mapToItem(root.viewport, 0, 0).y
            if (top + root.height > 0 && top < root.viewport.height) return
            root.abandon()
        }
    }
    function queueDeparture() {
        if (!root.ownsEdit() || root.pending || !root.viewport || !root.viewport.visible
                || root.viewport.hiddenHeld || root.containing) return
        lifecycle.restart()
    }
    Connections {
        target: root.viewport
        function onContentYChanged() { root.queueDeparture() }
    }

    Rectangle {
        id: editFrame
        width: parent.width
        height: root.fieldHeight
        color: Theme.color.background
        border.width: Theme.spacing.hairline
        border.color: root.errorText.length > 0 ? Theme.color.error : Theme.color.accent
    }

    FontMetrics {
        id: typedLine
        font: field.font
    }

    // The extra arguments are read only so a layout or a scroll re-runs the binding, as mutedExtension's x does.
    function edgeX(position, laidOut, scrolled) { return field.x + field.positionToRectangle(position).x }
    Item {
        id: selectionFill
        visible: field.selectionEnd > field.selectionStart && (field.activeFocus || field.persistentSelection)
        readonly property real fromX: Math.max(field.x, root.edgeX(field.selectionStart, field.contentWidth, field.cursorRectangle.x))
        readonly property real toX: Math.min(field.x + field.width, root.edgeX(field.selectionEnd, field.contentWidth, field.cursorRectangle.x))
        x: fromX
        y: Theme.spacing.hairline
        width: Math.max(0, toX - fromX)
        height: root.fieldHeight - 2 * Theme.spacing.hairline

        Rectangle {
            anchors.fill: parent
            color: Theme.color.accent
        }
    }

    TextInput {
        id: field
        anchors { left: parent.left; right: parent.right; top: parent.top }
        height: root.fieldHeight
        anchors.leftMargin: Theme.spacing.gap
        anchors.rightMargin: Theme.spacing.gap
        verticalAlignment: TextInput.AlignVCenter
        color: Theme.color.foreground
        // The selection is painted by selectionFill, which fills the interior where the field's own stops at its line.
        selectionColor: "transparent"
        selectedTextColor: Theme.color.background
        font.family: Theme.font.family
        font.hintingPreference: Font.PreferFullHinting
        font.pixelSize: Theme.font.body
        clip: true
        readOnly: root.pending
        // Grid can drop child focus before pooling; preserve the selection its replacement inherits.
        persistentSelection: root.containOnBegin

        // Both keys are handled and accepted here rather than through onAccepted, because an
        // unaccepted Return goes on to the list's own Keys handler, which reads it as "open" and
        // tries to open the row under a name the rename has just taken away.
        Keys.onPressed: function (event) {
            if (event.key === Qt.Key_Escape) {
                root.abandon()
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
        onActiveFocusChanged: if (!activeFocus && root.begun) lifecycle.restart()
    }

    Text {
        id: errorLabel
        anchors { top: field.bottom; left: parent.left }
        anchors.topMargin: Theme.spacing.gap
        width: root.errorSpan >= 0 ? root.errorSpan : root.width
        visible: root.errorText.length > 0
        text: root.errorText
        color: Theme.color.error
        font.family: Theme.font.family
        font.hintingPreference: Font.PreferFullHinting
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
        // One hairline in from the frame's top and bottom, so the patch never covers the frame's sides.
        y: field.y + Theme.spacing.hairline
        width: Math.max(0, field.width - (x - field.x))
        height: field.height - 2 * Theme.spacing.hairline
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
            font.hintingPreference: field.font.hintingPreference
            font.pixelSize: field.font.pixelSize
            textFormat: Text.PlainText
        }
    }
}
