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
        pendingSelect: "",
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

    // h climbs the tree and keeps the place: the parent listing selects the directory we left.
    var up = pane()
    up.path = "/home/gm/Work"
    up.opened = []
    up.open = function (path) { up.opened.push(path) }
    Nav.parent(up)
    check("h opens the parent directory", up.opened.join(""), "/home/gm")
    check("and names the directory it left, so the cursor lands on it",
          up.pendingSelect, "/home/gm/Work")

    var rootDir = pane()
    rootDir.path = "/"
    rootDir.opened = []
    rootDir.open = function (path) { rootDir.opened.push(path) }
    Nav.parent(rootDir)
    check("the root does not climb", rootDir.opened.length, 0)
    check("and does not plant a select on a climb that did not happen",
          rootDir.pendingSelect, "")

    var home = pane()
    home.path = "/home"
    home.opened = []
    home.open = function (path) { home.opened.push(path) }
    Nav.parent(home)
    check("a child of the root climbs to the root", home.opened.join(""), "/")
    check("and still names the directory it left", home.pendingSelect, "/home")

    var busyUp = pane()
    busyUp.listInFlight = true
    busyUp.path = "/home/gm/Work"
    busyUp.opened = []
    busyUp.open = function (path) { busyUp.opened.push(path) }
    Nav.parent(busyUp)
    check("a refused climb sends nothing", busyUp.opened.length, 0)
    check("and plants no select", busyUp.pendingSelect, "")
}
