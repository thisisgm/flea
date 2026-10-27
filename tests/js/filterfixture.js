.pragma library

.import "../../ui/js/Filter.js" as Filter
.import "../../ui/js/FilterWire.js" as FilterWire

// The filter narrows the listing already on screen, so every check here is pure or takes a stub
// pane: one that needed a live listing would be checking the wrong feature.

// Directories first is not optional, so the fixture is already in the order the backend produces:
// directories, then files, each run name ascending. Screens is capitalised on purpose.
//   0 Screens  1 scripts  2 tools  3 notes.md  4 readme.txt  5 screenrecording  6 screenshot
function listing() {
    return [
        { n: "Screens", d: true },
        { n: "scripts", d: true },
        { n: "tools", d: true },
        { n: "notes.md", d: false },
        { n: "readme.txt", d: false },
        { n: "screenrecording-2026-08-21.mp4", d: false },
        { n: "screenshot-2026-08-30.png", d: false }
    ]
}

// The same seven reversed the way the backend really returns them: directories still lead, each
// run reversed inside itself (docs/protocol.md, "sort").
//   0 tools  1 scripts  2 Screens  3 screenshot  4 screenrecording  5 readme.txt  6 notes.md
function reversed() {
    return [
        { n: "tools", d: true },
        { n: "scripts", d: true },
        { n: "Screens", d: true },
        { n: "screenshot-2026-08-30.png", d: false },
        { n: "screenrecording-2026-08-21.mp4", d: false },
        { n: "readme.txt", d: false },
        { n: "notes.md", d: false }
    ]
}

// Only the members the filter path touches. showRow records the view position it was asked for,
// because that scroll is the one thing ui/js/Filter.js hands back to ui/Pane.qml.
function pane(query, held) {
    var p = {
        rows: listing(),
        held: held === undefined ? 0 : held,
        total: 7,
        cursorIndex: 0,
        cursorSeq: 0,
        filterQuery: query === undefined ? "" : query,
        filterTyping: false,
        selectionVersion: 0,
        selectionAnchor: 0,
        scrolled: -1,
        contexts: [],
        picked: {},
        listInFlight: false,
        // ui/Pane.qml's FilterWire state: the ask's gen, numbering and query, the folder's answer, the view-space window of matched rows, and the window rules the drift check reads.
        filterGen: 0,
        filterAskedFor: -1,
        filterAsked: "",
        filterAnswer: null,
        filterHeld: 0,
        filterRows: [],
        filterKinds: [],
        kindNames: [],
        buffer: 50,
        windowSize: 130,
        refetchMargin: 25
    }
    // The backend calls the filter's wire makes, recorded in order: a window as "start:count", a filter as
    // "filter query:gen:at:before:count", a filter window as "fwindow start:count" and a close as "close".
    p.backend = {
        heldListing: 1,
        asked: [],
        window: function (start, count) { p.backend.asked.push(start + ":" + count) },
        filter: function (query, gen, at, before, count) { p.backend.asked.push("filter " + query + ":" + gen + ":" + at + ":" + before + ":" + count) },
        filterWindow: function (start, count) { p.backend.asked.push("fwindow " + start + ":" + count) },
        filterClose: function () { p.backend.asked.push("close") }
    }
    // ui/Pane.qml's own: the held window first, then the filter's window for a row only it holds.
    p.rowFor = function (index) {
        var offset = index - p.held
        if (offset >= 0 && offset < p.rows.length) return p.rows[offset]
        return p.shown === null || p.shown === undefined ? null : FilterWire.rowAt(p, index)
    }
    // The contexts each showRow was handed, so a pointer path proves its 0.
    p.showRow = function (view, context) {
        p.scrolled = view
        p.contexts.push(context)
    }
    // Mirrors ui/js/Selection.js: only() arms the lone row a plain move carries and every deliberate mark drops it.
    var lone = false
    var shiftBase = null
    var shiftLast = -1
    var shiftSeq = 0
    p.selection = {
        has: function (i) { return p.picked[i] === true },
        promote: function (i) { if (lone && p.selectedIndices().length === 1 && p.picked[i]) { lone = false; return true } return false },
        count: function () { return p.selectedIndices().length },
        only: function (i) { p.picked = {}; p.picked[i] = true; lone = true; shiftBase = null },
        toggle: function (i) { if (p.picked[i]) delete p.picked[i]; else p.picked[i] = true; lone = false; shiftBase = null },
        clear: function () { p.picked = {}; lone = false; shiftBase = null },
        all: function (n) { p.picked = {}; for (var i = 0; i < n; i++) p.picked[i] = true; lone = false; shiftBase = null },
        extendTo: function (i, anchor) {
            p.picked = {}
            for (var r = Math.min(i, anchor); r <= Math.max(i, anchor); r++) p.picked[r] = true
            lone = false
            shiftBase = null
        },
        shiftBegin: function (base, last, seq) { shiftBase = base.slice(); shiftLast = last; shiftSeq = seq || 0 },
        shiftMoved: function (last, seq) { shiftLast = last; shiftSeq = seq || 0 },
        shiftState: function () { return shiftBase === null ? null : { base: shiftBase, last: shiftLast, seq: shiftSeq } },
        shiftApply: function (range) {
            p.picked = {}
            for (var i = 0; i < shiftBase.length; i++) p.picked[shiftBase[i]] = true
            for (var j = 0; j < range.length; j++) p.picked[range[j]] = true
            lone = false
        },
        follows: function () { return lone && p.selectedIndices().length === 1 }
    }
    p.selectedIndices = function () {
        var out = []
        for (var k in p.picked) out.push(Number(k))
        out.sort(function (a, b) { return a - b })
        return out
    }
    // The two ui/Pane.qml computes as bindings, recomputed here so a stub can never go stale.
    p.refresh = function () {
        p.shown = FilterWire.standing(p.filterAnswer, p.rows, p.held, p.filterQuery)
        p.shownTotal = p.shown === null ? p.total : p.shown.length
    }
    p.refresh()
    return p
}

function typeInto(p, text) {
    for (var i = 0; i < text.length; i++) {
        Filter.typed(p, text.charAt(i))
        p.refresh()
    }
}

// Names the match list resolves to, so a check reads what is drawn and never an index.
function nameIn(rows) {
    return function (i) { return rows[i] === undefined ? "?" : rows[i].n }
}

function picks(p) {
    return p.selectedIndices().join(",")
}

