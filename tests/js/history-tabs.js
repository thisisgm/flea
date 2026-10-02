.import "../../ui/js/History.js" as History
.import "../../ui/js/Filter.js" as Filter
.import "../../ui/js/Search.js" as Search
.import "../../ui/js/Tabs.js" as Tabs
.import "tabsfixture.js" as Fixture

// A cursor Back or Forward still owes travels with its tab: tests/js/tabsfixture.js's pane, holding
// a first window of a long listing, with the locate for "far" out when the hop happens.

function owing(path) {
    var p = Fixture.pane(path)
    p.held = 0
    p.rows = [{ n: "a" }, { n: "b" }]
    p.total = 900
    p.cursorIndex = 0
    p.asked = []
    p.restores = []
    p.rowFor = function (i) { return p.rows[i - p.held] || null }
    p.join = function (base, name) { return base + "/" + name }
    // ui/Pane.qml setCursor scrolls through showRow, which drops an owed cursor on every deliberate move.
    p.setCursor = function (i) { p.historyRestore = null; p.cursorIndex = Math.max(0, Math.min(p.total - 1, i)) }
    p.backend.heldListing = 5
    p.backend.send = function (message) { p.asked.push(message.path) }
    var listing = p.openWithoutHistory
    // ui/js/Nav.js openWithoutHistory arms whatever restore the hop hands it.
    p.openWithoutHistory = function (next, options) {
        p.restores.push(options && options.restore ? options.restore.name : "")
        listing(next, options)
        History.arm(p, options && options.restore)
    }
    History.resume(p, History.entry(path, "far", 450))
    return p
}

function answer(p, index) {
    History.located(p, { t: "located", directory: p.path, path: p.path + "/far", index: index })
}

