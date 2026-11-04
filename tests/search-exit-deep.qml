//@ pragma ShellId flea-search-exit-deep-test

import QtQuick
import Quickshell
import "flea" as Flea

// Escape out of a live search begun on a row past the folder's first window lands the cursor on that row in the first frame.
ShellRoot {
    id: root

    readonly property string fixture: Quickshell.env("EXITDEEP_DIR")
    readonly property int reps: Math.max(1, parseInt(Quickshell.env("EXITDEEP_REPS") || "1"))
    // 60 folders, 8 notes, 300 misc and 300 north files, in the order a name sort with folders first lists them.
    readonly property int folderRows: 668
    // Inside the first window and below the viewport, past the first window, and the last row.
    readonly property var targets: [
        { depth: "shallow", name: "misc-100.dat", index: 167 },
        { depth: "middle", name: "north-150.txt", index: 517 },
        { depth: "deep", name: "north-300.txt", index: 667 }
    ]
    readonly property var top: ({ depth: "top", name: "box-01", index: 0 })
    readonly property int tickMs: 20
    readonly property int giveUpMs: 8000
    readonly property int settleMs: 200
    property var ops: []
    property int at: 0
    property var awaiting: null
    property var queued: []
    property double stamp: Date.now()
    property double landedAt: 0
    property int checks: 0
    property int failures: 0
    property var pane: null
    property var keys: null
    property var before: null
    property int gen: 0
    property var target: null
    property bool ticking: false
    // The frames drawn after Esc: the search still drawn, the folder drawn, and which folder frame first drew the cursor on its row.
    property bool watching: false
    property double escAt: 0
    property int heldFrames: 0
    property int folderFrames: 0
    property int landedFrame: 0
    property double firstFolderMs: -1
    property double landedMs: -1
    property real landedY: -1
    // The view as the landing's own turn left it, read where rowsLanded ends the listing; every queued call runs after it.
    property real turnY: -1
    property var trace: []

    function check(label, actual, expected) {
        root.checks += 1
        var ok = JSON.stringify(actual) === JSON.stringify(expected)
        if (!ok) root.failures += 1
        console.log("EXITDEEP " + (ok ? "PASS " : "FAIL ") + label + " got=" + JSON.stringify(actual) + " expected=" + JSON.stringify(expected))
    }
    // A ListView builds rows past its viewport with no done signal, so the quit waits for the pane's destruction, which cancels every queued build.
    property bool finished: false
    function finish() {
        if (root.finished) return
        root.finished = true
        root.stamp = Date.now()
        console.log("EXITDEEP DONE checks=" + root.checks + " failed=" + root.failures)
        if (root.pane) root.pane.destroy()
    }
    function viewOf() {
        return root.pane.viewMode === "columns" ? root.pane.listArea.activeColumn() : root.pane.listArea
    }
    function viewY() {
        return root.pane.viewMode === "columns" ? root.viewOf().viewport.contentY : root.pane.listArea.contentY
    }
    // The name of the row a view draws at a listing index, or "" when no delegate is built for it.
    function drawnAt(index) {
        var item = root.pane.visibleItemFor(index)
        return item && item.visible && item.row ? item.row.n : ""
    }
    function inView(index) {
        var range = root.viewOf().visibleRange()
        return index >= range.first && index <= range.last
    }
    function cursorName() {
        var row = root.pane.rowFor(root.pane.cursorIndex)
        return row ? row.n : ""
    }
    // The cursor on the named row, held, drawn and inside the viewport.
    function onRow(name) {
        return root.cursorName() === name && root.drawnAt(root.pane.cursorIndex) === name && root.inView(root.pane.cursorIndex)
    }
    function line() {
        return "view=" + root.pane.viewMode + " inFlight=" + root.pane.listInFlight + " state=" + root.pane.listingState
            + " mode=" + JSON.stringify(root.pane.searchMode) + " leaving=" + root.pane.searchLeaving
            + " total=" + root.pane.total + " held=" + root.pane.held + " rows=" + root.pane.rows.length
            + " cursor=" + root.pane.cursorIndex + " name=" + JSON.stringify(root.cursorName()) + " y=" + root.viewY()
    }
    function key(text) {
        if (text === "ESC") root.keys.keyClick(Qt.Key_Escape, Qt.NoModifier, -1)
        else root.keys.keyClickChar(text, Qt.NoModifier, -1)
    }
    function landed() {
        return !root.pane.listInFlight && root.pane.searchMode.length === 0 && root.pane.total === root.folderRows
    }
    function answered() { return root.pane.searchGen === root.gen && !root.pane.searchRunning && root.pane.searchSettledAt >= root.pane.searchKeyAt }

    // Each frame after Esc is read just before it is synchronised, so it is what that frame draws.
    function onFrame() {
        if (!root.watching || root.pane === null) return
        if (root.pane.searchMode.length > 0 || root.pane.searchLeaving === true) {
            root.heldFrames += 1
            return
        }
        root.folderFrames += 1
        if (root.folderFrames === 1) root.firstFolderMs = Date.now() - root.escAt
        var on = root.onRow(root.target.name)
        if (root.trace.length < 6)
            root.trace.push({ f: root.folderFrames, cursor: root.pane.cursorIndex, name: root.cursorName(), held: root.pane.held, y: root.viewY(), on: on })
        if (on && root.landedFrame === 0) {
            root.landedFrame = root.folderFrames
            root.landedMs = Date.now() - root.escAt
            root.landedY = root.viewY()
        }
    }

    Connections {
        target: root.pane
        function onListInFlightChanged() { if (root.watching && !root.pane.listInFlight && root.turnY < 0) root.turnY = root.viewY() }
    }

    // The exit's verdict once the folder has stood settleMs, against the swap's own counters and the frames drawn.
    function exited(label) {
        if (!root.landed() || root.folderFrames === 0) { root.landedAt = 0; return false }
        if (root.landedAt === 0) root.landedAt = Date.now()
        if (Date.now() - root.landedAt < root.settleMs) return false
        root.watching = false
        var after = root.pane.swap.describe()
        root.check(label + " exit held the walk until it landed", [after.holds - root.before.holds, after.last.end], [1, "landed"])
        root.check(label + " exit blankFrames", after.blankFrames, root.before.blankFrames)
        root.check(label + " exit fallbacks", after.fallbacks, root.before.fallbacks)
        // The walk landed inside the cap with no loading mark drawn, so no rows line waited out a minimum.
        root.check(label + " exit waits", [typeof after.waits, after.waits - root.before.waits], ["number", 0])
        root.check(label + " exit state", [root.pane.searchLeaving, root.pane.listInFlight, root.pane.listingState], [false, false, "ready"])
        root.check(label + " exit cursor file", root.cursorName(), root.target.name)
        root.check(label + " exit cursor index", root.pane.cursorIndex, root.target.index)
        root.check(label + " exit draws the cursor row in the viewport", root.onRow(root.target.name), true)
        // The counted check: the folder's first frame is the landing frame, so no frame draws row 0 or the top first.
        root.check(label + " folder frames up to the cursor on its row", root.landedFrame, 1)
        root.check(label + " the view stays where it landed", root.viewY(), root.landedY)
        // The frame check above sees a later move only when a frame falls between; this one sees it on every run.
        root.check(label + " the landing turn leaves the view where it stays", root.viewY(), root.turnY)
        console.log("EXITDEEP TIME view=" + root.pane.viewMode + " depth=" + root.target.depth + " firstFolderMs=" + root.firstFolderMs
            + " landedMs=" + root.landedMs + " heldFrames=" + root.heldFrames + " folderFrames=" + root.folderFrames)
        if (root.landedFrame !== 1)
            console.log("EXITDEEP TRACE " + label + " " + JSON.stringify(root.trace) + " " + root.line())
        return true
    }

    FloatingWindow {
        id: win
        implicitWidth: 1400
        implicitHeight: 800
        color: Flea.Theme.color.background
        Item {
            id: frameProbe
            Connections {
                target: frameProbe.Window.window
                function onAfterAnimating() { root.onFrame() }
            }
        }
    }
    Flea.Backend { id: realBackend }
    Component {
        id: paneComponent
        Flea.Pane {
            width: 1400
            height: 800
            backend: realBackend
        }
    }

    Component.onCompleted: {
        var list = [{ open: true }]
        var views = ["list", "columns", "grid"]
        for (var v = 0; v < views.length; v++) {
            // The switch itself starts on row 0, so each view's own landing is all a case measures.
            list.push({ cursor: root.top }, { view: views[v] })
            for (var r = 0; r < root.reps; r++) {
                for (var t = 0; t < root.targets.length; t++) {
                    list.push({ cursor: root.targets[t] }, { search: true }, { type: "nor" }, { exit: views[v] + " " + root.targets[t].depth })
                }
            }
        }
        root.ops = list
    }

    Timer {
        interval: root.tickMs
        running: true
        repeat: true
        onTriggered: {
            // A key event spins the event loop, so a tick can fire inside an op; a nested op would run twice.
            if (root.ticking) return
            root.ticking = true
            try { root.tick() } finally { root.ticking = false }
        }
    }

    function tick() {
        if (root.finished) {
            if (root.pane === null) Qt.exit(root.failures > 0 ? 1 : 0)
            else if (Date.now() - root.stamp > root.giveUpMs) { console.log("EXITDEEP FAIL the pane outlived its destroy, so the probe never quit"); Qt.exit(1) }
            return
        }
        if (Date.now() - root.stamp > root.giveUpMs) {
            root.check("probe completes", "timeout at op " + root.at + " " + JSON.stringify(root.ops[root.at - 1]) + " " + (root.pane ? root.line() : ""), "complete")
            root.finish()
            return
        }
        if (root.awaiting !== null && !root.awaiting()) return
        root.awaiting = null
        if (root.queued.length > 0) {
            root.key(root.queued.shift())
            root.gen += 1
            var sent = root.gen
            root.awaiting = function () { return root.pane.searchGen === sent && root.answered() }
            root.stamp = Date.now()
            return
        }
        if (root.at >= root.ops.length) {
            root.finish()
            return
        }
        root.stamp = Date.now()
        root.run(root.ops[root.at])
        root.at += 1
    }

    function run(op) {
        if (op.open) {
            Flea.ViewState.changeKey("view", "list")
            root.pane = paneComponent.createObject(win.contentItem)
            root.keys = Qt.createQmlObject("import QtTest; TestEvent {}", root.pane.listArea)
            root.pane.open(root.fixture)
            root.awaiting = function () { return root.landed() && root.drawnAt(0).length > 0 }
        } else if (op.view !== undefined) {
            root.keys.keyClick(op.view === "list" ? Qt.Key_1 : op.view === "columns" ? Qt.Key_2 : Qt.Key_3, Qt.ControlModifier, -1)
            root.awaiting = function () { return root.pane.viewMode === op.view && root.landed() && root.drawnAt(root.pane.cursorIndex).length > 0 }
        } else if (op.cursor !== undefined) {
            root.target = op.cursor
            root.pane.setCursor(op.cursor.index)
            root.awaiting = function () { return !root.pane.listInFlight && root.onRow(op.cursor.name) }
        } else if (op.search) {
            root.pane.listArea.forceActiveFocus()
            root.gen = root.pane.searchGen
            root.key("f")
            root.awaiting = function () { return root.pane.searchMode === "typing" }
        } else if (op.type !== undefined) {
            root.queued = op.type.split("")
        } else if (op.exit !== undefined) {
            root.check(op.exit + " search stands with results", root.pane.total > 0 && root.pane.total < root.folderRows, true)
            root.before = root.pane.swap.describe()
            root.heldFrames = 0
            root.folderFrames = 0
            root.landedFrame = 0
            root.firstFolderMs = -1
            root.landedMs = -1
            root.landedY = -1
            root.turnY = -1
            root.trace = []
            root.landedAt = 0
            root.escAt = Date.now()
            root.watching = true
            root.key("ESC")
            root.awaiting = function () { return root.exited(op.exit) }
        }
    }
}
