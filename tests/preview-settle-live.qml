//@ pragma ShellId flea-preview-settle-test

import QtQuick
import Quickshell
import "flea" as Flea

// Production previews use the real 120ms timer; count backend asks and loads in automatic and seeded manual modes.
ShellRoot {
    id: root

    property var failures: []
    property int stage: 0
    property int metaMark: 0
    // A backdated duplicate tells same apart from a re-arm without reading the clock.
    property int duplicateBackdateMs: 60
    property int pollWant: -2
    property int pollNext: -1
    property int qlLoads: 0
    property var flow: []
    property bool manual: Quickshell.env("FLEA_PREVIEWSETTLE_MANUAL") === "1"
    // e80q: the held Quick Look move's pre-landing path and mid-frame count.
    property string qlOld: ""
    property int qlMid0: 0
    // A burst inside Quick Look's open hold: its settle and cap Timers, and what happened while the settle ran.
    property var holdSettle: null
    property var holdCap: null
    property bool holdWatch: false
    property bool holdReadySettling: false
    property int holdReadyDrops: 0
    property bool holdLandSettling: false
    property double holdStopAt: 0
    property double holdLandAt: 0
    property int holdCaps: 0
    // Past the runner's 60 s timeout, so neither the cap nor the held settle fires inside the probe however slow the decode.
    readonly property int holdCapMs: 60000
    property string holdPhase: ""
    property bool holdTimedOut: false
    // Every wait ends on the state it polls or at this one deadline, inside the runner's 60 s timeout.
    readonly property double deadlineAt: Date.now() + 45000
    // When the open burst's moves began, or 0 outside one; a burst opens after its idle move's check, which no stall can fail.
    property double burstAt: 0

    function holdDetail() {
        return "phase=" + root.holdPhase + " ready=" + root.holdReadySettling
            + " readyDrops=" + root.holdReadyDrops + " lookReady=" + quick.lookReady + " sizeWait=" + quick.sizeWait + " status=" + quick.status
            + " settling=" + (root.holdSettle !== null && root.holdSettle.running)
            + " capRunning=" + (root.holdCap !== null && root.holdCap.running)
            + " landSettling=" + root.holdLandSettling + " stopAt=" + root.holdStopAt + " landAt=" + root.holdLandAt
            + " caps=" + (quick.revealCaps - root.holdCaps) + " early=" + quick.revealedEarly
            + " timeout=" + root.holdTimedOut
    }

    function buildRows() {
        var rows = []
        for (var i = 0; i < 8; i++)
            rows.push({ n: "img" + i + ".jpg", d: false, i: "image-x-generic", p: 420,
                s: 1000 + i, m: 1758835200 + i, t: true, k: 0 })
        return rows
    }

    function imagePath(index) { return pane.join(pane.path, "img" + index + ".jpg") }

    // A burst trails only while its moves fall inside this window.
    function settleWindow() { return Math.min(pane.settleMs, quick.followSettleMs) }

    // Past its burst's window a check may be the stall's doing, so it is dropped and stalled() ends the run; a kept check reads nothing a stall moves.
    function check(name, cond, detail, kept) {
        if (!kept && root.burstAt > 0 && Date.now() - root.burstAt >= root.settleWindow())
            return
        if (!cond)
            failures.push(name)
        console.log("PREVIEWSETTLE " + (cond ? "PASS" : "FAIL") + " " + name
            + " meta=" + backend.metaCalls + " loaded=" + preview.loadedIndex
            + " canRead=" + preview.canRead + " visible=" + preview.visible
            + " automatic=" + Flea.ViewState.previewAutomatic + " storage=" + pane.storageClass
            + " known=" + pane.storageKnown + (detail ? " " + detail : ""))
    }

    function step(index) {
        pane.cursorIndex = index
        preview.followSelection()
    }

    function quickTimer(test) {
        for (var i = 0; i < quick.resources.length; i++) {
            var t = quick.resources[i]
            if (t instanceof Timer && test(t))
                return t
        }
        return null
    }

    function followSettle() { return root.quickTimer(function (t) { return t.objectName === "followSettle" }) }
    function followTimersRunning() {
        var n = 0
        for (var i = 0; i < quick.resources.length; i++) {
            var t = quick.resources[i]
            if (t instanceof Timer && t.interval === quick.followSettleMs && t.running) n += 1
        }
        return n
    }

    function later(ms) {
        waiter.interval = ms
        waiter.restart()
    }

    function go(next) {
        root.stage = next
        root.flow[next]()
    }

    function pollFor(want, next) {
        root.pollWant = want
        root.pollNext = next
        poller.start()
    }

    // A burst stalled past the window ends the run: red when a check taken before the stall failed, else void, and the runner repeats it.
    function stalled(name) {
        var span = Date.now() - root.burstAt
        root.burstAt = 0
        if (span < root.settleWindow())
            return false
        var earlier = root.failures.length
        console.log("PREVIEWSETTLE " + (earlier > 0 ? "STALL" : "VOID") + " " + name + " span=" + span
            + " earlier=" + earlier + " attempt=" + Quickshell.env("FLEA_PREVIEWSETTLE_ATTEMPT"))
        if (earlier > 0)
            root.done()
        else
            Quickshell.execDetached(["kill", String(Quickshell.processId)])
        return true
    }

    function done() {
        var line = "PREVIEWSETTLE DONE failures=" + root.failures.length
        for (var i = 0; i < root.failures.length; i++)
            line += " [" + root.failures[i] + "]"
        console.log(line)
        Quickshell.execDetached(["kill", String(Quickshell.processId)])
    }

    QtObject {
        id: backend
        property int metaCalls: 0
        property int thumbCalls: 0
        property int token: 0
        signal metaResult(var message)
        signal meta(int row, int w, int h, int orient, real durationMs, int sampleRate,
            int entries, real unpacked, bool archiveFailed, var names, real lines,
            bool partial, bool linesFailed, string target, bool targetDir, string owner)
        function askMeta(index, wantText, wantMedia, wantArchive) {
            metaCalls += 1
            token += 1
            return token
        }
        function thumb(ask, cacheOnly) { thumbCalls += 1 }
        function thumbcancel(rows) {}
    }

    QtObject {
        id: swap
        property bool fellBack: false
        property bool capturing: false
        property var order: []
        property var queue: []
        property int starts: 0
        function hold(apply, key, atWork) {
            order.push(atWork === true ? "load" : "clear")
            if (capturing) {
                if (apply)
                    queue.push(apply)
            } else if (apply) {
                apply()
            }
        }
        function start(isPdf) { starts += 1 }
        function release() {
            var pending = queue
            queue = []
            for (var i = 0; i < pending.length; i++)
                pending[i]()
        }
        function reset() {
            order = []
            queue = []
            starts = 0
        }
    }

    QtObject {
        id: pane
        property string path: Quickshell.env("FLEA_PREVIEWSETTLE_DIR")
        property int cursorIndex: -1
        property int selectionVersion: 0
        property var rows: root.buildRows()
        property var kindNames: ({ 0: "" })
        property int settleMs: 120
        property bool storageKnown: true
        property string storageClass: "local"
        property bool listInFlight: false
        property var thumbState: ({ file: {}, order: [] })
        property var backend: backend
        property var listArea: ({ forceActiveFocus: function () {} })
        function rowFor(index) { return (index >= 0 && index < rows.length) ? rows[index] : null }
        function join(path, name) { return path + "/" + name }
        function selectionCount() { return 1 }
        function selectedIndices() { return [cursorIndex] }
    }

    // A real window gives SelectionPreview the effective visibility it has in the product.
    Window {
        visible: true
        width: 800
        height: 600

        Flea.SelectionPreview {
            id: preview
            width: 800
            height: 600
            pane: pane
            swap: swap
        }
    }

    // Quick Look in its own window, so its swap is visible and takes a picture offscreen.
    Window {
        visible: true
        width: 800
        height: 600

        Flea.Preview {
            id: quick
            width: 800
            height: 600
            pane: pane
            onPathChanged: root.qlLoads += 1
        }
    }

    Timer {
        id: waiter
        repeat: false
        onTriggered: root.go(root.stage + 1)
    }

    Timer {
        id: poller
        interval: 10
        repeat: true
        onTriggered: {
            if (preview.loadedIndex === root.pollWant) {
                poller.stop()
                root.go(root.pollNext)
            } else if (Date.now() > root.deadlineAt) {
                poller.stop()
                root.failures.push("timeout waiting for loaded " + root.pollWant)
                root.done()
            }
        }
    }

    // landed is per open: it gates only the stage after the img1 reopen, whose move needs that open landed.
    Timer {
        id: qpoll
        interval: 10
        repeat: true
        property string want: ""
        onTriggered: {
            if (quick.path === qpoll.want && quick.landed) {
                qpoll.stop()
                root.go(root.stage + 1)
            } else if (Date.now() > root.deadlineAt) {
                qpoll.stop()
                root.failures.push("timeout waiting for quick path " + qpoll.want)
                root.done()
            }
        }
    }

    Connections {
        target: root.holdWatch ? quick : null
        function onLookReadyChanged() {
            if (root.holdPhase === "settle" && !quick.lookReady)
                root.holdReadyDrops += 1
            if (quick.lookReady && root.holdSettle.running)
                root.holdReadySettling = true
        }
        function onLandedChanged() {
            if (!quick.landed)
                return
            root.holdLandAt = Date.now()
            if (root.holdSettle.running)
                root.holdLandSettling = true
        }
    }

    Connections {
        target: root.holdWatch ? root.holdSettle : null
        function onRunningChanged() {
            if (root.holdPhase === "settle" && !root.holdSettle.running) {
                root.holdStopAt = Date.now()
                console.log("PREVIEWSETTLE HOLD stopped " + root.holdDetail())
            }
        }
    }

    Timer {
        id: holdPoll
        interval: 10
        repeat: true
        onTriggered: {
            if (Date.now() > root.deadlineAt) {
                root.holdTimedOut = true
                console.log("PREVIEWSETTLE HOLD watchdog " + root.holdDetail())
            } else if (root.holdPhase === "setup") {
                if (!quick.lookReady || quick.path !== root.imagePath(3) || !root.holdSettle.running)
                    return
                root.holdReadySettling = true
                // Hold only decode setup; restore and run the production 120ms settle before observing its stop.
                root.holdSettle.interval = quick.followSettleMs
                root.holdSettle.restart()
                root.holdPhase = "settle"
                console.log("PREVIEWSETTLE HOLD ready " + root.holdDetail())
                return
            } else if (!quick.landed) {
                return
            }
            holdPoll.stop()
            root.go(root.stage + 1)
        }
    }

    function qpollFor(want) {
        qpoll.want = want
        qpoll.start()
    }

    property var autoStages: [
        function () { root.later(200) },
        function () {
            root.check("startup sends no backend work", backend.metaCalls === 0 && backend.thumbCalls === 0)
            root.step(1)
            root.check("idle first move loads synchronously", preview.loadedIndex === 1 && backend.metaCalls === 1)
            root.later(200)
        },
        function () {
            var since = Date.now()
            root.step(2)
            root.check("idle second move loads at once", preview.loadedIndex === 2 && backend.metaCalls === 2)
            root.burstAt = since
            root.step(3)
            root.check("burst holds first repeat", backend.metaCalls === 2)
            root.step(4)
            root.check("burst holds second repeat", backend.metaCalls === 2)
            root.step(5)
            root.check("burst holds third repeat", backend.metaCalls === 2)
            if (root.stalled("selection burst")) return
            root.later(300)
        },
        function () {
            root.check("burst lands one trailing load", backend.metaCalls === 3 && preview.loadedIndex === 5)
            root.check("trailing load names the final row", preview.loadedDirectory === pane.path
                && preview.loadedIdentity === JSON.stringify(["img5.jpg", 1005, 1758835205, 420, "image-x-generic"]))
            root.later(200)
        },
        function () {
            var since = Date.now()
            root.step(6)
            root.check("post-burst idle move loads at once", preview.loadedIndex === 6 && backend.metaCalls === 4)
            root.burstAt = since
            root.metaMark = backend.metaCalls
            root.step(7)
            root.check("rapid move trails", backend.metaCalls === root.metaMark)
            var heldAt = preview.lastMoveAt - root.duplicateBackdateMs
            preview.lastMoveAt = heldAt
            // The settle and the rest of a load made at once are the Timers under preview; its root type declares none.
            var previewTimers = []
            for (var i = 0; i < preview.resources.length; i++) {
                if (preview.resources[i] instanceof Timer)
                    previewTimers.push(preview.resources[i])
            }
            var timers = previewTimers.length
            // restart() on a running Timer is stop then start, so it emits runningChanged twice and a second arm of either reads settleRestarts 2.
            var settleRestarts = 0
            var counted = function () {
                settleRestarts += 1
            }
            previewTimers.forEach(function (timer) { timer.runningChanged.connect(counted) })
            root.step(7)
            previewTimers.forEach(function (timer) { timer.runningChanged.disconnect(counted) })
            root.check("duplicate loads nothing", backend.metaCalls === root.metaMark)
            // The duplicate returns before settleFor whether the move trailed or loaded, so no stall moves these.
            root.check("duplicate moves no second arm", preview.lastMoveKey === pane.path + "\n7" && preview.lastMoveAt === heldAt
                && settleRestarts === 0 && timers === 2, "timers=" + timers, true)
            if (root.stalled("selection rapid move")) return
            root.pollFor(7, root.stage + 1)
        },
        function () {
            root.check("duplicate lands one trailing load", backend.metaCalls === root.metaMark + 1)
            pane.storageClass = "network"
            root.later(150)
        },
        function () {
            root.metaMark = backend.metaCalls
            root.step(0)
            root.check("off class holds with no decode", backend.metaCalls === root.metaMark
                && preview.manualHold === true && preview.loadedIndex === 0)
            pane.storageClass = "local"
            pane.storageKnown = false
            root.later(150)
        },
        function () {
            root.metaMark = backend.metaCalls
            swap.reset()
            root.step(1)
            root.check("unknown class waits on the swap cap", backend.metaCalls === root.metaMark && swap.starts === 1)
            pane.storageKnown = true
            root.later(250)
        },
        function () {
            root.check("class landing loads the cursor row", preview.loadedIndex === 1)
            root.metaMark = backend.metaCalls
            preview.visible = false
            root.step(2)
            preview.followSelection()
            root.check("hidden preview does no work", backend.metaCalls === root.metaMark)
            preview.visible = true
            root.check("unhidden preview loads at once", preview.loadedIndex === 2
                && backend.metaCalls === root.metaMark + 1)
            root.later(150)
        },
        function () {
            swap.reset()
            swap.capturing = true
            root.later(200)
        },
        function () {
            root.metaMark = backend.metaCalls
            root.step(3)
            root.check("picture hold runs before the load", swap.order.join(",") === "clear,load")
            root.check("capturing queues both callbacks", swap.queue.length === 2
                && backend.metaCalls === root.metaMark && preview.loadedIndex === 2)
            swap.release()
            root.check("released queue lands the move", preview.loadedIndex === 3
                && backend.metaCalls === root.metaMark + 1)
            swap.capturing = false
            root.later(150)
        },
        function () {
            var touched = pane.rows.slice()
            touched[3] = Object.assign({}, touched[3], { s: touched[3].s + 1, m: touched[3].m + 1 })
            root.metaMark = backend.metaCalls
            pane.rows = touched
            root.check("same-index identity refresh reloads", backend.metaCalls === root.metaMark + 1
                && preview.loadedIndex === 3 && preview.loadedIdentity === preview.identity(pane.rowFor(3)))
            root.later(150)
        },
        function () {
            root.metaMark = backend.metaCalls
            preview.visible = false
            preview.visible = true
            root.check("same-target hide and show restores", preview.loadedIndex === 3
                && backend.metaCalls === root.metaMark + 1 && preview.pending === false)
            root.later(150)
        },
        function () {
            root.metaMark = backend.metaCalls
            preview.followSelection()
            root.check("settled duplicate keeps showing", backend.metaCalls === root.metaMark
                && preview.loadedIndex === 3 && preview.pending === false)
            root.later(150)
        },
        function () {
            preview.swap = null
            root.later(200)
        },
        function () {
            root.metaMark = backend.metaCalls
            var since = Date.now()
            root.step(4)
            root.check("no-swap idle move loads at once", preview.loadedIndex === 4
                && backend.metaCalls === root.metaMark + 1)
            root.burstAt = since
            root.step(5)
            root.check("no-swap rapid move trails", backend.metaCalls === root.metaMark + 1)
            if (root.stalled("no-swap rapid move")) return
            root.later(300)
        },
        function () {
            root.check("no-swap trailing load lands", preview.loadedIndex === 5
                && backend.metaCalls === root.metaMark + 2)
            root.later(200)
        },
        function () {
            var touched = pane.rows.slice()
            touched[5] = Object.assign({}, touched[5], { s: touched[5].s + 1, m: touched[5].m + 1 })
            root.metaMark = backend.metaCalls
            pane.rows = touched
            root.check("no-swap same-target refresh works", backend.metaCalls === root.metaMark + 1
                && preview.loadedIndex === 5 && preview.loadedIdentity === preview.identity(pane.rowFor(5)))
            root.later(200)
        },
        function () {
            var followTimers = 0
            for (var i = 0; i < quick.resources.length; i++) {
                var t = quick.resources[i]
                if (t instanceof Timer && t.interval === quick.followSettleMs && t.repeat === false) followTimers += 1
            }
            root.check("Quick Look keeps the follow settle and its rest as the two follow-interval timers", followTimers === 2, "timers=" + followTimers)
            quick.follow(root.imagePath(6), "image-x-generic", 1006, "")
            root.check("quick follow stages its pending row", quick.pendingPath === root.imagePath(6))
            root.qpollFor(root.imagePath(6))
        },
        function () {
            root.check("quick idle follow opens the row", quick.active && quick.path === root.imagePath(6))
            // No burst: Quick Look is revealed here, so its capture holds even a "now" load and no stall moves this check.
            quick.lastMoveAt = Date.now()
            var loads = root.qlLoads
            quick.follow(root.imagePath(7), "image-x-generic", 1007, "")
            root.check("quick fresh move defers", quick.pendingPath === root.imagePath(7)
                && quick.path === root.imagePath(6) && root.qlLoads === loads)
            root.qpollFor(root.imagePath(7))
        },
        function () {
            root.check("quick trailing follow lands once", quick.path === root.imagePath(7) && root.qlLoads === 2)
            root.burstAt = Date.now()
            quick.lastMoveAt = root.burstAt
            var loads = root.qlLoads
            quick.follow(root.imagePath(0), "image-x-generic", 1000, "")
            quick.follow(root.imagePath(0), "image-x-generic", 1000, "")
            // follow() stages the row before it decides, so no stall moves the staged path.
            root.check("quick duplicate stages once", quick.pendingPath === root.imagePath(0), "", true)
            root.check("quick duplicate loads nothing", root.qlLoads === loads)
            if (root.stalled("quick duplicate")) return
            root.qpollFor(root.imagePath(0))
        },
        function () {
            root.check("quick duplicate lands once", quick.path === root.imagePath(0) && root.qlLoads === 3)
            quick.close()
            root.check("close resets the move key", quick.lastMoveKey === "" && !quick.active)
            quick.follow(root.imagePath(1), "image-x-generic", 1001, "")
            root.qpollFor(root.imagePath(1))
        },
        function () {
            root.check("quick reopens the fresh row", quick.active && quick.path === root.imagePath(1)
                && quick.pendingPath === root.imagePath(1) && root.qlLoads === 4)
            // The reopen's load arms its rest once its own work returns; the held move waits for that rest to run out.
            root.later(quick.followSettleMs + 30)
        },
        function () {
            root.check("quick reopen rest has run out before the held move", root.followTimersRunning() === 0,
                "running=" + root.followTimersRunning())
            root.qlOld = quick.path
            root.qlMid0 = quick.swapState().midFrames
            quick.lastMoveAt = 0
            var target = root.imagePath(2)
            quick.follow(target, "image-x-generic", 1002, "")
            var settle = root.followSettle()
            root.check("quick held move loads at once", settle !== null && settle.running === false,
                "settleRunning=" + (settle === null ? "missing" : settle.running))
            root.check("quick held move captures before it starts",
                quick.swap && quick.swap.capturing === true && quick.swap.started === false,
                "capturing=" + (quick.swap && quick.swap.capturing) + " started=" + (quick.swap && quick.swap.started))
            root.check("quick held move keeps the old path until the capture lands",
                quick.path === root.qlOld, "path=" + quick.path)
            root.qpollFor(target)
        },
        function () {
            root.check("quick held move lands whole", quick.path === root.imagePath(2)
                && quick.swapState().midFrames === root.qlMid0,
                "mid=" + quick.swapState().midFrames + " mid0=" + root.qlMid0)
            quick.close()
            root.later(150)
        },
        function () {
            // No captured listing row: a same-file reload must not toggle the independent image-metadata readiness gate.
            pane.cursorIndex = -1
            quick.lastMoveAt = 0
            var since = Date.now()
            quick.follow(root.imagePath(3), "image-x-generic", 1003, "")
            root.holdSettle = root.quickTimer(function (t) { return t.objectName === "followSettle" })
            root.holdCap = root.quickTimer(function (t) { return t.running && t.interval !== quick.followSettleMs && t.interval !== quick.stripHideMs })
            root.holdCaps = quick.revealCaps
            if (root.holdCap)
                root.holdCap.interval = root.holdCapMs
            root.holdWatch = root.holdSettle !== null
            root.holdPhase = "setup"
            if (root.holdWatch)
                root.holdSettle.interval = root.holdCapMs
            root.check("quick hold opens its settle and cap", root.holdWatch && root.holdCap !== null,
                "settle=" + (root.holdSettle !== null) + " cap=" + (root.holdCap !== null))
            root.burstAt = since
            quick.follow(root.imagePath(4), "image-x-generic", 1004, "")
            quick.follow(root.imagePath(3), "image-x-generic", 1003, "")
            if (root.holdWatch && quick.lookReady && root.holdSettle.running)
                root.holdReadySettling = true
            root.check("quick burst inside the hold trails", root.holdWatch && root.holdSettle.running
                && quick.unrevealed && quick.path === root.imagePath(3), "unrevealed=" + quick.unrevealed)
            if (root.stalled("quick hold burst")) return
            holdPoll.start()
        },
        function () {
            root.holdWatch = false
            var detail = root.holdDetail()
            root.check("quick hold burst lands nothing while settling", root.holdReadySettling
                && root.holdReadyDrops === 0 && !root.holdLandSettling, detail)
            root.check("quick hold burst lands once the settle stops", !root.holdTimedOut
                && root.holdStopAt > 0 && root.holdLandAt >= root.holdStopAt && quick.landed
                && quick.path === root.imagePath(3) && root.holdSettle.interval === quick.followSettleMs
                && quick.revealCaps === root.holdCaps && !quick.revealedEarly && root.holdCap.running, detail)
            root.done()
        }
    ]

    property var manualStages: [
        function () { root.later(200) },
        function () {
            root.check("manual startup sends no backend work", backend.metaCalls === 0)
            root.step(1)
            root.step(2)
            root.check("manual moves schedule nothing", backend.metaCalls === 0)
            preview.loadSelection()
            root.check("explicit load still works", backend.metaCalls === 1 && preview.loadedIndex === 2)
            root.done()
        }
    ]

    Component.onCompleted: {
        if (pane.path.length === 0 || pane.path.charAt(0) !== "/") {
            root.failures.push("runner image directory missing")
            root.done()
            return
        }
        root.flow = root.manual ? root.manualStages : root.autoStages
        root.go(0)
    }
}
