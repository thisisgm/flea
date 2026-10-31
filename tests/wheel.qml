//@ pragma ShellId flea-wheel-test

import QtQuick
import QtTest
import Quickshell
import "flea" as Flea
import "flea/js/ActionWheel.js" as Wheel

// tests/wheel.sh's harness: the real ui/ActionWheel.qml over a stub pane. It checks the root
// actions, the Tangerine format conversion sub-entries, the annulus hit test, keys, pointer
// clicks, focus restore and routes handed to the pane; offscreen.
ShellRoot {
    id: shell

    readonly property int layoutMs: 300
    property int checks: 0
    property var failures: []
    property int closedCount: 0
    property string chosenId: ""

    function check(label, actual, expected) {
        shell.checks += 1
        if (actual === expected) {
            console.log("WHEEL ok " + label)
            return
        }
        shell.failures.push(label)
        console.log("WHEEL FAIL " + label + ": got " + JSON.stringify(actual) + ", expected " + JSON.stringify(expected))
    }
    function quit() { Quickshell.execDetached(["kill", String(Quickshell.processId)]) }
    function report() {
        var verdict = shell.failures.length === 0 ? "PASS" : "FAIL"
        console.log("WHEEL " + verdict + " " + shell.checks + " checks, " + shell.failures.length + " failed")
        shell.quit()
    }

    function allNamed(item, name, out) {
        var found = out || []
        for (var i = 0; i < item.children.length; i++) {
            var child = item.children[i]
            if (child.objectName === name) found.push(child)
            shell.allNamed(child, name, found)
        }
        return found
    }
    function oneNamed(item, name) {
        var found = shell.allNamed(item, name)
        return found.length === 1 ? found[0] : null
    }
    function pathOf(petal) {
        return petal.outlinePath || ""
    }
    function pointAt(index, count) {
        var n = count || wheel.actionCount
        var radius = (Wheel.GEOMETRY.inner + Wheel.GEOMETRY.outer) / 2
        return Wheel.pointAt(radius, Wheel.angleFor(index, n))
    }
    function clickPetal(index) {
        var dial = shell.oneNamed(wheel, "actionWheelDial")
        var point = shell.pointAt(index, wheel.actionCount)
        keys.mouseClick(dial, wheel.backdrop + point.x, wheel.backdrop + point.y, Qt.LeftButton, Qt.NoModifier, -1)
    }

    FloatingWindow {
        id: window
        implicitWidth: 600
        implicitHeight: 500
        color: Flea.Theme.color.background

        Item {
            id: parking
            anchors.fill: parent
            focus: true
        }

        Flea.ActionWheel {
            id: wheel
            anchors.fill: parent
            pane: mockPane
        }

        TestEvent { id: keys }
    }

    QtObject {
        id: mockPane
        property var listArea: parking
        property var acted: []
        property var sheets: []
        property var rows: [{ n: "sample.jpg", d: false }]
        property int cursorIndex: 0
        property string path: "/home/user"
        function rowFor(idx) { return rows[idx] }
        function join(dir, file) { return dir + "/" + file }
        function act(action) { acted.push(action) }
        function sheetMenuAction(action) { sheets.push(action) }
        property var backend: QtObject {
            function convertImage(path, dest, strip, menuId, requestId, check) {}
        }
    }

    Connections {
        target: wheel
        function onClosed() { shell.closedCount += 1 }
        function onChosen(action) { shell.chosenId = action }
    }

    Timer {
        interval: shell.layoutMs
        running: true
        repeat: false
        onTriggered: shell.run()
    }

    function run() {
        shell.check("the wheel owns root actions including convert", wheel.actionCount, 7)
        shell.check("Tangerine's inner rim", wheel.inner, 48.64)
        shell.check("Tangerine's outer rim", wheel.outer, 119.52)
        shell.check("Tangerine's backdrop", wheel.backdrop, 125.92)
        shell.check("it starts closed", wheel.opened, false)
        shell.check("and draws nothing", wheel.visible, false)

        var petals = shell.allNamed(wheel, "actionWheelPetal")
        var captions = shell.allNamed(wheel, "actionWheelCaption")
        shell.check("one shape per action", petals.length, 7)
        shell.check("one caption per action", captions.length, 7)
        shell.check("the hub is drawn once", shell.allNamed(wheel, "actionWheelHub").length, 1)
        var paths = petals.map(shell.pathOf)
        var broken = paths.filter(function (path) {
            return path.indexOf("M ") !== 0 || path.indexOf(" A ") < 0 || path.slice(-2) !== " Z" || /NaN|undefined/.test(path)
        })
        shell.check("every petal is a closed SVG path", broken.length, 0)
        shell.check("no two petals share a path", paths.filter(function (path, index) { return paths.indexOf(path) !== index }).length, 0)

        parking.forceActiveFocus()
        shell.check("the listing holds the keyboard first", parking.activeFocus, true)
        wheel.openAt(Qt.point(300, 250))
        shell.check("the wheel opens", wheel.opened, true)
        shell.check("and becomes visible", wheel.visible, true)
        shell.check("it sits under the pointer", wheel.wheelX + "," + wheel.wheelY, "300,250")
        shell.check("nothing is lit at rest", wheel.highlighted, -1)
        shell.check("the wheel takes the keyboard", shell.oneNamed(wheel, "actionWheelKeys").activeFocus, true)

        var misses = []
        for (var i = 0; i < wheel.actionCount; i++) {
            var point = shell.pointAt(i, wheel.actionCount)
            wheel.pointerAt(wheel.backdrop + point.x, wheel.backdrop + point.y)
            if (wheel.highlighted !== i) misses.push(i)
        }
        shell.check("every painted petal answers its own middle", misses.join(","), "")
        wheel.pointerAt(wheel.backdrop, wheel.backdrop)
        shell.check("the hub lights no petal", wheel.highlighted, -1)

        keys.keyClick(Qt.Key_Escape, Qt.NoModifier, -1)
        shell.check("Escape closes the wheel", wheel.opened, false)
        shell.check("and gives the keyboard back", parking.activeFocus, true)
        shell.check("the close is reported once", shell.closedCount, 1)

        wheel.openAt(Qt.point(2, 2))
        var margin = wheel.backdrop + 10
        shell.check("a corner click keeps the whole backdrop inside the pane",
                    wheel.wheelX + "," + wheel.wheelY, margin + "," + margin)

        keys.keyClick(Qt.Key_Right, Qt.NoModifier, -1)
        shell.check("Right lights the first petal", wheel.highlighted, 0)
        keys.keyClick(Qt.Key_Right, Qt.NoModifier, -1)
        shell.check("and steps clockwise", wheel.highlighted, 1)
        keys.keyClick(Qt.Key_Left, Qt.NoModifier, -1)
        shell.check("Left steps back", wheel.highlighted, 0)
        keys.keyClick(Qt.Key_Left, Qt.NoModifier, -1)
        shell.check("and wraps to the last petal", wheel.highlighted, 6)
        keys.keyClick(Qt.Key_Return, Qt.NoModifier, -1)
        shell.check("Enter runs the lit petal", mockPane.acted.join(","), "openTerminal")
        shell.check("the wheel closes on a choice", wheel.opened, false)
        shell.check("the choice is reported", shell.chosenId, "terminal")
        shell.check("the listing gets the keyboard back after a choice", parking.activeFocus, true)

        mockPane.acted = []
        mockPane.sheets = []
        shell.chosenId = ""
        wheel.openAt(Qt.point(300, 250))
        shell.clickPetal(3)
        shell.check("a petal click closes the wheel", wheel.opened, false)
        shell.check("Shelf goes through the pane's menu snapshot", mockPane.sheets.join(","), "addToShelf")
        shell.check("and reports its own id", shell.chosenId, "shelf")

        // Sub-entries: clicking Convert petal opens format sub-wheel
        wheel.openAt(Qt.point(300, 250))
        shell.clickPetal(5)
        shell.check("clicking Convert switches to convert mode", wheel.mode, "convert")
        shell.check("wheel remains opened in sub-mode", wheel.opened, true)
        shell.check("sub-mode populates conversion format petals", wheel.actionCount, 7)
        keys.keyClick(Qt.Key_Escape, Qt.NoModifier, -1)
        shell.check("Escape in sub-mode returns to root actions", wheel.mode, "actions")
        shell.check("wheel stays open after returning to root actions", wheel.opened, true)
        keys.keyClick(Qt.Key_Escape, Qt.NoModifier, -1)
        shell.check("second Escape closes wheel", wheel.opened, false)

        // Opening directly in convert mode
        wheel.openAt(Qt.point(300, 250), "convert")
        shell.check("openAt accepts convert initial mode", wheel.mode, "convert")
        shell.clickPetal(0)
        shell.check("choosing format petal closes wheel", wheel.opened, false)
        shell.check("reports convert format choice", shell.chosenId, "convert:png")

        wheel.openAt(Qt.point(300, 250))
        keys.mouseClick(wheel, wheel.wheelX, wheel.wheelY, Qt.LeftButton, Qt.NoModifier, -1)
        shell.check("a hub click chooses nothing", wheel.opened, true)
        shell.check("and lights nothing", wheel.highlighted, -1)
        keys.keyClick(Qt.Key_Escape, Qt.NoModifier, -1)

        wheel.openAt(Qt.point(300, 250))
        keys.mouseClick(wheel, 8, 8, Qt.LeftButton, Qt.NoModifier, -1)
        shell.check("a click outside the dial closes the wheel", wheel.opened, false)

        settle.start()
    }

    Timer {
        id: settle
        interval: 250
        running: false
        repeat: false
        onTriggered: {
            shell.check("the closed wheel leaves nothing drawn", wheel.visible, false)
            shell.report()
        }
    }
}
