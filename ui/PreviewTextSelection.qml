import QtQuick

// One pointer owner for the document. Only existing text delegates participate;
// native selection documents are created on contact, never on first paint.
MouseArea {
    id: root
    required property var document
    required property Item textRoot
    acceptedButtons: Qt.LeftButton
    preventStealing: true
    cursorShape: Qt.IBeamCursor
    property var origin: null
    property int anchor: 0
    property bool moved: false
    property var selected: []
    property string selectedText: ""
    property var clipboard: null

    function texts() {
        var out = [], stack = [root.textRoot]
        while (stack.length) {
            var item = stack.pop()
            if (!item || !item.visible) continue
            if (item.selectionEditor !== undefined) out.push(item)
            var children = item.children || []
            for (var i = 0; i < children.length; i++) stack.push(children[i])
        }
        out.sort(function(a, b) {
            var pa = a.mapToItem(root, 0, 0), pb = b.mapToItem(root, 0, 0)
            return Math.abs(pa.y - pb.y) > 1 ? pa.y - pb.y : pa.x - pb.x
        })
        return out
    }
    function editorFor(item) {
        if (item.selectionEditor === null)
            item.selectionEditor = Qt.createComponent("PreviewSelectionDocument.qml").createObject(item, { host: item, controller: root })
        return item.selectionEditor
    }
    function nearest(x, y, inside) {
        var items = root.texts(), found = null, distance = Infinity
        for (var i = 0; i < items.length; i++) {
            var p = items[i].mapFromItem(root, x, y)
            var dx = Math.max(0, -p.x, p.x - items[i].width)
            var dy = Math.max(0, -p.y, p.y - items[i].height)
            var d = dx * dx + dy * dy
            if (d < distance) { found = items[i]; distance = d }
        }
        return inside && distance > 0 ? null : found
    }
    function position(item, x, y) {
        var p = item.mapFromItem(root, x, y), edit = root.editorFor(item)
        return p.y < 0 ? 0 : p.y >= item.height ? edit.length : edit.positionAt(p.x, p.y)
    }
    function clear() {
        for (var i = 0; i < root.selected.length; i++)
            if (root.selected[i] && root.selected[i].selectionEditor) root.selected[i].selectionEditor.deselect()
        if (root.origin && root.origin.selectionEditor) root.origin.selectionEditor.deselect()
        root.selected = []
        root.selectedText = ""
        root.origin = null
    }
    function extend(x, y) {
        if (!root.origin) return
        var nearest = root.nearest(x, y, false)
        if (!nearest) return
        var items = root.texts(), end = root.position(nearest, x, y)
        var startIndex = items.indexOf(root.origin), endIndex = items.indexOf(nearest)
        if (startIndex < 0) return
        var forward = endIndex > startIndex || (endIndex === startIndex && end >= root.anchor)
        var first = Math.min(startIndex, endIndex), last = Math.max(startIndex, endIndex)
        var chosen = [], pieces = []
        for (var k = 0; k < items.length; k++) {
            if (k < first || k > last) {
                if (items[k].selectionEditor) items[k].selectionEditor.deselect()
                continue
            }
            var editor = root.editorFor(items[k])
            var lo = k === first ? (forward ? root.anchor : end) : 0
            var hi = k === last ? (forward ? end : root.anchor) : editor.length
            editor.select(lo, hi)
            chosen.push(items[k])
            pieces.push(editor.selectedText)
        }
        root.selected = chosen
        root.selectedText = pieces.join("\n")
    }
    function keyboardSelection(item) {
        for (var i = 0; i < root.selected.length; i++)
            if (root.selected[i] && root.selected[i] !== item && root.selected[i].selectionEditor) root.selected[i].selectionEditor.deselect()
        root.origin = item
        root.selected = [item]
        root.selectedText = ""
    }
    function copy() {
        if (root.selectedText === "") {
            if (root.origin && root.origin.selectionEditor) root.origin.selectionEditor.copy()
            return
        }
        if (root.clipboard === null) root.clipboard = clipboardFactory.createObject(root)
        root.clipboard.text = root.selectedText
        root.clipboard.selectAll()
        root.clipboard.copy()
        root.clipboard.text = ""
    }
    onPressed: function(mouse) {
        var item = root.nearest(mouse.x, mouse.y, true)
        if (!item) { mouse.accepted = false; return }
        root.clear()
        root.origin = item
        var edit = root.editorFor(item)
        root.anchor = root.position(item, mouse.x, mouse.y)
        edit.cursorPosition = root.anchor
        edit.forceActiveFocus()
        root.moved = false
    }
    onPositionChanged: function(mouse) {
        if (!root.pressed || !root.origin) return
        root.extend(mouse.x, mouse.y)
        root.moved = root.moved || root.selectedText !== ""
    }
    onReleased: function(mouse) {
        if (!root.origin || root.moved) return
        var p = root.origin.mapFromItem(root, mouse.x, mouse.y)
        var link = root.origin.linkAt(p.x, p.y)
        if (link !== "" && typeof root.origin.linkGate === "function" && root.origin.linkGate(link)) Qt.openUrlExternally(link)
    }
    onDoubleClicked: function(mouse) {
        if (!root.origin) return
        var edit = root.editorFor(root.origin)
        edit.cursorPosition = root.position(root.origin, mouse.x, mouse.y)
        edit.selectWord()
        root.selectedText = edit.selectedText
        root.moved = true
    }
    onVisibleChanged: if (!root.visible) root.clear()
    Component { id: clipboardFactory; TextEdit { visible: false; textFormat: TextEdit.PlainText } }
    Connections {
        target: root.document
        function onPathChanged() { root.clear() }

    }
}
