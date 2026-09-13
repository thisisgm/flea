.import "../../ui/js/Anchor.js" as Anchor
.import "../../ui/js/Nav.js" as Nav

// Issue 68's watched re-read: a change another program made under the open listing is read again
// without moving the user off the file they were on. Its own suite because tests/js/nav.js sits at
// the 300-line JS hard cap, and because this is one behaviour rather than another navigation.

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
    p.message = function (text, isError) { p.said.push(text) }
    p.listArea = { primeSettle: function () {} }
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
    p.openWithoutHistory = function (target) { Nav.openWithoutHistory(p, target) }
    return p
}


function run(check) {
    // Issue 68: a change another program made under the listing is re-read in place. The cursor goes
    // back on the file it was on by name, because a create above it renumbers every row below.
    var seen = watched(0, [{ n: "a" }, { n: "b" }, { n: "c" }], 1)
    var anchor = Anchor.watched(seen)
    check("a watched re-read asks for the same directory again",
          seen.sent.join(","), "list /home/gm,fsinfo")
    check("and anchors on the name the cursor was on, not on its index",
          anchor.name + "|" + anchor.index, "b|1")
    check("and keeps the filter, which narrows rows rather than choosing the directory",
          seen.filterQuery, "scr")

    // The name moved down a row, which is exactly what a create above the cursor does.
    seen.held = 0
    seen.rows = [{ n: "NEW" }, { n: "a" }, { n: "b" }, { n: "c" }]
    seen.total = 41
    check("the cursor lands on the anchored name at its new index",
          Anchor.apply(seen, anchor) + "|" + seen.cursorSetTo, "null|2")

    // A name that is gone leaves the old index, which keeps the view where the user left it rather
    // than throwing them back to the top of the directory.
    var deleted = watched(0, [{ n: "a" }, { n: "c" }], 1, 2)
    check("a deleted anchor falls back to the index it had",
          Anchor.apply(deleted, { name: "b", index: 1, start: 0, path: "/home/gm" }) + "|" + deleted.cursorSetTo, "null|1")
    var shrunk = watched(0, [{ n: "a" }], 7, 1)
    check("and that index is clamped to what the directory now holds",
          Anchor.apply(shrunk, { name: "gone", index: 7, start: 0, path: "/home/gm" }) + "|" + shrunk.cursorSetTo, "null|0")
    var emptied = watched(0, [], 3, 0)
    check("a directory that emptied moves no cursor at all",
          Anchor.apply(emptied, { name: "gone", index: 3, start: 0, path: "/home/gm" }) + "|" + emptied.cursorSetTo, "null|-1")

    // A cursor deep in a large directory: the re-read answers from row 0, so its own window is asked
    // for and the anchor stands until that window arrives rather than giving up on the first reply.
    var deep = watched(4000, [{ n: "m" }, { n: "n" }], 4001, 100000)
    var deepAnchor = Anchor.watched(deep)
    check("a re-read below the first window asks for the window the cursor was in",
          deep.sent.join(","), "list /home/gm,fsinfo,window 4000")
    // onRows returns until onListed has run, so a reply always carries its total; see ui/PaneWire.qml.
    deep.held = 0
    deep.rows = [{ n: "a" }, { n: "b" }]
    deep.total = 100000
    check("and the first window, which cannot hold that name, does not resolve the anchor",
          Anchor.apply(deep, deepAnchor) === deepAnchor, true)
    check("and moves no cursor while it waits", deep.cursorSetTo, -1)
    // The wait is on the window arriving, not on a number of replies, so more of the first window
    // in between does not give up on it; the anchor leaks for good if this ever stops holding.
    check("more replies at the first window do not give up on the window asked for",
          Anchor.apply(deep, deepAnchor) === deepAnchor, true)
    check("and still move no cursor", deep.cursorSetTo, -1)
    // A listing that shrank past that offset comes back clamped to row 0, so the window asked for is
    // never coming; waiting on it for ever would leave the cursor unrestored and the anchor leaking.
    var clamped = watched(4000, [{ n: "m" }, { n: "n" }], 4001, 100000)
    var clampedAnchor = Anchor.watched(clamped)
    clamped.held = 0
    clamped.rows = [{ n: "a" }, { n: "b" }]
    clamped.total = 2
    check("a listing that shrank past the window asked for resolves against the clamp",
          Anchor.apply(clamped, clampedAnchor) + "|" + clamped.cursorSetTo, "null|1")
    // A listing of exactly start rows holds 0 to start-1, so window(start) is clamped here too: this is
    // the offset the comparison has to exclude, and a >= would wait on that reply for ever.
    var exact = watched(4000, [{ n: "m" }, { n: "n" }], 4001, 100000)
    var exactAnchor = Anchor.watched(exact)
    exact.held = 0
    exact.rows = [{ n: "a" }, { n: "b" }]
    exact.total = 4000
    check("a listing of exactly the offset asked for is clamped too, and resolves",
          Anchor.apply(exact, exactAnchor) + "|" + exact.cursorSetTo, "null|3999")
    deep.held = 4000
    deep.rows = [{ n: "m" }, { n: "n" }]
    check("the window it asked for is what puts the cursor back",
          Anchor.apply(deep, deepAnchor) + "|" + deep.cursorSetTo, "null|4001")

    // Nothing under the cursor is not a reason to refuse the re-read; the index still stands.
    var unloaded = watched(500, [{ n: "x" }], 3)
    var noRow = Anchor.watched(unloaded)
    check("a cursor over a row the pane does not hold anchors on no name", noRow.name, "")

    // The anchor can outlive one rows reply, so a navigation in between drops it rather than putting
    // this directory's cursor row onto the next directory's listing.
    var left = watched(0, [{ n: "a" }, { n: "b" }], 1)
    var leftAnchor = Anchor.watched(left)
    left.path = "/home/gm/Work"
    left.rows = [{ n: "b" }]
    left.total = 1
    check("an anchor from another directory is dropped, not applied",
          Anchor.apply(left, leftAnchor) + "|" + left.cursorSetTo, "null|-1")

    // A re-read while a listing is already running would queue a second one behind it.
    var loading = watched(0, [{ n: "a" }], 0)
    loading.listInFlight = true
    check("a re-read is refused while a list is in flight",
          Anchor.watched(loading) === null && loading.sent.length === 0, true)
    check("and an absent anchor resolves to nothing", Anchor.apply(loading, null), null)

    // Reported 2026-09-11: "deleting one refreshes the entire file list and loses my selection, so I
    // have to start over". The rows that were marked are gone, so the anchor is the deleted cursor
    // row and applyAnchor's own fallback lands on whatever took its place, selected.
    var deleted2 = watched(0, [{ n: "a" }, { n: "b" }, { n: "c" }], 1)
    var deleteAnchor = Anchor.afterDelete(deleted2)
    check("a delete re-reads the same directory", deleted2.sent.join(","), "list /home/gm,fsinfo")
    check("and anchors on the row that was deleted", deleteAnchor.name, "b")
    deleted2.rows = [{ n: "a" }, { n: "c" }]
    deleted2.total = 2
    check("the row that took its place takes the cursor",
          Anchor.apply(deleted2, deleteAnchor) + "|" + deleted2.cursorSetTo, "null|1")
    check("and it is selected, so the next delete needs no mouse", deleted2.selectedAt, 1)

    // The last row deleted has nothing below it, so the cursor lands on the new last row.
    var lastGone = watched(0, [{ n: "a" }, { n: "b" }, { n: "c" }], 2)
    var lastAnchor = Anchor.afterDelete(lastGone)
    lastGone.rows = [{ n: "a" }, { n: "b" }]
    lastGone.total = 2
    check("deleting the last row selects the new last row",
          Anchor.apply(lastGone, lastAnchor) + "|" + lastGone.selectedAt, "null|1")

    // A delete that failed leaves the row standing, and then the name matches and the cursor and the
    // selection both go back exactly where they were.
    var refused = watched(0, [{ n: "a" }, { n: "b" }, { n: "c" }], 1)
    var refusedAnchor = Anchor.afterDelete(refused)
    refused.rows = [{ n: "a" }, { n: "b" }, { n: "c" }]
    refused.total = 3
    check("a delete nothing removed puts the cursor back on the same file",
          Anchor.apply(refused, refusedAnchor) + "|" + refused.selectedAt, "null|1")

    // Emptying a directory leaves nothing to select, and selecting row -1 would be a mark on nothing.
    var emptied2 = watched(0, [{ n: "a" }], 0)
    var emptyAnchor = Anchor.afterDelete(emptied2)
    emptied2.rows = []
    emptied2.total = 0
    check("deleting the only row selects nothing rather than a row that is not there",
          Anchor.apply(emptied2, emptyAnchor) + "|" + emptied2.selectedAt, "null|-1")

    // The watch's own anchor must not have grown a selection with it: a change another program made
    // is not a reason to rewrite what the operator had marked.
    var untouched = watched(0, [{ n: "a" }, { n: "b" }], 1)
    var untouchedAnchor = Anchor.watched(untouched)
    untouched.rows = [{ n: "a" }, { n: "b" }]
    check("a watched re-read still only moves the cursor",
          Anchor.apply(untouched, untouchedAnchor) + "|" + untouched.selectedAt, "null|-1")

    // The same refusal the watched re-read carries, for the same reason.
    var busy = watched(0, [{ n: "a" }], 0)
    busy.listInFlight = true
    check("a delete re-read is refused while a list is in flight",
          Anchor.afterDelete(busy) === null && busy.sent.length === 0, true)
}
