.pragma library

.import "../../ui/js/Nav.js" as Nav

// The stub panes the watched re-read and F5 suites share, so tests/js/watch.js and
// tests/js/refresh.js stay checks.

// Only the members openWithoutHistory writes, so the check is what a new listing forgets.
function pane() {
    var p = {
        listInFlight: false,
        listedSeen: true,
        path: "/home/gm",
        total: 40,
        held: 10,
        rows: [{ n: "a" }],
        kindNames: ["Plain text document"],
        thumbState: "stale",
        dirSizeState: "stale",
        cursorIndex: 7,
        renamingIndex: 4,
        trashArmedAt: 12345,
        listingState: "ready",
        stateMessage: "something",
        lockedMode: 0o40750,
        filterQuery: "scr",
        filterTyping: true,
        cleared: 0,
        said: [],
        sent: []
    }
    p.clearSelection = function () { p.cleared += 1 }
    p.renameEditor = function () { return null }
    p.message = function (text, isError) { p.said.push(text) }
    p.listArea = { primeSettle: function () {} }
    // ui/PaneSwap.qml with nothing held, so the reset and the query it hands back both run at the request.
    p.swap = { hold: function () { return false } }
    p.backend = {
        list: function (path, first, hidden) { p.sent.push("list " + path) },
        askFsInfo: function () { p.sent.push("fsinfo") },
        window: function (start, count) { p.sent.push("window " + start) }
    }
    return p
}

// Issue 68's re-read, which unlike a navigation puts the user back where they were. windowSize and
// setCursor are the two members only this path uses; rowFor is the pane's own held-window lookup.
function watched(held, rows, cursorIndex, total) {
    var p = pane()
    p.held = held
    p.rows = rows
    p.cursorIndex = cursorIndex
    p.total = total === undefined ? 40 : total
    p.windowSize = 350
    p.cursorSetTo = -1
    p.rowFor = function (index) {
        var offset = index - p.held
        return offset < 0 || offset >= p.rows.length ? null : p.rows[offset]
    }
    p.setCursor = function (index) { p.cursorSetTo = index }
    // Only a delete's own anchor selects; a watched re-read must never touch the operator's marks.
    p.selectedAt = -1
    p.selectOnly = function (index) { p.selectedAt = index; p.cursorSetTo = index }
    // The same wrapper ui/Pane.qml carries, so the re-read takes the one route that can refuse.
    p.openWithoutHistory = function (target, options) { Nav.openWithoutHistory(p, target, options) }
    return p
}
