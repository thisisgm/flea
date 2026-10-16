//@ pragma ShellId flea-font-pane-test
import QtQuick
import Quickshell
import "flea" as Flea

// Real pane, backend and key routing: Enter opens internally, j follows when a button has focus, and text keys stay in the editor.
ShellRoot {
    id: root
    property int stage: 0
    property int ticks: 0
    property int checks: 0
    property int failed: 0
    property var keys: null
    readonly property var pane: body.item ? body.item.currentPane : null
    readonly property var preview: body.item ? body.item.quickLook : null
    function check(ok, message) { checks++; if (!ok) { failed++; console.log("FONT FAIL " + message) } }
    function finish() {
        tick.stop()
        console.log("FONT DONE checks=" + checks + " failed=" + failed)
        Quickshell.execDetached(["kill", String(Quickshell.processId)])
    }
    FloatingWindow {
        id: window
        implicitWidth: 1000
        implicitHeight: 750
        visible: true
        Loader {
            id: body
            anchors.fill: parent
            source: "flea/WindowBody.qml"
            onLoaded: item.host = window
        }
    }
    Timer {
        id: tick
        interval: 100
        running: true
        repeat: true
        onTriggered: {
            if (++root.ticks > 350) { root.check(false, "pane timed out at stage " + root.stage); root.finish(); return }
            var p = root.pane, q = root.preview
            if (!p || p.listInFlight || p.listingState !== "ready") return
            if (q && q.active && q.isFont && !q.advancedFont) {
                if (q.status === "loading") return
                root.check(q.status === "font", "real backend draws the isolated quick sample")
                root.check(q.fontManageControl.visible, "real pane offers management over the upstream sample")
                root.keys.keyClick(Qt.Key_Tab, Qt.NoModifier, -1)
                root.check(q.fontManageControl.activeFocus, "Tab reaches font management")
                root.keys.keyClick(Qt.Key_Return, Qt.NoModifier, -1)
                return
            }
            if (root.stage === 0 && p.total === 2) {
                root.keys = Qt.createQmlObject("import QtTest; TestEvent {}", window.contentItem)
                p.listArea.forceActiveFocus()
                root.keys.keyClick(Qt.Key_Return, Qt.NoModifier, -1)
                root.stage++
            } else if (root.stage === 1 && q && q.status === "font" && q.fontItem && !q.fontItem.busy) {
                root.check(q.active && q.isFont, "Enter opened the real pane's font preview")
                q.fontItem.focusFirst()
                root.keys.keyClick(Qt.Key_J, Qt.NoModifier, -1)
                root.stage++
            } else if (root.stage === 2 && p.cursorIndex === 1 && q.path.endsWith(Quickshell.env("FONT_SECOND")) && q.status === "font" && q.fontItem && !q.fontItem.busy) {
                root.check(q.path.endsWith(Quickshell.env("FONT_SECOND")), "j follows the next font from a focused control")
                var edit = q.fontItem.sampleInput
                edit.forceActiveFocus()
                edit.text = "X"
                edit.cursorPosition = 1
                root.keys.keyClick(Qt.Key_Space, Qt.NoModifier, -1)
                root.keys.keyClick(Qt.Key_J, Qt.NoModifier, -1)
                root.check(edit.text === "X j" && q.active && p.cursorIndex === 1, "typing stays in editor with the real pane behind it: " + edit.text)
                root.keys.keyClick(Qt.Key_Escape, Qt.NoModifier, -1)
                root.stage++
            } else if (root.stage === 3) {
                root.check(!q.active && p.listArea.activeFocus, "Escape returns focus to the listing")
                root.finish()
            }
        }
    }
}
