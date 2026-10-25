import QtQuick
import "../ui/js/Scroll.js" as Scroll
import "../ui/js/SlowClick.js" as SlowClick

// Replays the native scroll physics and slow-click decisions through the unchanged oracle modules.
Item {
    function same(actual, expected) {
        if (typeof expected === "number")
            return typeof actual === "number" && (actual === expected || Math.abs(actual - expected) <= 1e-9 * Math.max(1, Math.abs(expected)))
        if (expected === null || typeof expected !== "object")
            return actual === expected
        if (Array.isArray(expected)) {
            if (!Array.isArray(actual) || actual.length !== expected.length)
                return false
            for (var i = 0; i < expected.length; i++)
                if (!same(actual[i], expected[i]))
                    return false
            return true
        }
        if (actual === null || typeof actual !== "object")
            return false
        for (var key in expected)
            if (!same(actual[key], expected[key]))
                return false
        return true
    }

    function qtMods(bits) {
        return ((bits & 1) ? Qt.ControlModifier : 0) | ((bits & 2) ? Qt.ShiftModifier : 0)
    }

    function sequence(row, states, step) {
        for (var i = 0; i < states.length; i++) {
            var actual = step(i)
            if (!same(actual, states[i]))
                return "step " + i + " actual=" + JSON.stringify(actual) + " expected=" + JSON.stringify(states[i])
        }
        return ""
    }

    function slow(row) {
        var pane = { renamePending: false, dragActive: false, selected: [], cursorIndex: 0 }
        pane.selectedIndices = function () { return pane.selected }
        pane.selectionCount = function () { return pane.selected.length }
        pane.act = function () {}
        return sequence(row, row.answers, function (i) {
            var step = row.steps[i]
            var answer = null
            if (step.op !== "cancel") {
                var gate = step.gate
                pane.singleClick = gate.single
                pane.clickRename = gate.clickRename
                pane.searchMode = gate.search ? "name" : ""
                pane.renamingIndex = gate.renaming ? 0 : -1
                pane.selectionBand = gate.band ? {} : null
                pane.menuVisible = gate.menu
            }
            if (step.op === "arm") {
                answer = SlowClick.arm(pane, step.index, qtMods(step.mods), step.now, step.interval, step.gate.drag, step.sole)
            } else if (step.op === "fire") {
                pane.selected = step.selected
                pane.cursorIndex = step.cursor
                var was = pane.slowClickIndex
                answer = SlowClick.fire(pane, step.gate.drag) ? was : -1
            } else {
                SlowClick.cancel(pane)
            }
            return [answer, pane.slowClickIndex === undefined ? SlowClick.CLEARED : pane.slowClickIndex]
        })
    }

    Component.onCompleted: {
        try {
            var modules = { Scroll: Scroll }
            var replay = { slow: slow }
            var path = Qt.application.arguments[Qt.application.arguments.length - 1]
            var request = new XMLHttpRequest()
            request.open("GET", "file://" + path, false)
            request.send()
            var lines = request.responseText.trim().split("\n")
            var checked = 0
            var failed = 0
            var kinds = {}
            for (var i = 0; i < lines.length; i++) {
                // Sample rows: {"kind": "fn", "m": "Scroll", "f": "distance", "args": [...], "out": 288} and {"kind": "slow", "steps": [...], "answers": [[true, 2]]}.
                var row = JSON.parse(lines[i])
                var problem
                if (row.kind === "fn") {
                    var module = modules[row.m]
                    if (!module || typeof module[row.f] !== "function") throw new Error("Unknown oracle function " + row.m + "." + row.f)
                    var actual = module[row.f].apply(null, row.args)
                    problem = same(actual, row.out) ? "" : "actual=" + JSON.stringify(actual)
                } else if (replay[row.kind]) {
                    problem = replay[row.kind](row)
                } else throw new Error("Unknown vector kind " + row.kind)
                var name = row.kind === "fn" ? row.m + "." + row.f : row.kind
                kinds[name] = (kinds[name] || 0) + 1
                if (problem !== "") {
                    if (failed < 20) console.log("FAIL native/oracle parity " + name + " " + problem + " row=" + lines[i].substring(0, 600))
                    failed++
                }
                checked++
            }
            var names = Object.keys(kinds).sort()
            console.log("G1_05 oracle kinds " + names.map(function (kind) { return kind + "=" + kinds[kind] }).join(" "))
            // vectors.rs ROW_FLOOR writes at least this many rows; 15 kinds is every scroll function plus the slow replay.
            var rowFloor = 7000
            var kindFloor = 15
            if (checked < rowFloor || names.length < kindFloor) { console.log("FAIL empty or incomplete vector matrix " + checked + " rows, " + names.length + " kinds"); Qt.exit(1); return }
            if (failed) { console.log("FAIL native/oracle mismatches " + failed + " of " + checked); Qt.exit(1); return }
            console.log("G1_05_ORACLE PASS " + checked + " decisions and replayed sequences over " + names.length + " kinds")
            Qt.exit(0)
        } catch (error) {
            console.log("FAIL oracle vector runner " + error)
            Qt.exit(1)
        }
    }
}
