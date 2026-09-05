.import "../../ui/js/Nav.js" as Nav

// Nav.js had no suite at all, so nothing loaded it outside the running app and a broken .import in
// it would first have been seen on the box. These are its two pure functions, which ui/ColumnsArea.qml
// walks the Miller trail with, plus the reset that a fresh listing runs.

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
        pendingSelect: "",
        pendingCursor: -1,
        cleared: 0,
        said: [],
        sent: [],
        placed: []
    }
    p.clearSelection = function () { p.cleared += 1 }
    p.setCursor = function (index) { p.cursorIndex = index; p.placed.push(index) }
    p.join = function (base, name) { return base + "/" + name }
    p.message = function (text, isError) { p.said.push(text) }
    p.listArea = { primeSettle: function () {} }
    p.backend = {
        list: function (path, first, hidden) { p.sent.push("list " + path) },
        askFsInfo: function () { p.sent.push("fsinfo") }
    }
    return p
}

function run(check) {
    check("a path's parent is everything above its last separator", Nav.parentOf("/home/gm/Work"), "/home/gm")
    check("a child of the root has the root as its parent", Nav.parentOf("/home"), "/")
    check("the root is its own parent, which is where climbing stops", Nav.parentOf("/"), "/")
    check("a leaf is the last component", Nav.leafOf("/home/gm/Work"), "Work")
    check("a trailing separator leaves the path as its own leaf", Nav.leafOf("/home/gm/"), "/home/gm/")
    check("the root has no leaf of its own", Nav.leafOf("/"), "/")

    // Everything a fresh listing forgets, written once so no caller can half-do it. The filter is on
    // that list: it narrows the rows already listed, and these are about to be different rows.
    var fresh = pane()
    Nav.openWithoutHistory(fresh, "/home/gm/Work")
    check("a new listing forgets the filter's query", fresh.filterQuery, "")
    check("and hands the keyboard back off its query line", fresh.filterTyping, false)
    check("and forgets a half-pressed dd, the cursor and the selection",
          fresh.trashArmedAt + "|" + fresh.cursorIndex + "|" + fresh.cleared, "0|0|1")
    check("and asks the backend for the directory it was given",
          fresh.sent.join(","), "list /home/gm/Work,fsinfo")
    // The Locked state carries a mode string, so the reset that forgets the state must forget the
    // mode with it: a new directory drawn under the last one's permissions would be a false claim.
    check("and forgets the mode the last denial drew", fresh.lockedMode, 0)
    // The editor's row belongs to the listing being replaced. Leaving the index set opened an empty
    // editor over whatever file arrived at that row, and in the parent it was a directory.
    check("and forgets the open rename, whose row is about to be a different file",
          fresh.renamingIndex, -1)

    // The in-flight guard is what stops a second Enter queueing a listing behind one already asked
    // for, and nothing may be forgotten on a navigation that was refused.
    var busy = pane()
    busy.listInFlight = true
    Nav.openWithoutHistory(busy, "/home/gm/Work")
    check("a refused navigation sends nothing", busy.sent.length, 0)
    check("and says so", busy.said.join(""), "A directory is already loading.")
    check("and leaves the filter standing, because the listing did not change", busy.filterQuery, "scr")
    check("and leaves the cursor where it was", busy.cursorIndex, 7)
    check("and leaves the locked mode standing too", busy.lockedMode, 0o40750)
    check("and leaves an open rename alone, because the listing did not change", busy.renamingIndex, 4)

    // A refresh re-reads the directory under the listing rather than opening another one, so the
    // rows already drawn, their count and the cursor all stand until the new ones land: nothing
    // paints an empty frame, and the cursor's index is what the reply puts the cursor back on.
    var kept = pane()
    Nav.refresh(kept, "")
    check("a refresh leaves the rows, the count and the cursor standing",
          kept.rows.length + "|" + kept.total + "|" + kept.held + "|" + kept.cursorIndex, "1|40|10|7")
    check("and remembers the cursor's index for the rows reply", kept.pendingCursor, 7)
    check("and re-reads the directory it is in", kept.sent.join(","), "list /home/gm,fsinfo")
    check("but still forgets what is indexed by row, because the rows are about to be renumbered",
          kept.thumbState + "|" + kept.dirSizeState + "|" + kept.trashArmedAt + "|" + kept.cleared,
          "[object Object]|[object Object]|0|1")

    // A refresh that names a path reveals that row instead, the way rename and new folder do.
    var named = pane()
    Nav.refresh(named, "/home/gm/new.txt")
    check("a refresh with a target reveals the target rather than the old index",
          named.pendingSelect + "|" + named.pendingCursor, "/home/gm/new.txt|-1")

    // The rows reply applies the index once, clamped to the listing that came back: trashing the
    // last row leaves the cursor on the new last row, not one past the end.
    var landed = pane()
    landed.pendingCursor = 7
    landed.total = 5
    Nav.applyPendingSelect(landed)
    check("the rows reply puts the cursor back, clamped to the rows that came back",
          landed.placed.join(","), "4")
    check("and the pending cursor is one shot", landed.pendingCursor, -1)
    var empty = pane()
    empty.pendingCursor = 0
    empty.total = 0
    Nav.applyPendingSelect(empty)
    check("an emptied directory asks for row 0 and no lower", empty.placed.join(","), "0")
    var idle = pane()
    Nav.applyPendingSelect(idle)
    check("a reply with nothing pending moves the cursor nowhere", idle.placed.length, 0)

    // A navigation is not a re-read, so a pending cursor a failed refresh left behind must not
    // land on some row of the next directory opened.
    var moved = pane()
    moved.pendingCursor = 7
    Nav.openWithoutHistory(moved, "/home/gm/Work")
    check("a new listing forgets a pending cursor with everything else", moved.pendingCursor, -1)

    // A keyboard rename reveals the row it renamed; one the pointer committed keeps the row the
    // click chose instead, because a write operation targets the selection ahead of the cursor.
    var typed = { renameKeepsPointerRow: false }
    check("a keyboard rename re-reveals the row it renamed",
          Nav.renameRefreshTarget(typed, "/d/new.txt"), "/d/new.txt")
    var clicked = { renameKeepsPointerRow: true }
    check("a pointer-committed rename reveals nothing, so the click keeps its row",
          Nav.renameRefreshTarget(clicked, "/d/new.txt"), "")
    check("and the flag is one shot, so the next rename reveals again",
          Nav.renameRefreshTarget(clicked, "/d/new.txt"), "/d/new.txt")
}
