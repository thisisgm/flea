.import "../../ui/js/Anchor.js" as Anchor
.import "../../ui/js/History.js" as History
.import "../../ui/js/Filter.js" as Filter
.import "../../ui/js/Search.js" as Search
.import "../../ui/js/Nav.js" as Nav
.import "nav.js" as NavSuite

// Back and Forward return the cursor to the row it left, by name: ui/js/History.js through Nav's
// public verbs. browsing() is tests/js/nav.js's navigating pane; the rows land here by hand, in the
// order ui/PaneSwap.qml runs them, and locate answers the way src/backend/run.rs does.

function rowsOf(names) { return names.map(function (name) { return { n: name } }) }

// A pane standing in /home/gm/Work with its cursor on row index, the stacks given as directories.
function standing(names, index, history) {
    var p = NavSuite.browsing(history || [])
    p.searchMode = ""
    p.held = 0
    p.rows = rowsOf(names)
    p.total = names.length
    p.cursorIndex = index
    p.selectionVersion = 0
    p.asked = []
    p.backend.heldListing = 1
    p.backend.send = function (message) { p.asked.push(message) }
    p.clearSelection = function () { p.selectionVersion++ }
    // ui/Pane.qml setCursor: Filter's clamp, then showRow, which drops an owed cursor on every deliberate move.
    p.setCursor = function (i) { p.historyRestore = null; p.cursorIndex = Math.max(0, Math.min(p.total - 1, i)) }
    return p
}

// The listing's first rows window: the swap's reset, the rows, then rowsLanded with the listing still out.
function land(p, names, total, listing) {
    Nav.forget(p)
    p.held = 0
    p.rows = rowsOf(names)
    p.total = total === undefined ? names.length : total
    p.backend.heldListing = listing || 2
    History.landed(p)
    p.listInFlight = false
    History.ended(p)
}

function answer(p, path, index, directory) {
    History.located(p, { t: "located", directory: directory || p.path, path: path, index: index })
}

