//@ pragma ShellId flea-preview-key-routing-test
import QtQuick
import QtTest
import Quickshell
import "flea" as Flea

// Preview and the pane are siblings as in WindowBody. The stub supplies listing
// state to the real Focus/PreviewKeys dispatch; directory rows avoid file loads.
ShellRoot {
    id: root
    property int stage: 0
    property int failures: 0
    function check(name, ok) {
        console.log("PREVIEWKEYS " + (ok ? "PASS " : "FAIL ") + name)
        if (!ok) root.failures++
    }
    QtObject {
        id: stub
        property var preview: quick
        property var sidebar: null
        property var backend: null
        property var rows: []
        property var thumbState: ({file: {}})
        property var listArea: sink
        property bool listInFlight: false
        property bool filterTyping: false
        property string searchMode: ""
        property string recentMode: ""
        property string filterQuery: ""
        property string path: "/fixture"
        property string viewMode: "list"
        property string keySequence: ""
        property string keySequenceIdentity: ""
        property double inputAt: 0
        property double rowsAt: 0
        property double trashArmedAt: 0
        property int selectionVersion: 0
        property int selectionAnchor: 0
        property int cursorIndex: 1
        property int cursorSeq: 0
        property int shownTotal: 3
        property var shown: null
        property var selection: ({follows: function() {return false}})
        function renameEditor() { return null }
        function showRow(index, context) {}
        function rowFor(index) { return {d: true, n: "folder"} }
    }
    Window {
        visible: true
        width: 640
        height: 360
        Item { id: sink; anchors.fill: parent }
        Flea.Preview {
            id: quick
            pane: stub
            Text { id: selectionHost; text: "copy this rendered text"; textFormat: Text.PlainText; visible: false }
            Flea.PreviewSelectionDocument { id: edit; host: selectionHost; z: 10 }
            Flea.SelectablePreviewText { id: source; visible: false; anchors.fill: parent; text: "copy this source text"; z: 20 }
        }
        TextEdit { id: receiver; visible: false; textFormat: TextEdit.PlainText }
        TestEvent { id: keys }
    }
    Timer {
        interval: 120
        running: true
        repeat: true
        onTriggered: {
            switch (root.stage++) {
            case 0:
                quick.active = true
                break
            case 1:
                edit.forceActiveFocus()
                edit.select(0, 4)
                keys.keyClick(Qt.Key_C, Qt.ControlModifier, -1)
                receiver.paste()
                root.check("copy stays in the selection editor", receiver.text === "copy" && stub.cursorIndex === 1 && quick.active && edit.activeFocus)
                keys.keyClick(Qt.Key_J, Qt.NoModifier, -1)
                root.check("j from focused rendered text reaches the pane", stub.cursorIndex === 2 && quick.active)
                keys.keyClick(Qt.Key_K, Qt.NoModifier, -1)
                root.check("k from focused rendered text reaches the pane", stub.cursorIndex === 1 && quick.active)
                keys.keyClick(Qt.Key_Space, Qt.NoModifier, -1)
                root.check("Space from focused rendered text closes Quick Look", !quick.active && sink.activeFocus)
                break
            case 2:
                quick.active = true
                edit.forceActiveFocus()
                keys.keyClick(Qt.Key_Escape, Qt.NoModifier, -1)
                root.check("Escape from focused rendered text closes Quick Look", !quick.active && sink.activeFocus)
                break
            case 3:
                quick.active = true
                source.visible = true
                source.editor.forceActiveFocus()
                keys.keyClick(Qt.Key_Escape, Qt.NoModifier, -1)
                root.check("Escape from source text closes Quick Look", !quick.active && sink.activeFocus)
                console.log("PREVIEWKEYS DONE failures=" + root.failures)
                Qt.quit()
            }
        }
    }
}
