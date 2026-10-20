//@ pragma ShellId flea-quicklook-followrest-test

import QtQuick
import QtTest
import Quickshell
import "flea/js/PreviewKeys.js" as PreviewKeys

// quicklook-followrest: a held j in Quick Look loads only its first repeat, a repeat a starved loop serves late trails, and the release settles once.
ShellRoot {
    id: root

    readonly property string uiDir: Quickshell.env("QLFR_UI")
    readonly property string fixture: Quickshell.env("QLFR_DIR")
    readonly property int stepMs: 25
    // Presses per burst; a starveMs busy wait runs before each listed 1-based press, so the rest that press owes is not handled between them.
    readonly property var sweeps: [
        { steps: 12, starve: [2, 5, 9] },
        { steps: 12, starve: [2, 3, 7] }
    ]
    readonly property int starveMs: 200
    readonly property int pollMs: 100
    readonly property int watchdogMs: 30000
    readonly property int floor: 14

    property int stage: 0
    property int sweep: 0
    property int pressed: 0
    property int checks: 0
    property int failures: 0
    property bool sending: false
    property int changes: 0
    property int burstChanges: 0
    property int starvedLoads: 0
    property int afterMark: 0
    property bool released: false
    property int quiet: 0
    property var starveSet: ({})
    property var keys: null
    property double stageAt: Date.now()

    function log(line) { console.log("QLFR " + line) }
    function check(name, ok, detail) {
        root.checks += 1
        if (!ok) {
            root.failures += 1
            root.log("FAIL " + name + (detail === undefined || detail === "" ? "" : " " + detail))
        } else root.log("ok " + name)
    }
    function finish() {
        root.log("DONE sweeps=" + root.sweeps.length + " checks=" + root.checks + " failed=" + root.failures)
        Qt.exit(root.failures ? 1 : 0)
    }
    function cur() { return root.sweeps[root.sweep] }
    function pane() { return body.item ? body.item.currentPane : null }
    function pv() { return root.pane() && root.pane().preview ? root.pane().preview : null }
    function busy(ms) { var until = Date.now() + ms; while (Date.now() < until) {} }
    // No settle and no rest is still owed: a single-shot Timer stays running until its trigger, a posted event, is handled.
    function settleRunning() {
        var p = root.pv()
        if (!p) return false
        for (var i = 0; i < p.resources.length; i++) {
            var t = p.resources[i]
            if (t instanceof Timer && t.interval === p.followSettleMs && t.running) return true
        }
        return false
    }
    // The follow settle a repeat arms when it trails; a repeat that loads at once stops it.
    function followSettle() {
        var p = root.pv()
        if (!p) return null
        for (var i = 0; i < p.resources.length; i++) {
            var t = p.resources[i]
            if (t instanceof Timer && t.objectName === "followSettle") return t
        }
        return null
    }
    function rowName() {
        var pane = root.pane()
        var row = pane.rowFor(pane.cursorIndex)
        return row === null ? "" : row.n
    }
    function finalPath() { return root.fixture + "/" + root.rowName() }

    function judge() {
        var p = root.pv()
        var moved = root.pane().cursorIndex
        var after = root.changes - root.afterMark
        root.log("SWEEP n=" + root.sweep + " moved=" + moved + " burst=" + root.burstChanges + " after=" + after
            + " starved=" + root.starvedLoads + " path=" + p.path)
        root.check("sweep " + root.sweep + " moved " + root.cur().steps + " rows", moved === root.cur().steps, "cursor=" + moved)
        root.check("sweep " + root.sweep + " the held burst loaded only the key's own first repeat", root.burstChanges === 1,
            "burst loads=" + root.burstChanges)
        root.check("sweep " + root.sweep + " no starved repeat loaded a row", root.starvedLoads === 0, "starved loads=" + root.starvedLoads)
        root.check("sweep " + root.sweep + " the release settles its own row once", after === 1 && p.path === root.finalPath(),
            "after=" + after + " path=" + p.path)
        root.check("sweep " + root.sweep + " shows no row but the key's first repeat and the released one", root.burstChanges + after === 2,
            "loads=" + (root.burstChanges + after))
        root.sweep += 1
        if (root.sweep >= root.sweeps.length) {
            root.finish()
            return
        }
        PreviewKeys.act("escape", root.pane())
        root.stage = 5
        root.quiet = 0
        root.stageAt = Date.now()
    }

    function advance() {
        if (Date.now() - root.stageAt > root.watchdogMs) {
            root.fail("watchdog in stage " + root.stage)
            root.finish()
            return
        }
        if (root.stage === 0) {
            var pane = root.pane()
            if (!pane || pane.listInFlight || pane.total < root.floor || !pane.storageKnown) return
            root.keys = Qt.createQmlObject("import QtTest; TestEvent {}", pane.listArea)
            pane.listArea.forceActiveFocus()
            root.stage = 1
            root.stageAt = Date.now()
            return
        }
        if (root.stage === 1) {
            var pane = root.pane()
            if (pane.cursorIndex !== 0) { root.keys.keyClickChar("g", Qt.NoModifier, -1); return }
            if (pane.listInFlight || Date.now() - root.stageAt < 400) return
            PreviewKeys.open(pane)
            root.stage = 2
            root.stageAt = Date.now()
            return
        }
        if (root.stage === 2) {
            // Open, landed, and rested on the clock follow() reads, so the key's first repeat loads at once.
            var p = root.pv()
            if (!p || !p.active || !p.landed || Date.now() - p.lastMoveAt < root.starveMs) return
            root.starveSet = {}
            var list = root.cur().starve
            for (var i = 0; i < list.length; i++) root.starveSet[list[i]] = true
            root.pressed = 0
            root.released = false
            root.burstChanges = 0
            root.starvedLoads = 0
            root.stage = 3
            root.stageAt = Date.now()
            return
        }
        if (root.stage === 4) {
            var p = root.pv()
            if (!p || p.path !== root.finalPath() || root.settleRunning()
                    || (p.swap !== null && (p.swap.holding || p.swap.capturing))) { root.quiet = 0; return }
            root.quiet += 1
            if (root.quiet < 3) return
            root.quiet = 0
            root.judge()
            return
        }
        if (root.stage === 5 && root.pv() !== null && !root.pv().active) {
            root.stage = 1
            root.stageAt = Date.now()
        }
    }

    function fail(why) {
        root.failures += 1
        root.log("FAIL " + why)
    }

    Connections {
        target: root.stage >= 2 && root.pv() ? root.pv() : null
        function onPathChanged() {
            root.changes += 1
            if (root.stage === 3 && !root.released) root.burstChanges += 1
        }
    }

    Timer {
        interval: root.pollMs
        repeat: true
        running: true
        onTriggered: root.advance()
    }

    Timer {
        interval: root.stepMs
        running: root.stage === 3
        repeat: true
        onTriggered: {
            if (root.sending) return
            root.sending = true
            var n = root.pressed + 1
            var starved = root.starveSet[n] === true
            if (starved) root.busy(root.starveMs)
            root.keys.keyPressChar("j", Qt.NoModifier, -1)
            // A starved repeat that trailed leaves its settle running; one that loaded at once stopped it.
            var settle = root.followSettle()
            if (starved && (settle === null || !settle.running)) {
                root.starvedLoads += 1
                root.log("FAIL starved repeat " + n + " loaded at once, its settle stopped")
            }
            root.pressed = n
            if (root.pressed >= root.cur().steps) {
                root.keys.keyReleaseChar("j", Qt.NoModifier, -1)
                root.released = true
                root.afterMark = root.changes
                root.stage = 4
                root.quiet = 0
                root.stageAt = Date.now()
            }
            root.sending = false
        }
    }

    FloatingWindow {
        id: window
        implicitWidth: 1000
        implicitHeight: 700
        Loader {
            id: body
            anchors.fill: parent
            source: "file://" + root.uiDir + "/WindowBody.qml"
            onLoaded: item.host = window
            onStatusChanged: if (status === Loader.Error) { root.log("FAIL WindowBody failed to load"); root.failures += 1; root.finish() }
        }
    }
}