function run(check) {
    // The round trip tests/js/nav.js held: forward keeps the departed directory, a refresh keeps the branch.
    var travel = standing(["a"], 0, ["/home/gm"])
    Nav.back(travel)
    check("back preserves the departed directory for forward", NavSuite.paths(travel.forwardHistory), "/home/gm/Work")
    Nav.forward(travel)
    check("forward while loading preserves the destination", travel.forwardHistory.length, 1)
    travel.listInFlight = false
    Nav.forward(travel)
    check("forward returns to the departed directory", travel.path, "/home/gm/Work")
    check("forward restores back history", NavSuite.paths(travel.history), "/home/gm")
    travel.listInFlight = false
    Nav.back(travel)
    travel.listInFlight = false
    Nav.open(travel, travel.path)
    check("refresh preserves forward history", travel.forwardHistory.length, 1)
    travel.listInFlight = false
    Nav.open(travel, "/tmp")
    check("new navigation discards the old forward branch", travel.forwardHistory.length, 0)

    // The defect: Back landed on row 0. Entering a folder records the row, and Back finds it by name
    // even after a file was created above it.
    var into = standing(["a", "b", "c", "d", "Music", "z"], 4)
    Nav.open(into, "/home/gm/Work/Music")
    check("entering a folder records the row the cursor left by name and index",
          into.history[0].path + "|" + into.history[0].name + "|" + into.history[0].index, "/home/gm/Work|Music|4")
    land(into, ["x", "y"])
    into.cursorIndex = 1
    Nav.back(into)
    check("back records the folder it leaves for forward", into.forwardHistory[0].path + "|" + into.forwardHistory[0].name,
          "/home/gm/Work/Music|y")
    land(into, ["a", "b", "new", "c", "d", "Music", "z"])
    check("back lands on the folder it left, by name, after an insertion renumbered it", into.cursorIndex, 5)
    check("and asks the backend for nothing more", into.asked.length + "|" + (into.historyRestore === null), "0|true")
    Nav.forward(into)
    land(into, ["y", "x"])
    check("forward lands on the row it left there, by name, after a reorder", into.cursorIndex, 0)
    into.cursorIndex = 1
    Nav.back(into)
    land(into, ["Music", "a"])
    check("a second back keeps finding the name wherever the sort put it", into.cursorIndex, 0)

    // Past the held window the name goes to the backend's name-only locate, then the cursor follows it.
    var deep = standing(["a"], 0, ["/home/gm"])
    deep.history = [History.entry("/home/gm", "item-450", 450)]
    Nav.back(deep)
    land(deep, ["item-0", "item-1", "item-2"], 900, 5)
    check("a name outside the held window asks locate for that one path",
          JSON.stringify(deep.asked), JSON.stringify([{ c: "locate", path: "/home/gm/item-450" }]))
    check("and leaves the cursor alone until it answers", deep.cursorIndex, 0)
    answer(deep, "/home/gm/item-450", 612)
    check("the answer moves the cursor to the row's index in this listing", deep.cursorIndex, 612)
    answer(deep, "/home/gm/item-450", 3)
    check("and is spent once, so a duplicate answer moves nothing", deep.cursorIndex, 612)

    // Gone since: the saved index, clamped to the listing, whether the window or locate said so.
    var gone = standing(["a"], 0)
    gone.history = [History.entry("/home/gm", "deleted", 7)]
    Nav.back(gone)
    land(gone, ["a", "b", "c"])
    check("a name missing from a listing held whole clamps without asking", gone.cursorIndex + "|" + gone.asked.length, "2|0")
    var lost = standing(["a"], 0)
    lost.history = [History.entry("/home/gm", "deleted", 7)]
    Nav.back(lost)
    land(lost, ["a", "b"], 300)
    answer(lost, "/home/gm/deleted", -1)
    check("a name locate cannot find falls back to the saved index", lost.cursorIndex, 7)
    var empty = standing(["a"], 0)
    empty.history = [History.entry("/home/gm", "deleted", 7)]
    Nav.back(empty)
    land(empty, [], 0)
    check("an empty directory settles at once with nothing owed",
          empty.cursorIndex + "|" + empty.asked.length + "|" + (empty.historyRestore === null), "0|0|true")

    // A late or foreign answer must not move a pane that moved on.
    function waiting() {
        var p = standing(["a"], 0)
        p.history = [History.entry("/home/gm", "far", 450)]
        Nav.back(p)
        land(p, ["a", "b"], 900, 5)
        return p
    }
    var moved = waiting()
    moved.setCursor(1)
    answer(moved, "/home/gm/far", 450)
    check("a cursor the user moved before the answer stays where they put it", moved.cursorIndex, 1)
    var picked = waiting()
    picked.selectionVersion++
    answer(picked, "/home/gm/far", 450)
    check("so does a selection made before the answer", picked.cursorIndex, 0)
    var resorted = waiting()
    resorted.backend.heldListing = 6
    answer(resorted, "/home/gm/far", 450)
    check("an answer read in the numbering a re-sort replaced moves nothing", resorted.cursorIndex, 0)
    var elsewhere = waiting()
    Nav.open(elsewhere, "/tmp")
    answer(elsewhere, "/home/gm/far", 450, "/home/gm")
    check("a newer navigation drops what the back still owed", elsewhere.historyRestore, null)
    land(elsewhere, ["far"], 1)
    check("and the old answer moves nothing in the directory it reached", elsewhere.cursorIndex, 0)
    var foreign = waiting()
    History.located(foreign, { t: "located", directory: "/home/gm", id: 3, transferId: 0, matches: [{ path: "/home/gm/far", index: 9 }] })
    answer(foreign, "/home/gm/other", 9)
    check("a batch answer or another path's answer neither moves nor spends the restore",
          foreign.cursorIndex + "|" + foreign.historyRestore.asked, "0|/home/gm/far")
    answer(foreign, "/home/gm/far", 450, "/home/gm/Work")
    check("an answer naming another directory moves nothing", foreign.cursorIndex, 0)
    var walking = waiting()
    walking.searchMode = "results"
    answer(walking, "/home/gm/far", 450)
    check("an answer landing under a search moves nothing", walking.cursorIndex, 0)

    // ui/Pane.qml onCursorClamped drops the restore when a wheel's clamp moves the cursor; any other
    // write that bypasses showRow meets the cursor recorded at the ask, so the answer still moves nothing.
    var clamped = waiting()
    clamped.cursorIndex = 44
    answer(clamped, "/home/gm/far", 450)
    check("a cursor written directly after the ask is kept over the late answer", clamped.cursorIndex, 44)
    var filtered = waiting()
    filtered.filterQuery = "fa"
    answer(filtered, "/home/gm/far", 450)
    var typing = waiting()
    typing.filterTyping = true
    answer(typing, "/home/gm/far", 450)
    check("a filter the user opened before the answer is not scrolled away from",
          filtered.cursorIndex + "|" + typing.cursorIndex, "0|0")

    var dismissedFilter = waiting()
    Filter.start(dismissedFilter)
    Filter.close(dismissedFilter)
    answer(dismissedFilter, "/home/gm/far", 450)
    check("dismissing a newly opened filter cannot revive the old locate", dismissedFilter.cursorIndex, 0)
    var dismissedSearch = waiting()
    dismissedSearch.searchFrom = ""
    Search.start(dismissedSearch)
    Search.close(dismissedSearch)
    answer(dismissedSearch, "/home/gm/far", 450)
    check("dismissing a newly opened search cannot revive the old locate", dismissedSearch.cursorIndex, 0)

    // Leaving before the answer records the row still owed, not the placeholder row 0 under it.
    var leaving = waiting()
    Nav.open(leaving, "/tmp")
    check("a navigation while the locate is out records the owed row by name",
          leaving.history[0].path + "|" + leaving.history[0].name + "|" + leaving.history[0].index, "/home/gm|far|450")
    var stepped = waiting()
    stepped.rows = rowsOf(["a", "b"])
    stepped.cursorIndex = 1
    Nav.open(stepped, "/tmp")
    check("but a cursor that moved since records where it actually is", stepped.history[0].name, "b")

    // The watch's re-read would anchor the placeholder row, so it waits for the answer, then is paid.
    function watchable(p) {
        p.renamingIndex = -1
        p.renamePending = false
        p.menuActions = { opened: false }
        p.selectionCount = function () { return 0 }
        p.selectionBand = null
        p.collide = { opened: false, pending: null }
        return p
    }
    var watched = watchable(waiting())
    var deferred = Anchor.busy(watched)
    answer(watched, "/home/gm/far", 450)
    check("a watched change waits while the locate is out and not after it lands",
          deferred + "|" + Anchor.busy(watched), "true|false")

    // The held swap's own reset runs after the request, so it must keep the cursor it was asked for.
    var held = standing(["a", "Music"], 1, ["/home/gm"])
    held.history = [History.entry("/home/gm", "Work", 1)]
    Nav.back(held)
    Nav.forget(held)
    check("the swap's reset keeps the cursor back still owes", held.historyRestore.entry.name, "Work")

    // A listing that failed answered no rows, so a later rows reply never spends its cursor.
    var failed = standing(["a"], 0)
    failed.history = [History.entry("/home/gm", "Work", 1)]
    Nav.back(failed)
    failed.listInFlight = false
    History.ended(failed)
    check("a failed listing drops the cursor it owed", failed.historyRestore, null)

    // A walk's rows are matches under its scope, so the entry names nothing to come back to; a query
    // retyped over those results is still the walk, while a query typed over the directory is not yet.
    var search = standing(["sub/found.txt"], 0, [])
    search.searchMode = "results"
    Nav.open(search, "/tmp")
    check("leaving a search records its scope at the top, naming no row",
          search.history[0].path + "|" + search.history[0].name + "|" + search.history[0].index, "/home/gm/Work||0")
    var retyped = standing(["sub/found.txt"], 0, [])
    retyped.searchMode = "typing"
    retyped.searchFrom = "/home/gm"
    Nav.open(retyped, "/tmp")
    var fresh = standing(["a", "b"], 1, [])
    fresh.searchMode = "typing"
    fresh.searchFrom = ""
    Nav.open(fresh, "/tmp")
    check("a query retyped over results names no row, one typed over the directory keeps its row",
          retyped.history[0].name + "|" + fresh.history[0].name + "|" + fresh.history[0].index, "|b|1")
}
