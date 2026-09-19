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
        // What ui/js/Nav.js records for the listing it asks for; ui/Pane.qml dropPath reads it.
        listingPath: "",
        total: 40,
        held: 10,
        rows: [{ n: "a" }],
        // The filter's own list, null while nothing is filtered, which is what the pane always carries.
        shown: null,
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
        // A listing that answers is what moves the pane: ui/PaneWire.qml onListed takes the path off
        // the answer, because Nav.js no longer writes it before the backend has agreed.
        list: function (path, first, hidden) { p.sent.push("list " + path); if (!p.refuses) p.path = path },
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
    p.forwardHistory = []
    // ui/Pane.qml menuVisible: the pane's own context menu, which covers the listing it was raised over.
    p.menuVisible = false
    p.open = function (target) { Nav.open(p, target) }
    p.openWithoutHistory = function (target) { Nav.openWithoutHistory(p, target) }
    return p
}

// What Enter did with one row, as one line: the directory it navigated to, the preview it opened,
// the path it handed the opener, and the path it handed the runner. Exactly one of the four may be
// filled for any row.
function entered(row) {
    var p = pane()
    var went = ["", "", "", ""]
    p.rowFor = function (index) { return row }
    p.join = function (base, name) { return base + "/" + name }
    p.open = function (target) { went[0] = target }
    p.preview = { open: function (path, icon, size) { went[1] = path + " " + icon + " " + size } }
    Nav.openCursor(p, { open: function (path) { went[2] = path },
                        run: function (path) { went[3] = path } })
    return went.join("|")
}

function run(check) {
    // StatusBar board rule 4's first lane: a refused hop leaves the breadcrumb where it was, which is
    // what taking the path off the answer buys. Nothing else in this suite can tell the two apart.
    var refused = browsing(["/home/gm"])
    refused.refuses = true
    Nav.open(refused, "/home/gm/Work/inner")
    check("a refused hop asks for the directory", refused.sent.join("|"), "list /home/gm/Work/inner|fsinfo")
    check("and leaves the pane standing where it was", refused.path, "/home/gm/Work")

    var travel = browsing(["/home/gm"])
    Nav.back(travel)
    check("back preserves the departed directory for forward", travel.forwardHistory.join("|"), "/home/gm/Work")
    Nav.forward(travel)
    check("forward while loading preserves the destination", travel.forwardHistory.length, 1)
    travel.listInFlight = false
    Nav.forward(travel)
    check("forward returns to the departed directory", travel.path, "/home/gm/Work")
    check("forward restores back history", travel.history.join("|"), "/home/gm")
    travel.listInFlight = false
    Nav.back(travel)
    travel.listInFlight = false
    Nav.open(travel, travel.path)
    check("refresh preserves forward history", travel.forwardHistory.length, 1)
    travel.listInFlight = false
    Nav.open(travel, "/tmp")
    check("new navigation discards the old forward branch", travel.forwardHistory.length, 0)

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
    // A drop taken while the reply is still out lands in the directory asked for and not the one
    // being left, so the request is recorded; the stub above answers at once, which the real
    // backend does not, and ui/Pane.qml dropPath reads this only while the listing is in flight.
    check("and records the directory it asked for, which is where a drop now lands",
          fresh.listingPath, "/home/gm/Work")
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
    busy.listingPath = "/home/gm/Music"
    Nav.openWithoutHistory(busy, "/home/gm/Work")
    check("a refused navigation sends nothing", busy.sent.length, 0)
    check("and leaves the listing in flight owning the path a drop would land in",
          busy.listingPath, "/home/gm/Music")
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

    // open()'s own copy of the guard back() carries. The push happened before openWithoutHistory
    // could refuse the listing, so a crumb clicked during a load stacked the directory the pane was
    // already standing in and the next back press navigated to where it already was.
    var busyOpen = browsing([])
    busyOpen.listInFlight = true
    Nav.open(busyOpen, "/home/gm")
    check("an open refused during a load remembers nothing", busyOpen.history.join(","), "")
    check("and stays where it is, saying the sentence every refused navigation gives",
          busyOpen.path + "|" + busyOpen.said.join(""),
          "/home/gm/Work|A directory is already loading.")

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

    // The operator's 0.1.4 ruling: Enter on an archive opens Flea's own view rather than handing the
    // file to this box's default for every archive type it can name, which is Nautilus.
    check("Enter on an archive opens Flea's own preview and launches nothing",
          entered({ n: "backup.zip", i: "package-x-generic", s: 4096, p: 0o100644 }),
          "|/home/gm/backup.zip package-x-generic 4096||")
    // The two answers that must not move, or the archive route would be a rewrite rather than a route.
    check("a directory still navigates and every other row still goes to the opener",
          entered({ n: "Work", d: true, p: 0o040755 }) + " / "
          + entered({ n: "notes.txt", i: "text-x-generic", s: 12, p: 0o100644 }),
          "/home/gm/Work||| / ||/home/gm/notes.txt|")

    // A file the operator has marked executable is a program, and the desktop database has no
    // handler for one: an AppImage is application-x-executable in generic-icons here, so the opener
    // could only come back refused. The row is already drawn in the executable colour off this same
    // mode, so Enter follows what the listing shows.
    check("Enter on an executable file starts it rather than asking the desktop to open it",
          entered({ n: "pcsx2.AppImage", i: "application-x-executable", s: 110086648, p: 0o100755 }),
          "|||/home/gm/pcsx2.AppImage")
    check("and the same file without the bit is still the desktop's to open",
          entered({ n: "pcsx2.AppImage", i: "application-x-executable", s: 110086648, p: 0o100644 }),
          "||/home/gm/pcsx2.AppImage|")
    // A script is a program the same way, which is the whole of what the execute bit says.
    check("an executable script is started too",
          entered({ n: "backup.sh", i: "text-x-script", s: 240, p: 0o100755 }),
          "|||/home/gm/backup.sh")
    // The one exception: only the desktop can read the Exec line inside a .desktop entry, so one
    // reaches the opener however it is moded.
    check("an executable desktop entry stays with the desktop",
          entered({ n: "steam.desktop", i: "application-x-desktop", s: 500, p: 0o100755 }),
          "||/home/gm/steam.desktop|")
    // The 0.1.4 archive ruling is untouched: the kind is read before the mode, so a marked archive
    // still opens Flea's own view rather than being handed to the kernel.
    check("an archive the operator marked executable still opens the preview",
          entered({ n: "backup.zip", i: "package-x-generic", s: 4096, p: 0o100755 }),
          "|/home/gm/backup.zip package-x-generic 4096||")
    // A directory's execute bit is the right to enter it and was never a program's.
    check("and a directory still navigates rather than being started",
          entered({ n: "Work", d: true, p: 0o040755 }), "/home/gm/Work|||")

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
