//@ pragma ShellId flea-font-formats-test
import QtQuick
import Quickshell
import "flea" as Flea
ShellRoot {
    id: root
    readonly property var cases: JSON.parse(Quickshell.env("FONT_CASES"))
    property int index: 0
    property int phase: 0
    property int ticks: 0
    property int checks: 0
    property int failed: 0
    function check(ok, message) { checks++; if (!ok) { failed++; console.log("FONT FAIL " + message) } }
    function finish() { tick.stop(); console.log("FONT DONE checks=" + checks + " failed=" + failed); Quickshell.execDetached(["kill", String(Quickshell.processId)]) }
    FloatingWindow {
        implicitWidth: 1100
        implicitHeight: 850
        Flea.Preview { id: preview }
    }
    Timer {
        id: tick
        interval: 100
        repeat: true
        running: true
        onTriggered: {
            if (++root.ticks > 350) { root.check(false, "format timed out " + cases[root.index] + " status=" + preview.status); root.finish(); return }
            if (root.phase === 0) {
                preview.show(cases[root.index], "font-x-generic", 0, "Font", "", -1)
                if (!preview.advancedFont) preview.fontManageControl.activated()
                root.phase = 1
                return
            }
            var item = preview.fontItem
            if (!item || item.busy || preview.status === "loading") return
            root.check(preview.status === "font", "Qt rendered " + cases[root.index] + " status=" + preview.status)
            if (item.faces.length > 1) {
                item.chooseFace(1)
                root.check(item.faceIndex === 1 && item.sampleInput.font.family === item.faces[1].family, "collection selector chooses another family")
            }
            if (item.axes.length > 0) {
                var axis = item.axes[0]
                item.setAxis(axis.tag, axis.maximum)
                root.check(item.sampleInput.font.variableAxes[axis.tag] === axis.maximum, "variable axis reaches the specimen font")
            }
            console.log("FONT FORMAT " + cases[root.index] + " faces=" + item.faces.length + " axes=" + item.axes.length + " status=" + preview.status)
            preview.close()
            root.phase = 0
            root.ticks = 0
            if (++root.index === cases.length) root.finish()
        }
    }
}
