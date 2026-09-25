//@ pragma ShellId flea-refresh-menu-test
import QtQuick
import Quickshell
import "ui" as Flea
import "ui/js/Anchor.js" as Anchor

// Real focus transfers between the menu and rename field: a plain JS pane cannot lose Qt focus.
ShellRoot {
    id: test
    property int failures: 0
    property int checks: 0
    function check(label, condition) {
        checks++
        if (!condition) {
            failures++
            console.log("FAIL " + label)
        }
    }
    FloatingWindow {
        implicitWidth: 900
        implicitHeight: 600
        Item {
            id: pane
            anchors.fill: parent
            property bool editing: false
            property bool menuVisible: menu.opened
            property string renameError: ""
            property bool renamePending: false
            property string searchMode: ""
            property bool searchRunning: false
            property bool listInFlight: false
            property var collide: ({ pending: null })
            property int writes: 0
            property string said: ""
            function renameEditor() { return editing ? editor : null }
            function message(text, error) { said = text }
            function openWithoutHistory() { writes++ }
            function rowFor(index) { return null }
            Flea.RenameField {
                id: editor
                width: 280
                height: 40
                name: "original.txt"
                visible: pane.editing
                pane: pane
                onAbandoned: pane.editing = false
            }
            Item { id: other; x: 400; width: 100; height: 40 }
            Flea.ContextMenu {
                id: menu
                focusOwner: other
                onChosen: function(action) {
                    if (action === "refresh") Anchor.manual(pane)
                }
            }
        }
    }
    Timer {
        interval: 100
        running: true
        onTriggered: {
            for (var mode of ["", "results"]) {
                pane.searchMode = mode
                pane.said = ""
                pane.editing = true
                editor.begin()
                editor.inputItem.text = "unfinished name.txt"
                test.check("rename takes focus " + mode, editor.inputItem.activeFocus)
                menu.openBackground(Qt.point(320, 100))
                test.check("background menu takes focus " + mode, menu.keyboardFocused)
                test.check("menu opening retains draft " + mode,
                           pane.editing && editor.current === "unfinished name.txt")
                if (!pane.editing) { menu.close(); continue }
                menu.choose("refresh")
                test.check("Refresh closes menu and restores editor focus " + mode,
                           !menu.opened && editor.inputItem.activeFocus)
                test.check("Refresh retains draft and refuses re-read " + mode,
                           pane.editing && editor.current === "unfinished name.txt"
                           && pane.writes === 0 && pane.said === "Finish or cancel the rename first.")
                menu.openBackground(Qt.point(320, 100))
                menu.close()
                test.check("dismissing menu restores draft focus " + mode,
                           pane.editing && editor.inputItem.activeFocus)
                other.forceActiveFocus()
                test.check("ordinary focus loss still abandons " + mode, !pane.editing)
            }
            console.log("refresh-menu: " + test.checks + " checks, " + test.failures + " failed")
            Quickshell.execDetached(["kill", String(Quickshell.processId)])
        }
    }
}
