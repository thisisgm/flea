import QtQuick
import Quickshell
import "." as Test

Item {
    id: root
    width: 100
    height: 100

    property int checked: 0
    property var failures: []

    function check(label, actual, expected) {
        root.checked++
        if (actual !== expected)
            root.failures.push(label + ": got " + actual + ", expected " + expected)
    }

    Flickable {
        id: target
        width: 100
        height: 100
        contentWidth: width
        contentHeight: 500
    }

    Test.FastScrollHandler {
        id: handler
        parent: root
        flickable: target
        mouseWheelStep: 72
    }

    Component.onCompleted: {
        target.contentY = 200
        root.check("smooth pixels retain their fractional value and accelerate",
                   handler.scrollByDeltas(10.5, 120), 158)
        root.check("pixel input wins when a device reports both delta forms", target.contentY, 158)

        target.contentY = 200
        root.check("one wheel notch follows the line preference and accelerates",
                   handler.scrollByDeltas(0, 120), 0)
        target.contentY = 100
        root.check("wheel direction reverses exactly", handler.scrollByDeltas(0, -120), 388)

        target.contentY = 12
        root.check("the top boundary is hard", handler.scrollByDeltas(20, 0), 0)
        target.contentY = 390
        root.check("the bottom boundary is hard", handler.scrollByDeltas(-20, 0), 400)
        root.check("zero and horizontal-only input do nothing", handler.scrollByDeltas(0, 0), 400)

        target.interactive = false
        root.check("a disabled viewport does not move", handler.scrollByDeltas(0, 120), 400)
        target.interactive = true
        target.contentHeight = 50
        root.check("content shorter than its viewport has no scroll range",
                   handler.scrollByDeltas(0, -120), 0)

        for (var i = 0; i < root.failures.length; i++)
            console.log("FAIL " + root.failures[i])
        console.log(root.checked + " checks, " + root.failures.length + " failed")
        Quickshell.execDetached(["kill", String(Quickshell.processId)])
    }
}
