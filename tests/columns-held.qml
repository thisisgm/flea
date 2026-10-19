//@ pragma ShellId flea-columns-held-test
import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import "flea" as Flea

// Columns-held: a held j through one real Pane in the list and then in columns, counts only; sweep 1 of each view replaces the rows mid-burst.
ShellRoot {
    id: root
    readonly property string fixture: Quickshell.env("FLEA_COLHELD_DIR") || ""
    readonly property string reqLog: Quickshell.env("FLEA_COLHELD_REQLOG") || ""
    readonly property int files: Number(Quickshell.env("FLEA_COLHELD_FILES") || "0")
    readonly property int steps: 70
    readonly property int stepMs: 25
    readonly property int sweeps: 3
    readonly property int replaceAt: 35
    readonly property var stages: ["list", "columns"]
    // The floors a held key cannot go under: the idle first press holds and loads at once, the second step ends that hold, and the settle loads once.
    readonly property int maxHolds: 3
    readonly property int maxLoads: 2
    // The extra frames a columns sweep may draw over the list after the release; the burst itself is bounded per move by maxBurstPerMove.
    readonly property int maxExtraFrames: 20
    // Burst frames per delivered move, columns over the list: 0.5 holds the loaded worst case with room; one frame a move reads 1.0.
    readonly property real maxBurstPerMove: 0.5
    property int checks: 0
    property int failures: 0
    property int phase: 0
    property int stage: 0
    property int sweep: 0
    property int pressed: 0
    property int rest: 0
    property var pane: null
    property var before: null
    property var results: ({})
    property int frames: 0
    property int rebinds: 0
    property int rowsLanded: 0
    property int loads: 0
    property int markFrames: 0
    property int markBurst: 0
    property int releaseFrames: 0
    property var mark: null
    property var hooked: []
    // Holds and loads at the press the replacement goes out on in sweep 1, taken at that press in every sweep: whether a sweep's second press
    // captures again turns on whether the first preview landed in between, so only the span from this press on tells what the replacement took.
    property var atReplace: ({ holds: 0, loads: 0 })
    property string trace: ""
    // QtTest runs the event loop inside every key it sends, so on a slow turn the next tick falls due inside the last one's press;
    // that tick waits for the next, or it would send a press the count has not taken yet.
    property bool sending: false
    readonly property string view: stages[stage]

    function check(name, ok, detail) {
        checks += 1
        if (!ok) {
            failures += 1
            console.log("COLHELD FAIL " + name + (detail === undefined ? "" : " " + detail))
        } else console.log("COLHELD ok " + name)
    }
    function finish() {
        console.log("COLHELD DONE " + checks + " checks, " + failures + " failed")
        Qt.quit()
    }

    FileView { id: reqFile; path: root.reqLog; blockLoading: true }
    // Backend request lines by kind, read from the tee the shim backend keeps; waitForJob makes the reload current.
    function requests() {
        reqFile.reload()
        reqFile.waitForJob()
        var counts = {}
        var re = /"c":"([a-zA-Z]+)"/g, m, text = reqFile.text()
        while ((m = re.exec(text)) !== null) counts[m[1]] = (counts[m[1]] || 0) + 1
        return counts
    }
    function delta(a, b) {
        var out = {}
        for (var k in b) if (b[k] - (a[k] || 0) !== 0) out[k] = b[k] - (a[k] || 0)
        return out
    }
    function fmt(o) {
        return Object.keys(o).sort().map(function (k) { return k + "=" + o[k] }).join(" ")
    }
    function sum(o, kinds) {
        var n = 0
        for (var i = 0; i < kinds.length; i++) n += o[kinds[i]] || 0
        return n
    }
    function swapNow() {
        var a = pane.listArea
        return view === "columns" && typeof a.swapState === "function" ? a.swapState() : null
    }
    function preview() { return view === "columns" ? pane.listArea.previewColumn : null }
    function viewport() { return view === "columns" ? pane.listArea.activeColumn().viewport : pane.listArea }
    function built() { return (view === "columns" ? pane.listArea.activeColumn() : pane.listArea).prebuild.createdCount }
    // The preview column's own loading mark, found by its shape: it is the one item with an armed and a heldOff.
    function findMark(item) {
        if (item.armed !== undefined && item.heldOff !== undefined) return item
        for (var i = 0; i < item.children.length; i++) {
            var found = findMark(item.children[i])
            if (found !== null) return found
        }
        return null
    }

    // Every delegate of the shown listing, counted when its row binding is written again; one that arrives mid-sweep is hooked as it is parented.
    function hookDelegate(item) {
        if (item.markRow === undefined || item.rowChanged === undefined || hooked.indexOf(item) >= 0) return
        hooked.push(item)
        item.rowChanged.connect(function () { root.rebinds += 1 })
    }
    function hookView() {
        var kids = viewport().contentItem.children
        for (var i = 0; i < kids.length; i++) hookDelegate(kids[i])
    }
    function snapshot() {
        var s = swapNow()
        return { requests: requests(), frames: frames, cursor: pane.cursorIndex, built: built(), rebinds: rebinds, rowsLanded: rowsLanded, loads: loads,
                 held: pane.held, rows: pane.rows.length, markFrames: markFrames, markBurst: markBurst,
                 holds: s === null ? 0 : s.holds, bursts: s === null ? 0 : s.bursts, mid: s === null ? 0 : s.midFrames,
                 heldFrames: s === null ? 0 : s.heldFrames }
    }

    FloatingWindow {
        id: win
        implicitWidth: 1280
        implicitHeight: 800
        Flea.Backend { id: probeBackend }
        Component {
            id: paneComponent
            Flea.Pane {
                width: 1280
                height: 800
                backend: probeBackend
            }
        }
        TestEvent { id: keys }
        Connections {
            target: root.phase >= 3 ? root.viewport().contentItem : null
            function onChildrenChanged() { root.hookView() }
        }
        Connections {
            target: root.phase >= 3 && root.view === "columns" ? root.preview() : null
            function onLoadedIndexChanged() { if (root.preview().loadedIndex >= 0) root.loads += 1 }
        }
        Connections {
            target: win.contentItem.Window.window
            function onFrameSwapped() {
                root.frames += 1
                if (root.mark !== null && root.mark.shown) {
                    root.markFrames += 1
                    if (root.phase === 5) root.markBurst += 1
                }
            }
        }
    }
    Connections {
        target: root.pane
        function onRowsChanged() { root.rowsLanded += 1 }
    }

    function report() {
        var now = snapshot()
        var req = delta(before.requests, now.requests)
        var out = { start: before.cursor, moved: now.cursor - before.cursor, requests: sum(req, ["window", "peek", "meta", "thumb", "thumbcancel", "dirsize"]),
                    meta: req.meta || 0, thumb: req.thumb || 0, window: req.window || 0, peek: req.peek || 0,
                    holds: now.holds - before.holds, bursts: now.bursts - before.bursts, loads: now.loads - before.loads,
                    rebinds: now.rebinds - before.rebinds, rowsLanded: now.rowsLanded - before.rowsLanded, delegates: hooked.length,
                    mid: now.mid - before.mid, heldFrames: now.heldFrames - before.heldFrames,
                    markFrames: now.markFrames - before.markFrames, markBurst: now.markBurst - before.markBurst,
                    frames: now.frames - before.frames, restFrames: now.frames - releaseFrames, burst: releaseFrames - before.frames, built: now.built - before.built,
                    lateHolds: now.holds - atReplace.holds, lateLoads: now.loads - atReplace.loads }
        results[view + sweep] = out
        console.log("COLHELD SWEEP view=" + view + " n=" + sweep + " " + fmt(out) + " held=" + before.held + "+" + before.rows + ">" + now.held + "+" + now.rows)
        console.log("COLHELD   requests " + fmt(req) + " storage=" + pane.storageClass)
        console.log("COLHELD   trace " + trace)
    }

    function judge() {
        for (var n = 0; n < sweeps; n++) {
            var list = results["list" + n], cols = results["columns" + n]
            check("sweep " + n + " moved " + root.steps + " rows in both views", list.moved === root.steps && cols.moved === root.steps, list.moved + "/" + cols.moved)
            check("sweep " + n + " columns reaches the preview", cols.loads >= 1 && cols.holds >= 1, cols.loads + " loads " + cols.holds + " holds")
            check("sweep " + n + " columns asks the backend no more than the list plus its loads' own asks", cols.requests <= list.requests + cols.loads,
                  cols.requests + " > " + list.requests + " + " + cols.loads)
            check("sweep " + n + " columns loads at most " + root.maxLoads + " previews", cols.loads <= root.maxLoads, cols.loads)
            check("sweep " + n + " columns takes at most " + root.maxHolds + " captures and ends one hold by burst", cols.holds <= root.maxHolds && cols.bursts <= 1,
                  cols.holds + " holds " + cols.bursts + " bursts")
            check("sweep " + n + " columns draws no half-built frame", cols.mid === 0, cols.mid)
            check("sweep " + n + " columns shows no loading mark while the key is held", cols.markBurst === 0, cols.markBurst)
            check("sweep " + n + " columns draws no more than " + root.maxExtraFrames + " frames over the list after the release", cols.restFrames <= list.restFrames + root.maxExtraFrames,
                  cols.restFrames + " vs " + list.restFrames)
            check("sweep " + n + " columns draws no more than " + root.maxBurstPerMove + " burst frames per move over the list",
                  cols.burst / cols.moved <= list.burst / list.moved + root.maxBurstPerMove,
                  (cols.burst / cols.moved).toFixed(2) + " vs " + (list.burst / list.moved).toFixed(2))
            if (n !== 1) check("sweep " + n + " columns rebinds no more rows than the list", cols.rebinds <= list.rebinds, cols.rebinds + " > " + list.rebinds)
        }
        // The replaced rows rebind each live delegate once, on top of what the same sweep costs without the replacement.
        check("rows replacement landed in both views", results.list1.rowsLanded === 1 && results.columns1.rowsLanded === 1,
              results.list1.rowsLanded + "/" + results.columns1.rowsLanded)
        // Sweep 0 runs the same presses with no replacement, so from the replacement's press to the sweep's end the two must take the same holds and loads.
        check("rows replacement under the burst takes no hold and no load of its own", results.columns1.lateHolds === results.columns0.lateHolds
              && results.columns1.lateLoads === results.columns0.lateLoads,
              results.columns1.lateHolds + "/" + results.columns0.lateHolds + " " + results.columns1.lateLoads + "/" + results.columns0.lateLoads)
        check("rows replacement rebinds each live delegate at most once", results.columns1.rebinds - results.columns0.rebinds <= results.columns1.delegates,
              (results.columns1.rebinds - results.columns0.rebinds) + " > " + results.columns1.delegates)
    }

    function advance() {
        if (phase === 0) {
            pane = paneComponent.createObject(win.contentItem)
            pane.open(fixture)
            phase = 1
        } else if (phase === 1) {
            // The storage class names the preview's verdict; until it lands the column loads nothing, and a sweep run before it would count nothing.
            if (pane.listInFlight || pane.total !== files || pane.rows.length === 0 || !pane.storageKnown) return
            check("the view is up", pane.viewMode === view, pane.viewMode)
            pane.listArea.forceActiveFocus()
            rest = 0
            phase = 2
        } else if (phase === 2) {
            // A second and a half of rest after the listing or the view switch, so a sweep starts from a settled window.
            rest += 1
            if (rest * 100 < 1500) return
            phase = 3
        } else if (phase === 3) {
            keys.keyClickChar("g", Qt.NoModifier, -1)
            rest = 0
            phase = 4
        } else if (phase === 4) {
            rest += 1
            if (rest * 100 < 1000) return
            check(view + " sweep " + sweep + " starts on row 0", pane.cursorIndex === 0, pane.cursorIndex)
            if (view === "columns" && mark === null) mark = findMark(preview())
            if (view === "columns") check("columns sweep " + sweep + " finds the preview column's loading mark", mark !== null)
            hookView()
            before = snapshot()
            trace = ""
            pressed = 0
            phase = 5
        } else if (phase === 6) {
            rest += 1
            if (rest * 100 < 1000) return
            report()
            sweep += 1
            if (sweep < sweeps) {
                phase = 3
            } else if (stage + 1 < stages.length) {
                // Phase first: the bindings that reach into the shown view stand down while it is swapped.
                phase = 1
                sweep = 0
                hooked = []
                mark = null
                pane.viewMode = stages[stage + 1]
                stage += 1
            } else {
                phase = 7
            }
        } else if (phase === 7) {
            phase = 8
            judge()
            finish()
        }
    }

    Timer { interval: 100; running: root.phase < 5 || root.phase === 6 || root.phase === 7; repeat: true; onTriggered: root.advance() }
    Timer {
        interval: root.stepMs
        running: root.phase === 5
        repeat: true
        onTriggered: {
            if (root.sending) return
            root.sending = true
            if (root.pressed < root.steps) {
                keys.keyPressChar("j", Qt.NoModifier, -1)
                root.pressed += 1
                var sw = root.swapNow()
                if (root.pressed === root.replaceAt) {
                    root.atReplace = { holds: sw === null ? 0 : sw.holds, loads: root.loads }
                    if (root.sweep === 1) pane.backend.window(pane.held, pane.windowSize)
                }
                root.trace += sw === null ? "-" : (sw.capturing ? "C" : sw.holding ? "H" : sw.fellBack ? "F" : ".")
            } else {
                keys.keyReleaseChar("j", Qt.NoModifier, -1)
                root.releaseFrames = root.frames
                root.rest = 0
                root.phase = 6
            }
            root.sending = false
        }
    }
    // Ends 30 s before the shell's timeout 150, so a stall names its phase before the timeout kills qs.
    Timer { interval: 120000; running: true; onTriggered: { console.log("COLHELD FAIL stalled in phase " + root.phase); Qt.quit() } }
}
