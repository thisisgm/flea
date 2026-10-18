import QtQuick
import "../ui/js/Keymap.js" as Keymap

Item {
    Component.onCompleted: {
        try {
        var arguments = Qt.application.arguments
        var path = arguments[arguments.length - 1]
        var request = new XMLHttpRequest()
        request.open("GET", "file://" + path, false)
        request.send()
        var lines = request.responseText.trim().split("\n")
        var checked = 0
        var failed = 0
        var sheets = {}
        var closed = {}
        var sheetGroups = 0
        for (var i = 0; i < lines.length; i++) {
            var row = JSON.parse(lines[i])
            var actual
            var expected
            if (row.kind === "key") {
                var modifiers = ((row.mods & 1) ? Qt.ControlModifier : 0)
                    | ((row.mods & 2) ? Qt.ShiftModifier : 0)
                    | ((row.mods & 4) ? Qt.AltModifier : 0)
                    | ((row.mods & 8) ? Qt.MetaModifier : 0)
                var key = row.key ? Qt["Key_" + row.key] : 0
                if (row.key && key === undefined) throw new Error("Undefined key " + row.key)
                actual = Keymap.lookupFor(row.preset, key, row.text, modifiers, row.context, row.frontend)
                expected = row.action
            } else if (row.kind === "hint") {
                actual = Keymap.hintsFor(row.preset, row.action)[row.action] || ""
                expected = row.hint
            } else if (row.kind === "sheet") {
                var group = row.preset + "|" + row.frontend
                if (closed[group]) throw new Error("Sheet row after its end " + group)
                var drawn = sheets[group] || (sheets[group] = [])
                actual = String(row.index)
                expected = String(drawn.length)
                drawn.push(row.index + "|" + row.action + "|" + row.keys + "|" + row.context)
            } else if (row.kind === "sheetend") {
                var ended = row.preset + "|" + row.frontend
                var native = sheets[ended] || []
                var oracle = Keymap.sheetFor(row.preset, row.frontend).map(function(candidate, index) {
                    return index + "|" + candidate.action + "|" + candidate.keys + "|" + candidate.context
                })
                closed[ended] = true
                sheetGroups++
                actual = native.length === row.count ? native.join(";") : "count " + native.length + " of " + row.count
                expected = oracle.join(";")
            } else throw new Error("Unknown vector kind " + row.kind)
            if (actual !== expected) {
                if (row.kind !== "sheetend") console.log("FAIL native/oracle parity " + JSON.stringify(row) + " actual=" + actual)
                else {
                    console.log("FAIL native/oracle sheet " + ended + " native " + native.length + " rows, oracle " + oracle.length)
                    var drawnRows = actual.split(";"), oracleRows = expected.split(";")
                    for (var at = 0; at < Math.max(drawnRows.length, oracleRows.length); at++) {
                        if (drawnRows[at] === oracleRows[at]) continue
                        console.log("G1_04 sheet first difference native=" + drawnRows[at] + " oracle=" + oracleRows[at])
                        break
                    }
                }
                failed++
            }
            checked++
        }
        for (var open in sheets) if (!closed[open]) { console.log("FAIL sheet without its end " + open); Qt.exit(1); return }
        if (sheetGroups !== 8) { console.log("FAIL sheet groups " + sheetGroups + " of 8"); Qt.exit(1); return }
        if (checked < 50000) { console.log("FAIL empty vector matrix"); Qt.exit(1); return }
        if (failed) { console.log("FAIL native/oracle mismatches " + failed); Qt.exit(1); return }
        console.log("G1_04_ORACLE PASS " + checked + " decisions, hints and sheet spellings")
        Qt.exit(0)
        } catch (error) {
            console.log("FAIL oracle vector runner " + error)
            Qt.exit(1)
        }
    }
}
