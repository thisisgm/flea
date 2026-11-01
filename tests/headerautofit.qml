//@ pragma ShellId flea-headerautofit-test

import QtQuick
import Quickshell
import "flea" as Flea
import "flea/js/Menu.js" as Menu
import "flea/js/Keymap.js" as Keymap
import "flea/js/Focus.js" as Focus

// Headerautofit (#333): the report's stored widths leave Modified and Kind switched on but undrawn in a
// 1388 px list. The header menu's Autofit columns row, run through Focus.act as Pane.act runs a chosen
// row, brings both back in one widths update; outside the list it writes nothing and says where it lives.
ShellRoot {
    id: root

    property var failures: []
    property int checks: 0
    property var row: ({ n: "report.txt", p: 33188, d: false, s: 18000, m: 1758835200, k: 0 })
    property string said: ""

    // The list header as the report had it: Mode hidden, Size, Modified and Kind switched on.
    Flea.Header {
        id: listHeader
        width: 1388
        hiddenCols: ["mode"]
        pane: ({ rows: [root.row], kindNames: ["Text"], dirSizeState: { file: {}, order: [] }, held: 0 })
    }

    // The members Focus.act reads for this action on a real Pane.
    QtObject {
        id: pane
        property string viewMode: "list"
        property var header: null
        function message(text, isError) { root.said = text }
    }

    // One counted check; a mismatch names what the row or the fit did.
    // Sample input: check("one update", 1, 1) passes silently.
    function check(label, actual, expected) {
        root.checks += 1
        if (actual !== expected)
            root.failures.push(label + ": got " + actual + ", expected " + expected)
    }

    // Sample input: Size and Kind drawn answers "size,kind".
    function drawn() {
        return ["size", "date", "kind"].filter(function (k) { return listHeader.cols[k] }).join(",")
    }

    Timer { interval: 300; running: true; onTriggered: root.run() }

    function run() {
        pane.header = listHeader
        Flea.ViewState.seed({ size: 480, kind: 295, date: 480, mode: 480 })
        Flea.ViewState.reset()
        check("the report's widths draw only Size beside Name", root.drawn(), "size")
        var fit = Menu.headerEntries(listHeader.hiddenCols, false).filter(function (r) { return r.action === "autofitColumns" })
        check("the header menu offers Autofit columns", fit.length === 1 ? fit[0].label : "", "Autofit columns")
        check("the row carries F4 as its hint", Keymap.hintFor("autofitColumns"), "f4")
        if (fit.length === 1)
            Focus.act(fit[0].action, pane)
        check("the row persists one widths update", Flea.ViewState.commits.length, 1)
        // The two the width dropped are fitted too, not only the drawn Size.
        var written = Flea.ViewState.commits.length === 1 ? Object.keys(Flea.ViewState.commits[0].entries).sort().join(",") : ""
        check("the update fits every switched-on column", written, "date,kind,size")
        check("Modified and Kind are drawn again", root.drawn(), "size,date,kind")
        check("the hidden Mode is not fitted", Flea.ViewState.state.columnWidths.mode, 480)
        pane.viewMode = "grid"
        Flea.ViewState.reset()
        Focus.act("autofitColumns", pane)
        check("outside the list the row writes nothing", Flea.ViewState.commits.length, 0)
        check("outside the list the row says where autofit lives", root.said, "Autofit needs the list view.")
        console.log("HEADERAUTOFIT DONE checks=" + root.checks)
        if (root.failures.length === 0)
            console.log("HEADERAUTOFIT PASS checks=" + root.checks)
        for (var f = 0; f < root.failures.length; f++)
            console.log("HEADERAUTOFIT FAIL " + root.failures[f])
        Quickshell.execDetached(["kill", String(Quickshell.processId)])
    }
}
