//@ pragma ShellId flea-filter-folder-test

import QtQuick
import Quickshell
import "flea" as Flea
import "flea/js/Filter.js" as Filter

// Issue 315 on the real Pane and backend, offscreen, in a 5,000-file folder whose held window is a
// screenful and a buffer each side: a query names a file the window does not hold, the folder's answer
// shows it with its row drawn, a query no name holds says so for the folder, and Esc parks the listing
// on the cursor's row with every drawn row held. Then the cursor moved 600 matches down a 1,000-match
// answer, where the view's reset to the top used to pull it: a watched re-read keeps it on its file at its
// place on screen, Esc keeps it, a letter its row still matches keeps it from outside the held window,
// and backspacing the query away parks on it.
ShellRoot {
    id: root

    readonly property string fixture: Quickshell.env("FILTERFOLDER_DIR")
    readonly property int folderRows: 5000
    readonly property int tickMs: 20
    readonly property int giveUpMs: 15000
    readonly property var ops: [
        { open: true },
        { cursor: 2000 }, { settle: 300 },
        { slash: true }, { type: "file-2345" }, { answered: true }, { settle: 300 },
        { check: "found" },
        { escape: true }, { settle: 300 },
        { check: "parked" },
        { slash: true }, { type: "zzz" }, { answered: true }, { settle: 300 },
        { check: "none" },
        { escape: true }, { settle: 300 },
        { check: "closed" },
        // file-2000 to file-2999 match, rows 1999 to 2998; the cursor goes to row 2899, file-2900.txt, view position 900.
        { slash: true }, { type: "file-2" }, { answered: true }, { settle: 300 },
        { cursorRow: 2899 }, { rowHeld: 2899 }, { settle: 300 },
        { check: "far" },
        // Enter commits the query, then another program touches the folder: the watched re-read renumbers every row.
        // The view first scrolls the cursor's row up the screen, off the place showRow would put it.
        { scrollRows: 10 }, { settle: 300 },
        { commit: true }, { settle: 300 },
        { reread: true }, { relanded: true }, { settle: 300 },
        { check: "reread" },
        { escape: true }, { rowHeld: 2899 }, { settle: 300 },
        { check: "parkedFar" },
        // Under "9" the cursor goes to row 899, file-0900.txt, outside the held window around 2899; "90" still names it.
        { slash: true }, { type: "9" }, { answered: true }, { settle: 300 },
        { cursorRow: 899 }, { rowHeld: 899 }, { settle: 300 },
        { type: "0" }, { answered: true }, { rowHeld: 899 }, { settle: 300 },
        { check: "kept" },
        { backspace: 2 }, { rowHeld: 899 }, { settle: 300 },
        { check: "emptied" }
    ]
    property int at: 0
    property var awaiting: null
    property double stamp: Date.now()
    property var failures: []
    property var results: []
    property var pane: null
    property var keys: null
    property int heldBefore: 0
    property double settledAt: 0
    property bool ticking: false
    // The re-read's numbering before, where the cursor's row stood on screen, and the first filter window after.
    property real listingBefore: -1
    property real offsetBefore: 0
    property int firstHeldAfter: -1

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

    function fail(text) { root.failures.push(text) }
    function landed() { return !root.pane.listInFlight && root.pane.total === root.folderRows && root.pane.rows.length > 0 }
    // The folder's answer is in once it names the standing query; a query with matches also brings their first window.
    function answered() {
        var p = root.pane
        if (p.listInFlight || p.filterAnswer === null || p.filterAnswer.query !== p.filterQuery) return false
        return p.shownTotal === 0 || p.filterRows.length > 0
    }
    function settle(ms) {
        root.settledAt = Date.now() + ms
        root.awaiting = function () { return Date.now() >= root.settledAt }
    }
    // The delegates inside the viewport, each with whether it has a row to draw.
    function drawn() {
        var v = root.pane.listArea
        var out = []
        var kids = v.contentItem.children
        for (var i = 0; i < kids.length; i++) {
            var k = kids[i]
            if (k.listingIndex === undefined || k.listingIndex < 0 || !k.visible) continue
            if (k.y + k.height <= v.contentY || k.y >= v.contentY + v.height) continue
            out.push({ index: k.listingIndex, held: !!k.row, name: k.row ? k.row.n : "" })
        }
        return out
    }
    function blank(rows) { return rows.filter(function (r) { return !r.held }).length }
    // Where a drawn row stands in the viewport, or null when no delegate draws it.
    function offsetOf(index) {
        var v = root.pane.listArea
        var kids = v.contentItem.children
        for (var i = 0; i < kids.length; i++) {
            if (kids[i].listingIndex === index && kids[i].visible) return kids[i].y - v.contentY
        }
        return null
    }

    Connections {
        target: root.pane
        function onFilterRowsChanged() {
            var p = root.pane
            if (root.listingBefore >= 0 && root.firstHeldAfter < 0 && p.filterAnswer !== null
                    && p.filterAnswer.listing !== root.listingBefore && p.filterRows.length > 0)
                root.firstHeldAfter = p.filterHeld
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
            root.fail("timeout at op " + root.at + " " + JSON.stringify(root.ops[root.at - 1]))
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

    function run(op) {
        if (op.open) {
            Flea.ViewState.changeKey("view", "list")
            root.pane = paneComponent.createObject(win.contentItem)
            root.keys = Qt.createQmlObject("import QtTest; TestEvent {}", root.pane.listArea)
            root.pane.open(root.fixture)
            root.awaiting = function () { return root.landed() }
        } else if (op.cursor !== undefined) {
            root.pane.listArea.forceActiveFocus()
            Filter.setCursorView(root.pane, op.cursor)
        } else if (op.cursorRow !== undefined) {
            Filter.setCursor(root.pane, op.cursorRow)
        } else if (op.rowHeld !== undefined) {
            root.awaiting = function () { return root.pane.rowFor(op.rowHeld) !== null }
        } else if (op.backspace !== undefined) {
            for (var b = 0; b < op.backspace; b++) root.keys.keyClick(Qt.Key_Backspace, Qt.NoModifier, -1)
        } else if (op.settle !== undefined) {
            root.settle(op.settle)
        } else if (op.slash) {
            root.heldBefore = root.pane.rows.length
            root.keys.keyClickChar("/", Qt.NoModifier, -1)
            root.awaiting = function () { return root.pane.filterTyping }
        } else if (op.type !== undefined) {
            for (var i = 0; i < op.type.length; i++) root.keys.keyClickChar(op.type.charAt(i), Qt.NoModifier, -1)
        } else if (op.answered) {
            root.awaiting = function () { return root.answered() }
        } else if (op.scrollRows !== undefined) {
            root.pane.listArea.contentY += op.scrollRows * Flea.Theme.fileRowHeight
        } else if (op.commit) {
            root.keys.keyClick(Qt.Key_Return, Qt.NoModifier, -1)
        } else if (op.reread) {
            root.listingBefore = root.pane.backend.heldListing
            root.offsetBefore = root.offsetOf(root.pane.cursorIndex)
            realBackend.changed(root.fixture)
        } else if (op.relanded) {
            root.awaiting = function () {
                return root.pane.backend.heldListing !== root.listingBefore && root.answered() && root.firstHeldAfter >= 0
            }
        } else if (op.escape) {
            root.keys.keyClick(Qt.Key_Escape, Qt.NoModifier, -1)
        } else if (op.check !== undefined) {
            root.check(op.check)
        }
    }

    function check(what) {
        var p = root.pane
        var rows = root.drawn()
        var tile = p.stateMessageItem
        var strip = Filter.summary(p.shown, p.filterTested, p.total)
        if (what === "found") {
            if (root.heldBefore >= p.total) root.fail("the held window was the whole folder, so nothing was tested")
            if (p.shownTotal !== 1 || p.shown[0] !== 2344) root.fail("file-2345 among 5,000 shows " + p.shownTotal + " rows: " + JSON.stringify(p.shown))
            if (p.cursorIndex !== 2344) root.fail("the cursor did not take the one row shown: " + p.cursorIndex)
            var row = p.rowFor(2344)
            if (!row || row.n !== "file-2345.txt") root.fail("the matched row outside the held window has no row to draw: " + JSON.stringify(row))
            if (rows.length !== 1 || !rows[0].held || rows[0].name !== "file-2345.txt") root.fail("drawn under the filter: " + JSON.stringify(rows))
            if (strip !== "1 of 5,000 shown") root.fail("the strip says " + JSON.stringify(strip))
            if (tile && tile.visible) root.fail("the no-match tile is up over a match")
            root.results.push("held=" + root.heldBefore + " shown=" + p.shownTotal + " row=" + (row ? row.n : "none") + " strip=" + strip)
        } else if (what === "parked") {
            if (p.filterQuery.length !== 0 || p.shown !== null) root.fail("Esc left the filter up")
            if (p.cursorIndex !== 2344) root.fail("Esc moved the cursor: " + p.cursorIndex)
            if (rows.length < 20 || blank(rows) !== 0) root.fail("after Esc " + blank(rows) + " of " + rows.length + " drawn rows have no row")
            if (!rows.some(function (r) { return r.index === 2344 })) root.fail("after Esc the cursor's row is not on screen")
            root.results.push("esc drawn=" + rows.length + " blank=" + blank(rows))
        } else if (what === "none") {
            if (p.shownTotal !== 0) root.fail("zzz shows " + p.shownTotal + " rows")
            if (!tile || !tile.visible || tile.detail !== "No match among 5,000 items") root.fail("the no-match tile says " + (tile ? tile.visible + ":" + JSON.stringify(tile.detail) : "nothing"))
            root.results.push("none tile=" + JSON.stringify(tile ? tile.detail : ""))
        } else if (what === "closed") {
            if (p.filterQuery.length !== 0 || blank(rows) !== 0) root.fail("Esc after no match left " + blank(rows) + " blank rows")
        } else if (what === "far") {
            if (p.shownTotal !== 1000 || p.cursorIndex !== 2899) root.fail("file-2 shows " + p.shownTotal + " rows with the cursor at " + p.cursorIndex)
            var far = p.rowFor(2899)
            if (!far || far.n !== "file-2900.txt") root.fail("the cursor's row 600 matches down has no row to draw: " + JSON.stringify(far))
            if (rows.length < 20 || blank(rows) !== 0) root.fail("600 matches down " + blank(rows) + " of " + rows.length + " drawn rows have no row")
            if (!rows.some(function (r) { return r.index === 2899 })) root.fail("the cursor's row is not on screen 600 matches down")
            root.results.push("far drawn=" + rows.length + " blank=" + blank(rows))
        } else if (what === "reread") {
            if (p.filterQuery !== "file-2" || p.filterTyping) root.fail("the re-read left the query " + JSON.stringify(p.filterQuery) + " typing=" + p.filterTyping)
            if (p.shownTotal !== 1000 || p.cursorIndex !== 2899) root.fail("after the re-read file-2 shows " + p.shownTotal + " rows with the cursor at " + p.cursorIndex)
            // The first window of the folder's answer is placed around the cursor's row, view position 900, so no row it draws waits on another ask.
            if (root.firstHeldAfter !== 900 - p.buffer) root.fail("the re-read's first filter window starts at " + root.firstHeldAfter + ", not " + (900 - p.buffer))
            if (rows.length < 20 || blank(rows) !== 0) root.fail("after the re-read " + blank(rows) + " of " + rows.length + " drawn rows have no row")
            var at = root.offsetOf(2899)
            if (at === null || root.offsetBefore === null || Math.abs(at - root.offsetBefore) > 0.5)
                root.fail("the re-read moved the cursor's row on screen from " + root.offsetBefore + " to " + at)
            root.results.push("reread first=" + root.firstHeldAfter + " offset=" + at)
        } else if (what === "parkedFar") {
            if (p.filterQuery.length !== 0 || p.shown !== null) root.fail("Esc on a scrolled result left the filter up")
            if (p.cursorIndex !== 2899) root.fail("Esc on a scrolled result moved the cursor: " + p.cursorIndex)
            if (rows.length < 20 || blank(rows) !== 0) root.fail("after Esc on a scrolled result " + blank(rows) + " of " + rows.length + " drawn rows have no row")
            if (!rows.some(function (r) { return r.index === 2899 })) root.fail("after Esc on a scrolled result the cursor's row is not on screen")
            root.results.push("esc far drawn=" + rows.length + " blank=" + blank(rows))
        } else if (what === "kept") {
            if (p.filterQuery !== "90") root.fail("the query is " + JSON.stringify(p.filterQuery))
            if (p.cursorIndex !== 899) root.fail("a letter moved the cursor off a row that still matches: " + p.cursorIndex)
            if (Filter.viewOf(p.shown, 899) < 0) root.fail("the folder's answer does not show the cursor's row")
            if (rows.length < 10 || blank(rows) !== 0) root.fail("under 90 " + blank(rows) + " of " + rows.length + " drawn rows have no row")
            if (!rows.some(function (r) { return r.index === 899 })) root.fail("the cursor's row is not on screen under 90")
            root.results.push("kept shown=" + p.shownTotal + " drawn=" + rows.length)
        } else if (what === "emptied") {
            if (p.filterQuery.length !== 0 || p.shown !== null) root.fail("backspacing the query away left the filter up")
            if (p.cursorIndex !== 899) root.fail("backspacing the query away moved the cursor: " + p.cursorIndex)
            if (rows.length < 20 || blank(rows) !== 0) root.fail("after the query emptied " + blank(rows) + " of " + rows.length + " drawn rows have no row")
            if (!rows.some(function (r) { return r.index === 899 })) root.fail("after the query emptied the cursor's row is not on screen")
            root.results.push("emptied drawn=" + rows.length + " blank=" + blank(rows))
        }
    }

    function finish() {
        if (root.failures.length > 0) console.log("FILTERFOLDER FAIL " + root.failures.join("; "))
        else console.log("FILTERFOLDER PASS " + root.results.join("; "))
        console.log("FILTERFOLDER DONE")
        Qt.exit(root.failures.length > 0 ? 1 : 0)
    }
}
