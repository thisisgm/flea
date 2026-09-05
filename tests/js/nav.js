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

// A pane that can navigate: the two wrappers ui/Pane.qml carries, so back(), parent() and the mouse
// button all take the one route into openWithoutHistory rather than a stub that cannot refuse.
function browsing(history) {
    var p = pane()
    p.filterQuery = ""
    p.filterTyping = false
    p.path = "/home/gm/Work"
    p.history = history
    // ui/Pane.qml menuVisible: the pane's own context menu, which covers the listing it was raised over.
    p.menuVisible = false
    p.open = function (target) { Nav.open(p, target) }
    p.openWithoutHistory = function (target) { Nav.openWithoutHistory(p, target) }
    return p
}

// The two readings a crumb check makes: what the bar draws, and where each piece would take you.
function drawn(list) {
    return list.map(function (c) { return c.text }).join("")
}

function targets(list) {
    return list.map(function (c) { return c.path }).join(" ")
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

    // The in-flight guard has to run before the pop. openWithoutHistory is what refuses a listing
    // while one is loading, and by then back() has already shortened the history, so a back taken
    // during a load threw away the directory it was going to and went nowhere.
    var loading = browsing(["/home/gm"])
    loading.listInFlight = true
    Nav.back(loading)
    check("a back refused during a load keeps the history entry it was going to",
          loading.history.join(","), "/home/gm")
    check("and stays in the directory that is still loading", loading.path, "/home/gm/Work")
    check("and says so, which is the sentence every refused navigation gives",
          loading.said.join(""), "A directory is already loading.")
    check("and sends no listing", loading.sent.length, 0)

    // Issue 20 asked for the mouse's back button to climb. Nautilus and Explorer bind that button to
    // history, so it goes back where there is somewhere to go back to and climbs where there is not:
    // one button, both meanings, and no forward stack because the chrome draws one arrow.
    var remembered = browsing(["/home/gm"])
    Nav.mouseBack(remembered)
    check("mouse back with history behind it goes to the remembered directory",
          remembered.path, "/home/gm")
    check("and takes that entry off, so a second press is not the same place again",
          remembered.history.join(","), "")
    var climbing = browsing([])
    Nav.mouseBack(climbing)
    check("mouse back with no history climbs, which is the up arrow's own verb",
          climbing.path, "/home/gm")
    check("and remembers the directory it left, because climbing is a navigation",
          climbing.history.join(","), "/home/gm/Work")
    var atRoot = browsing([])
    atRoot.path = "/"
    Nav.mouseBack(atRoot)
    check("mouse back at the root with no history stays put and asks for no listing",
          atRoot.path + "|" + atRoot.sent.length, "/|0")
    var busyBack = browsing(["/home/gm"])
    busyBack.listInFlight = true
    Nav.mouseBack(busyBack)
    check("and a press during a load keeps the history it would have popped",
          busyBack.history.join(",") + "|" + busyBack.path, "/home/gm|/home/gm/Work")
    // An open context menu covers the listing and nothing in a navigation closes it, so a press
    // here left the menu standing over rows from another directory and its next row acted on
    // whatever had arrived at that index: on Move to Trash that is a different file trashed.
    var menuUp = browsing(["/home/gm"])
    menuUp.menuVisible = true
    Nav.mouseBack(menuUp)
    check("mouse back behind an open context menu goes nowhere at all",
          menuUp.path + "|" + menuUp.sent.length, "/home/gm/Work|0")
    check("and keeps the history entry it would have popped, so the menu's rows stay its own",
          menuUp.history.join(","), "/home/gm")

    // Issue 45: the chrome's path as the pieces a click can land on. The pieces have to concatenate
    // to exactly the one line they replace, or the bar draws something nobody asked for, and each
    // has to name the directory ui/ChromeBar.qml would hand to pathEntered.
    var under = Nav.crumbs("/home/gm/Work/claude", "/home/gm")
    check("the crumbs read as the tilde path they replace", drawn(under), "~/Work/claude")
    check("and each one names the directory it would open",
          targets(under), "/home/gm /home/gm/Work /home/gm/Work/claude")
    check("and only the last is the directory the pane is already in",
          under.map(function (c) { return c.last }).join(","), "false,false,true")
    var atHome = Nav.crumbs("/home/gm", "/home/gm")
    check("home itself is one crumb, the bare tilde",
          drawn(atHome) + "|" + targets(atHome), "~|/home/gm")
    var outside = Nav.crumbs("/usr/share", "/home/gm")
    check("a path outside home keeps its leading separator, which is a crumb of its own",
          drawn(outside) + "|" + targets(outside), "/usr/share|/ /usr /usr/share")
    var root = Nav.crumbs("/", "/home/gm")
    check("the root is one crumb and it is the last one",
          drawn(root) + "|" + targets(root) + "|" + root.length, "/|/|1")
    var noHome = Nav.crumbs("/home/gm/Work", "")
    check("with no home in the environment every component is its own crumb",
          drawn(noHome) + "|" + targets(noHome), "/home/gm/Work|/ /home /home/gm /home/gm/Work")
    // ui/js/Format.js tilde writes /home/gmx as "~x", which is a wrong label on a line nobody can
    // click and a wrong destination on one they can, so the crumbs test the separator themselves.
    var sibling = Nav.crumbs("/home/gmx/deep", "/home/gm")
    check("a sibling whose name merely starts with home's is outside it, and says so",
          drawn(sibling) + "|" + targets(sibling), "/home/gmx/deep|/ /home /home/gmx /home/gmx/deep")

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