function run(check) {
    var snap = owing("/home/gm/Work")
    check("a snapshot taken while a locate is out carries the owed row",
          Tabs.snapshot(snap).cursorRestore.name + "|" + (Tabs.snapshot(snap, "/tmp").cursorRestore === null), "far|true")
    answer(snap, 450)
    check("and once the answer landed it carries none", Tabs.snapshot(snap).cursorRestore, null)

    // Closing a tab the pane is not showing re-reads nothing, so the owed cursor still lands.
    var closing = owing("/home/gm/Work")
    closing.tabs = Tabs.pack([Tabs.snapshot(closing), { path: "/tmp" }], 0)
    Tabs.closeAt(closing, 1)
    answer(closing, 450)
    check("closing an inactive tab leaves the active tab's owed row to land", closing.cursorIndex, 450)

    // A clone of the same folder resumes the owed row by name; the tab left keeps it too.
    var cloned = owing("/home/gm/Work")
    Tabs.openNew(cloned)
    check("a new tab on the same folder asks for the owed row again rather than keeping row 0",
          cloned.asked.join(",") + "|" + cloned.tabs.items[0].cursorRestore.name, "/home/gm/Work/far,/home/gm/Work/far|far")
    answer(cloned, 451)
    check("and the answer lands it there", cloned.cursorIndex, 451)
    cloned.setCursor(7)
    Tabs.selectAt(cloned, 0)
    check("returning to the first tab resolves its owed row by name, not the index it was on",
          cloned.cursorIndex + "|" + cloned.asked.length, "7|3")
    answer(cloned, 452)
    check("and lands on the row the name is at now", cloned.cursorIndex, 452)

    // A tab on another folder hands its owed row to that folder's listing, and no index is pending.
    var away = owing("/home/gm/Work")
    var held = Tabs.snapshot(away)
    away.tabs = Tabs.pack([{ path: "/tmp", history: [], cursorIndex: 2, viewMode: "list", showHidden: false,
                             selected: [], sortBy: "name", sortDesc: false }, held], 0)
    away.path = "/tmp"
    away.historyRestore = null
    Tabs.selectAt(away, 1)
    check("switching to a tab elsewhere lists it with the owed row as the restore",
          away.restores.join(",") + "|" + away.tabs.pendingCursor + "|" + away.historyRestore.entry.name, "far|-1|far")

    // A tab whose order differs re-sorts first; the owed row waits for the reordered rows.
    var sorted = owing("/home/gm/Work")
    var mine = Tabs.snapshot(sorted)
    sorted.tabs = Tabs.pack([mine, Tabs.snapshot(sorted)], 0)
    sorted.tabs.items[1].sortBy = "size"
    sorted.tabs.items[1].cursorRestore = mine.cursorRestore
    Tabs.selectAt(sorted, 1)
    History.landed(sorted)
    check("a same-folder tab in another order waits while the old numbering still holds",
          sorted.sorted.join(",") + "|" + sorted.historyRestore.waiting + "|" + sorted.tabs.pendingCursor, "size:false|true|-1")
    sorted.backend.heldListing = 6
    sorted.rows = [{ n: "far" }, { n: "a" }]
    History.landed(sorted)
    check("and lands by name on the sort's own rows", sorted.cursorIndex + "|" + sorted.historyRestore, "0|null")

    // A listing's first rows answered with the tab's sort are the old order; the restore waits for the sort's.
    var pending = owing("/tmp/next")
    History.arm(pending, History.entry("/tmp/next", "far", 450))
    pending.listInFlight = true
    pending.listingPath = "/tmp/next"
    pending.tabs = Tabs.pack([{ path: "/tmp/next" }], 0)
    pending.tabs.pendingSortBy = "mtime"
    var started = Tabs.applyPending(pending)
    History.defer(pending)
    pending.listInFlight = false
    History.ended(pending)
    check("a tab's sort on the first rows defers the owed row past the listing's end",
          started + "|" + pending.historyRestore.waiting, "true|true")
    pending.backend.heldListing = 9
    History.landed(pending)
    check("and the sort's rows ask where the name went", pending.asked[pending.asked.length - 1], "/tmp/next/far")

    // A filter opened while the restore waits for reordered rows is newer intent: those rows, even
    // holding the owed name, must not move the cursor, and nothing more is asked of the backend.
    function waitingVia(route) {
        var p = owing("/home/gm/Work")
        if (route === "wait") {
            var own = Tabs.snapshot(p)
            p.tabs = Tabs.pack([own, Tabs.snapshot(p)], 0)
            p.tabs.items[1].sortBy = "size"
            p.tabs.items[1].cursorRestore = own.cursorRestore
            Tabs.selectAt(p, 1)
        } else {
            History.arm(p, History.entry("/home/gm/Work", "far", 450))
            p.listInFlight = true
            p.listingPath = "/home/gm/Work"
            p.tabs = Tabs.pack([{ path: "/home/gm/Work" }], 0)
            p.tabs.pendingSortBy = "mtime"
            Tabs.applyPending(p)
            History.defer(p)
            p.listInFlight = false
            History.ended(p)
        }
        return p
    }
    function reordered(p, filter) {
        if (filter === "strip") p.filterTyping = true
        if (filter === "query") p.filterQuery = "a"
        var asked = p.asked.length
        p.backend.heldListing = 7
        p.rows = [{ n: "a" }, { n: "far" }, { n: "b" }]
        History.landed(p)
        return p.cursorIndex + "|" + p.historyRestore + "|" + (p.asked.length - asked)
    }
    var routes = ["wait", "defer"]
    for (var r = 0; r < routes.length; r++) {
        check(routes[r] + ": an empty filter strip opened while waiting keeps the cursor and drops the restore",
              reordered(waitingVia(routes[r]), "strip"), "0|null|0")
        check(routes[r] + ": a query that keeps the cursor's row keeps the cursor there and drops the restore",
              reordered(waitingVia(routes[r]), "query"), "0|null|0")
        check(routes[r] + ": with no filter the reordered rows land the owed name where it now stands",
              reordered(waitingVia(routes[r]), ""), "1|null|0")
        var dismissedFilter = waitingVia(routes[r])
        Filter.start(dismissedFilter)
        Filter.close(dismissedFilter)
        check(routes[r] + ": a dismissed filter cannot revive a waiting cursor", reordered(dismissedFilter, ""), "0|null|0")
        var dismissedSearch = waitingVia(routes[r])
        Search.start(dismissedSearch)
        Search.close(dismissedSearch)
        check(routes[r] + ": a dismissed search cannot revive a waiting cursor", reordered(dismissedSearch, ""), "0|null|0")
        var picked = waitingVia(routes[r])
        picked.selectionVersion++
        check(routes[r] + ": a selection made while the sort waits keeps the cursor", reordered(picked, ""), "0|null|0")
    }
}
