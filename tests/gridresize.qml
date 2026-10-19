//@ pragma ShellId flea-gridresize-test
import QtQuick
import QtTest
import Quickshell
import "flea" as Flea

// A Grid resized while scrolled keeps every drawn tile's row, after a width reflow or a height grow.
ShellRoot {
    id: root
    readonly property string fixture: Quickshell.env("FLEA_GV_DIR") || ""
    readonly property int entries: Number(Quickshell.env("FLEA_GV_ENTRIES") || "0")
    readonly property string caseName: Quickshell.env("FLEA_GR_CASE") || "narrow-far"
    readonly property real wideWidth: 1200
    readonly property real narrowWidth: 800
    readonly property real oneColumnWidth: 300
    readonly property real startWidth: caseName === "widen-mid" ? oneColumnWidth : wideWidth
    readonly property int scrollMid: 800
    readonly property int scrollFar: 1500
    property int checks: 0
    property int failures: 0
    property var pane: null
    property var steps: []
    property var waitFor: null
    property double waitUntil: 0
    property double deadline: 0
    property int clicked: -1
    property int drawnCount: 0
    property int blankCount: 0
    property int wrongCount: 0
    property int firstDrawn: -1
    property int startColumns: 0
    property var heightStart: null

    function log(line) { console.log("GRIDRESIZE " + line) }
    function fail(text) { failures += 1; log("FAIL " + text) }
    function check(name, ok, detail) {
        checks += 1
        if (!ok) fail(name + " " + detail)
        else log("CHECK " + name + " " + detail)
    }
    function finish() {
        ticker.stop()
        log("DONE " + checks + " checks, " + failures + " failed")
        Qt.quit()
    }
    // Names follow the fixture's sort: file-0001.txt is row 0.
    function expectName(index) {
        var n = String(index + 1)
        while (n.length < 4) n = "0" + n
        return "file-" + n + ".txt"
    }
    function grid() { return pane ? pane.listArea : null }
    function whole() {
        var g = grid()
        return pane !== null && pane.viewMode === "grid" && g !== null && g.visible && !g.hiddenHeld && g.count > 0 && g.itemAtIndex(0) !== null
    }
    function landed() { return pane && !pane.listInFlight && !pane.swap.holding && pane.total === entries }
    // Tiles the Grid draws, counted by what they show: a drawn tile with no row is the blank one.
    function drawn() {
        var g = grid(), out = { drawn: 0, blank: 0, wrong: 0, first: -1, last: -1 }
        for (var i = 0; i < pane.total; i++) {
            if (g.itemAtIndex(i) === null) continue
            var row = pane.rowFor(i)
            out.drawn += 1
            if (out.first < 0) out.first = i
            out.last = i
            if (row === null) out.blank += 1
            else if (row.n !== expectName(i)) out.wrong += 1
        }
        return out
    }
    function allDrawnHaveRows() { var d = drawn(); return d.drawn > 0 && d.blank === 0 && d.wrong === 0 }
    // The height grow's own window reply: only the window it asks for reaches past the one held before it.
    function grownWindowLanded() { return pane.held + pane.rows.length > heightStart.heldEnd }
    function scanTo(label) {
        var d = drawn()
        drawnCount = d.drawn; blankCount = d.blank; wrongCount = d.wrong; firstDrawn = d.first
        log(label + " drawn=" + d.drawn + " blank=" + d.blank + " wrong=" + d.wrong + " range=" + d.first + ".." + d.last
            + " cols=" + grid().columns + " height=" + grid().height + " visibleRows=" + grid().visibleTileRows
            + " originY=" + grid().originY + " contentY=" + grid().contentY + " held=" + pane.held + " len=" + pane.rows.length)
    }
    // A click on the icon of the first drawn tile that has its row and whose icon centre is on screen.
    function clickFirstWithRow() {
        var g = grid()
        for (var i = 0; i < pane.total; i++) {
            var tile = g.itemAtIndex(i)
            if (tile === null || pane.rowFor(i) === null) continue
            var p = tile.thumbItem.mapToItem(g, tile.thumbItem.width / 2, tile.thumbItem.height / 2)
            if (p.x < 0 || p.x >= g.width || p.y < 0 || p.y >= g.height) continue
            clicked = i
            keys.mouseClick(g, p.x, p.y, Qt.LeftButton, Qt.NoModifier, -1)
            return
        }
        clicked = -1
    }
    // The checks every case ends on: no drawn tile is blank or holds another row.
    function checkDrawnRows(label) {
        scanTo(label)
        check(label + ": every drawn tile has its row", drawnCount > 0 && blankCount === 0 && wrongCount === 0,
            "drawn=" + drawnCount + " blank=" + blankCount + " wrong=" + wrongCount)
    }

    FloatingWindow {
        id: win
        implicitWidth: root.wideWidth
        implicitHeight: 800
        Flea.Backend { id: probeBackend }
        Component {
            id: paneComponent
            Flea.Pane {
                width: win.width
                height: win.height
                backend: probeBackend
            }
        }
        TestEvent { id: keys }
    }

    Component.onCompleted: {
        var open = [
            function () { pane = paneComponent.createObject(win.contentItem); pane.open(fixture); if (root.startWidth !== root.wideWidth) pane.width = root.startWidth; waitFor = landed; waitUntil = Date.now() + 15000 },
            function () { pane.listArea.forceActiveFocus(); keys.keyClick(Qt.Key_3, Qt.ControlModifier, -1); waitFor = whole; waitUntil = Date.now() + 5000 },
            function () { waitUntil = Date.now() + 800 },
            function () { startColumns = grid().columns; scanTo("start") }
        ]
        var narrowFar = [
            function () { grid().positionViewAtIndex(root.scrollMid, GridView.Beginning); waitFor = allDrawnHaveRows; waitUntil = Date.now() + 4000 },
            function () { scanTo("wide-middle") },
            function () { pane.width = root.narrowWidth; waitUntil = Date.now() + 1500 },
            function () {
                check("narrow: the Grid re-flowed", grid().columns < startColumns && grid().originY !== 0,
                    "cols " + startColumns + " -> " + grid().columns + " originY=" + grid().originY)
                scanTo("narrow-middle")
            },
            function () { grid().positionViewAtIndex(root.scrollFar, GridView.Beginning); waitFor = allDrawnHaveRows; waitUntil = Date.now() + 4000 },
            function () {
                checkDrawnRows("narrow-far")
                clickFirstWithRow()
                waitUntil = Date.now() + 400
            },
            function () {
                check("narrow: a click on a far tile selects it", clicked >= 0 && pane.cursorIndex === clicked,
                    "clicked=" + clicked + " cursor=" + pane.cursorIndex)
            }
        ]
        var narrowStill = [
            function () { grid().positionViewAtIndex(root.scrollMid, GridView.Beginning); waitFor = allDrawnHaveRows; waitUntil = Date.now() + 4000 },
            function () { pane.width = root.narrowWidth; waitUntil = Date.now() + 1500 },
            function () {
                check("narrow-still: the Grid re-flowed", grid().columns < startColumns && grid().originY !== 0,
                    "cols " + startColumns + " -> " + grid().columns + " originY=" + grid().originY)
                checkDrawnRows("narrow-still")
            }
        ]
        var widenMid = [
            function () { grid().positionViewAtIndex(root.scrollMid, GridView.Beginning); waitFor = allDrawnHaveRows; waitUntil = Date.now() + 4000 },
            function () { scanTo("one-column-middle") },
            function () { pane.width = root.wideWidth; waitUntil = Date.now() + 1500 },
            function () {
                check("widen: the Grid re-flowed", grid().columns > startColumns,
                    "cols " + startColumns + " -> " + grid().columns + " originY=" + grid().originY)
                checkDrawnRows("widen-mid")
            }
        ]
        var heightMid = [
            function () { grid().positionViewAtIndex(root.scrollMid, GridView.Beginning); waitFor = allDrawnHaveRows; waitUntil = Date.now() + 4000 },
            function () {
                scanTo("short-middle")
                heightStart = { width: pane.width, rows: grid().visibleTileRows, y: grid().contentY,
                    origin: grid().originY, heldEnd: pane.held + pane.rows.length }
                pane.height = 6000
                // The checks wait for that reply however long a loaded box takes to lay out and answer the grow; one never sent stalls the case.
                waitFor = grownWindowLanded
            },
            function () {
                var g = grid(), range = g.visibleRange()
                check("height-mid: only height changed", pane.width === heightStart.width && g.columns === startColumns
                    && g.visibleTileRows > heightStart.rows && g.contentY === heightStart.y && g.originY === heightStart.origin,
                    "cols " + startColumns + " -> " + g.columns + " rows " + heightStart.rows + " -> " + g.visibleTileRows
                    + " contentY " + heightStart.y + " -> " + g.contentY + " originY " + heightStart.origin + " -> " + g.originY)
                check("height-mid: the grow needs rows past the old window", range.last >= heightStart.heldEnd,
                    "visible=" + range.first + ".." + range.last + " oldEnd=" + heightStart.heldEnd)
                check("height-mid: the held window covers the visible range", range.first >= pane.held && range.last < pane.held + pane.rows.length,
                    "visible=" + range.first + ".." + range.last + " held=" + pane.held + ".." + (pane.held + pane.rows.length - 1))
                checkDrawnRows("height-mid")
            }
        ]
        var cases = { "narrow-far": narrowFar, "narrow-still": narrowStill, "widen-mid": widenMid, "height-mid": heightMid }
        var body = cases[root.caseName]
        if (body === undefined) { fail("unknown FLEA_GR_CASE " + root.caseName); finish(); return }
        steps = open.concat(body)
        steps.push(function () { finish() })
        deadline = Date.now() + 60000
    }

    Timer {
        id: ticker
        interval: 10
        running: true
        repeat: true
        onTriggered: {
            if (Date.now() > root.deadline) {
                root.fail("stalled with " + root.steps.length + " steps left")
                root.finish()
                return
            }
            if (root.waitFor ? !root.waitFor() && (root.waitUntil === 0 || Date.now() < root.waitUntil)
                             : Date.now() < root.waitUntil) return
            root.waitFor = null
            root.waitUntil = 0
            if (root.steps.length === 0) return
            var step = root.steps[0]
            root.steps = root.steps.slice(1)
            try {
                step()
            } catch (error) {
                root.fail("a step threw: " + error)
                root.finish()
            }
        }
    }
}
