//@ pragma ShellId flea-search-answered-test

import QtQuick
import Quickshell
import "flea" as Flea
import "flea/js/Search.js" as Search
import "flea/js/Match.js" as Match

// A key struck in a live search changes only the strip: every row keeps drawing the query it answered until the next answer lands.
ShellRoot {
    id: root

    readonly property string fixture: Quickshell.env("ANSWERED_DIR")
    readonly property int tickMs: 20
    readonly property int giveUpMs: 8000
    readonly property int sampled: 10
    // Each op strikes with no event loop between the key and the snapshots, so the held frame is the one a key frame draws.
    readonly property var ops: [
        { open: true }, { start: true }, { strike: "n" }, { strike: "o" }, { strike: "r" }, { back: true }, { settle: 300 },
        { serialised: true }, { window: true }, { leave: true }, { reopen: true }, { settle: 300 }
    ]
    property int at: 0
    property var awaiting: null
    property double stamp: Date.now()
    property int checks: 0
    property int failures: 0
    property var pane: null
    property var keys: null
    property int gen: 0
    property string query: ""
    property bool ticking: false
    // Each answer's rows line by the query it answered, so a landing can be replayed with rows that count their reads.
    property var answers: ({})
    // Counted outside any QML property, so the proxy read inside a binding adds no dependency to it.
    property var tally: ({ serialised: 0 })
    // Rows lines with no gen, the plain window replies a scroll asks for.
    property int plainRows: 0

    function check(label, actual, expected) {
        root.checks += 1
        var ok = JSON.stringify(actual) === JSON.stringify(expected)
        if (!ok) root.failures += 1
        console.log("SEARCHANSWERED " + (ok ? "PASS " : "FAIL ") + label + " got=" + JSON.stringify(actual) + " expected=" + JSON.stringify(expected))
    }
    function finish() {
        console.log("SEARCHANSWERED DONE checks=" + root.checks + " failed=" + root.failures)
        Qt.exit(root.failures > 0 ? 1 : 0)
    }
    // What each drawn row of the first window shows: its name, location and mark, and which layout it is in.
    function frame() {
        var out = []
        for (var i = 0; i < root.sampled; i++) {
            var item = root.pane.visibleItemFor(i)
            out.push(item && item.visible && item.row
                ? { n: item.row.n, name: item.displayName, where: item.locationText, run: item.nameRun, searching: item.searching } : null)
        }
        return out
    }
    // The rows behind a proxy that counts each whole-array serialisation: JSON.stringify asks the array for toJSON once.
    // Qt 6.12 hands a handler's var rows as a sequence wrapper whose length getter refuses the proxy as its receiver, so the
    // proxy stands over a plain array of the same row objects.
    function counted(rows, tally) {
        return new Proxy(Array.prototype.slice.call(rows), { get: function (target, key, receiver) {
            if (key === "toJSON") tally.serialised += 1
            return Reflect.get(target, key, receiver)
        } })
    }
    function keep(start, items, ms, kinds, listing, gen) {
        if (gen === 0) root.plainRows += 1
        if (root.pane && gen > 0 && start === 0) root.answers[root.pane.searchQuery] = { items: items, kinds: kinds, listing: listing, total: root.pane.total }
    }
    function drawn(frame) { return frame.filter(function (cell) { return cell !== null }).length }
    // The newest sent gen answered and its walk over.
    function answered() { return root.pane.searchGen === root.gen && !root.pane.searchRunning && root.pane.searchSettledAt >= root.pane.searchKeyAt }
    function landed() { return !root.pane.listInFlight && root.pane.searchMode.length === 0 && root.pane.total > 0 && root.drawn(root.frame()) > 0 }

    FloatingWindow {
        id: win
        implicitWidth: 1400
        implicitHeight: 800
        color: Flea.Theme.color.background
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

    Timer {
        interval: root.tickMs
        running: true
        repeat: true
        onTriggered: {
            if (root.ticking) return
            root.ticking = true
            try { root.tick() } finally { root.ticking = false }
        }
    }

    function tick() {
        if (Date.now() - root.stamp > root.giveUpMs) {
            root.check("probe completes", "timeout at op " + root.at + " " + JSON.stringify(root.ops[root.at - 1]), "complete")
            root.finish()
            return
        }
        if (root.awaiting !== null && !root.awaiting()) return
        root.awaiting = null
        if (root.at >= root.ops.length) {
            root.finish()
            return
        }
        root.stamp = Date.now()
        root.run(root.ops[root.at])
        root.at += 1
    }

    // One key in the same JS turn as its two snapshots: nothing arrives from the backend between them.
    function strike(character, label) {
        var before = root.frame()
        root.gen += 1
        if (character === "BS") Search.shrink(root.pane)
        else Search.extend(root.pane, character)
        root.query = root.pane.searchQuery
        var held = root.frame()
        root.check(label + " the key frame draws the answered rows", root.drawn(before) > 0 && JSON.stringify(held) === JSON.stringify(before), true)
        root.check(label + " the strip echoes the key", root.pane.searchQuery, root.query)
        var sent = root.gen
        var query = root.query
        // The answer lands with its rows and marks in one sample: rows draw, and none names a file that does not hold the new query.
        root.awaiting = function () {
            if (!root.answered()) return false
            var stamps = Search.stamps(root.pane)
            console.log("SEARCHANSWERED STAMPS " + label + " keyToFirstRows=" + (stamps.firstRowsAt - stamps.keyAt) + "ms keyToSettled=" + (stamps.settledAt - stamps.keyAt) + "ms")
            var now = root.frame()
            root.check(label + " rows and marks both answer the new query", root.drawn(now) > 0 && now.every(function (cell) {
                return cell === null || (cell.searching && JSON.stringify(cell.run) === JSON.stringify(Match.run(cell.name, query)))
            }), true)
            return true
        }
    }

    function run(op) {
        if (op.open) {
            Flea.ViewState.changeKey("view", "list")
            root.pane = paneComponent.createObject(win.contentItem)
            root.keys = Qt.createQmlObject("import QtTest; TestEvent {}", root.pane.listArea)
            realBackend.rows.connect(root.keep)
            root.pane.open(root.fixture)
            root.awaiting = function () { return root.landed() }
        } else if (op.start) {
            root.pane.listArea.forceActiveFocus()
            root.keys.keyClickChar("f", Qt.NoModifier, -1)
            root.awaiting = function () { return root.pane.searchMode === "typing" }
        } else if (op.strike !== undefined) {
            // The first character meets the folder's own listing: its layout and marks stay until the first answer.
            var listing = root.query.length === 0
            if (listing) root.check("the folder lists before the first key", root.frame().every(function (cell) { return cell === null || !cell.searching }), true)
            root.strike(op.strike, "key " + (root.query.length + 1) + (listing ? " from the listing" : ""))
        } else if (op.back) {
            root.strike("BS", "backspace")
        } else if (op.serialised) {
            // "nor" lands over "no" with no serialisation; the first menu identity read serialises its rows, five cursor moves do not again, and rows landing again do.
            var p = root.pane, sw = p.swap, nor = root.answers["nor"]
            root.check("the nor answer was kept", nor !== undefined && nor.total !== p.total, true)
            var tally = root.tally
            p.searchGen += 1
            root.gen = p.searchGen
            sw.takeListed(nor.total, 0, 0, p.searchBase, -1, p.searchGen)
            sw.takeRows(0, root.counted(nor.items, tally), nor.kinds, nor.listing, p.searchGen)
            root.check("an answer landing serialises its rows 0 times", tally.serialised, 0)
            var first = p.menuSelectionIdentity()
            root.check("the first identity read serialises the rows once", tally.serialised, 1)
            tally.serialised = 0
            for (var i = 1; i <= 5; i++) p.setCursor(i)
            var moved = p.menuSelectionIdentity()
            root.check("five cursor moves and a second read serialise the rows 0 times", tally.serialised, 0)
            var head = JSON.parse(moved.split("\n")[0]), was = JSON.parse(first.split("\n")[0])
            root.check("and the second read names the moved cursor", [head[3], was[3] !== 5, moved.split("\n")[1] === first.split("\n")[1]].join(" "), "5 true true")
            sw.takeRows(0, root.counted(nor.items, tally), nor.kinds, nor.listing, p.searchGen)
            root.check("rows landing again serialise 0 times before a read", tally.serialised, 0)
            p.menuSelectionIdentity()
            root.check("rows landing again serialise once at the next read", tally.serialised, 1)
        } else if (op.window) {
            // A scroll past the held window asks for a plain window: its reply carries no gen and answers no query.
            var answered = root.pane.swap.answeredQuery
            root.check("a gen-stamped answer names its query", answered.length > 0 && answered === root.pane.searchQuery, true)
            var heard = root.plainRows
            realBackend.window(20, root.pane.windowSize)
            root.awaiting = function () {
                if (root.plainRows === heard) return false
                root.check("a plain window reply keeps the answered query", root.pane.swap.answeredQuery, answered)
                return true
            }
        } else if (op.leave) {
            Search.abort(root.pane)
            root.awaiting = function () {
                if (!root.landed()) return false
                root.check("the folder draws no search layout after the exit", root.frame().every(function (cell) { return cell === null || !cell.searching }), true)
                return true
            }
        } else if (op.reopen) {
            Search.start(root.pane)
            root.check("a new session starts from no answered query", root.pane.swap.answeredQuery, "")
            Search.abort(root.pane)
            root.awaiting = function () { return root.pane.searchMode.length === 0 }
        } else if (op.settle !== undefined) {
            // Rows still building at exit warn as items in creation at engine destruction.
            var until = Date.now() + op.settle
            root.awaiting = function () { return Date.now() >= until }
        }
    }
}
