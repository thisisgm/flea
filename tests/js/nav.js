.import "../../ui/js/Nav.js" as Nav
.import "../../ui/js/Anchor.js" as Anchor
.import "../../ui/js/SlowClick.js" as SlowClick

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
        slowClickIndex: 0,
        slowClickAt: 1000,
        cancelled: 0,
        trashArmedAt: 12345,
        listingState: "ready",
        stateMessage: "something",
        lockedMode: 0o40750,
        filterQuery: "scr",
        filterTyping: true,
        cleared: 0,
        // The collision card, shut; ui/CollideHost.qml opened is what the mouse back button reads.
        collide: { opened: false },
        said: [],
        sent: []
    }
    p.clearSelection = function () { p.cleared += 1 }
    p.message = function (text, isError) { p.said.push(text) }
    // ui/Pane.qml cancelSlowClick stops the timer and clears the tap record.
    p.cancelSlowClick = function () { p.cancelled += 1; SlowClick.cancel(p) }
    p.listArea = { primeSettle: function () {} }
    // ui/PaneSwap.qml with nothing held, so the reset runs at the request; tests/js/swap.js holds.
    p.swap = { hold: function () { return false } }
    p.backend = {
        // A listing that answers is what moves the pane, ui/PaneSwap.qml applyListed, never the request.
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
    p.openWithoutHistory = function (target, options) { Nav.openWithoutHistory(p, target, options) }
    return p
}

// What Enter did with one row, as one line: the directory it navigated to, the preview it opened,
// and the path it handed the opener. Exactly one of the three may be filled for any row.
function entered(row) {
    var p = pane()
    var went = ["", "", ""]
    p.rowFor = function (index) { return row }
    p.join = function (base, name) { return base + "/" + name }
    p.open = function (target) { went[0] = target }
    p.preview = { open: function (path, icon, size) { went[1] = path + " " + icon + " " + size } }
    p.quickLook = function () { return p.preview }
    Nav.openCursor(p, { open: function (path) { went[2] = path } })
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
    // A re-list puts a different file at the tapped index, so the slow-click record goes with the rename.
    check("and clears the slow-click tap record", fresh.slowClickIndex, -2)
    check("and stops its timer through the pane", fresh.cancelled, 1)
    // The direct reset PaneSwap.release runs on a re-read: same clearing by name.
    var tapped = pane()
    tapped.slowClickIndex = 0
    Nav.forget(tapped)
    check("a re-list clears the slow-click record to SlowClick's cleared value",
          tapped.slowClickIndex, -2)
    check("and calls the pane's cancel", tapped.cancelled, 1)
    // A fixture pane without a slow-click timer is still forgotten, not crashed.
    var bare = pane()
    bare.cancelSlowClick = undefined
    bare.slowClickIndex = 0
    Nav.forget(bare)
    check("a pane without the timer is still forgotten", bare.total + "|" + bare.cursorIndex, "0|0")

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
    // The collision card holds a transfer into the folder it asked about, so the pane stays behind it.
    var cardUp = browsing(["/home/gm"])
    cardUp.collide = { opened: true }
    Nav.mouseBack(cardUp)
    check("mouse back behind an open collision card goes nowhere at all",
          cardUp.path + "|" + cardUp.sent.length + "|" + cardUp.history.join(","), "/home/gm/Work|0|/home/gm")
    // Show original's pending id belongs to the listing being left, so leaving it drops the reveal.
    var waiting = pane()
    waiting.linkTargetPendingId = 7
    Nav.openWithoutHistory(waiting, "/home/gm/Elsewhere")
    check("a navigation elsewhere drops a waiting Show original", waiting.linkTargetPendingId, 0)
    var staying = pane()
    staying.linkTargetPendingId = 7
    Nav.openWithoutHistory(staying, "/home/gm")
    check("a same-path re-read keeps it", staying.linkTargetPendingId, 7)
    var noHistory = browsing([])
    noHistory.collide = { opened: true }
    Nav.mouseBack(noHistory)
    check("and does not climb either", noHistory.path + "|" + noHistory.sent.length, "/home/gm/Work|0")

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
          entered({ n: "backup.zip", i: "package-x-generic", s: 4096 }),
          "|/home/gm/backup.zip package-x-generic 4096|")
    // The two answers that must not move, or the archive route would be a rewrite rather than a route.
    check("a directory still navigates and every other row still goes to the opener",
          entered({ n: "Work", d: true }) + " / " + entered({ n: "notes.txt", i: "text-x-generic", s: 12 }),
          "/home/gm/Work|| / ||/home/gm/notes.txt")

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

    // Issue 193, measured on the box at 0.3.4: Backspace on a refused hop's Locked tile climbed past the
    // folder the breadcrumb names, a whole listing away from the refused folder's own Permissions row.
    function lockedUp(path, asked) {
        var p = pane()
        p.path = path
        p.listingPath = asked
        p.listingState = "locked"
        p.opened = []
        p.open = function (to) { p.opened.push(to) }
        Nav.parent(p)
        return p.opened.join("") + " " + p.pendingSelect
    }
    check("up from a refused hop's Locked tile goes back to the folder the breadcrumb names, on the refused one",
          lockedUp("/home/gm/Downloads", "/home/gm/Downloads/locked"), "/home/gm/Downloads /home/gm/Downloads/locked")
    check("while a folder refused on its own re-read has no row there to return to, so it still climbs",
          lockedUp("/home/gm/Work", "/home/gm/Work"), "/home/gm /home/gm/Work")
    check("up from a Locked tile of a refused sidebar hop opens the refused folder's own parent, on the refused row",
          lockedUp("/home/gm/Downloads", "/root"), "/ /root")
    check("up from a Locked tile of a bookmark with a trailing slash trims it first",
          lockedUp("/home/gm/Downloads", "/root/"), "/ /root")

    // Defect 30: a new item is selected by the name the backend lists, so an NFC name on
    // hfsplus finds the NFD row it actually listed rather than missing it by bytes.
    var nfc = "Café"
    var nfd = nfc.normalize("NFD")
    check("the fixture really has two spellings", nfc === nfd, false)
    check("an exact row still matches first", Anchor.selectMatch([{ n: "a.txt" }, { n: "b.txt" }], "/d/b.txt", "/d"), 1)
    check("an NFC target finds its NFD row", Anchor.selectMatch([{ n: nfd }, { n: "other" }], "/d/" + nfc, "/d"), 0)
    check("an NFD target finds its NFC row", Anchor.selectMatch([{ n: nfc }], "/d/" + nfd, "/d"), 0)
    check("a name that is nowhere matches nothing", Anchor.selectMatch([{ n: "a.txt" }], "/d/nope.txt", "/d"), -1)
    check("a target outside the folder matches nothing", Anchor.selectMatch([{ n: "a.txt" }], "/elsewhere/a.txt", "/d"), -1)
    check("an equal-length sibling folder matches nothing", Anchor.selectMatch([{ n: "a.txt" }], "/e/a.txt", "/d"), -1)
    check("a USB1 row never matches a USB2 target", Anchor.selectMatch([{ n: "untitled folder" }], "/run/media/gm/USB2/untitled folder", "/run/media/gm/USB1"), -1)
    check("the root folder still matches its row", Anchor.selectMatch([{ n: "a.txt" }], "/a.txt", "/"), 0)
    check("an exact later row wins over an earlier NFC-only row", Anchor.selectMatch([{ n: nfd }, { n: nfc }], "/d/" + nfc, "/d"), 1)

    // Defect 26: past the wait a navigation starts clean instead of refusing forever.
    function waitingPane() {
        return { listInFlight: true, listingState: "waiting", stateMessage: "stale", path: "/mnt/dead",
                 history: [], said: [], message: function (t) { this.said.push(t) },
                 openWithoutHistory: function () {} }
    }
    var waiting = waitingPane()
    check("clearing a waiting pane ends its flight", Anchor.clearWaiting(waiting), true)
    check("its flight flag is gone", waiting.listInFlight, false)
    check("its state is loading again", waiting.listingState, "loading")
    check("its stale sentence is gone", waiting.stateMessage, "")
    var settled = { listInFlight: true, listingState: "loading", stateMessage: "" }
    check("a loading pane is left alone", Anchor.clearWaiting(settled), false)
    check("and keeps its flight", settled.listInFlight, true)

    var multi = pane()
    var openedTargets = []
    multi.path = "/music"
    multi.selectedIndices = function () { return [0, 1] }
    multi.rowFor = function (i) { return i === 0 ? { n: "1.mp3", d: false } : { n: "2.mp3", d: false } }
    multi.join = function (base, name) { return base + "/" + name }
    Nav.openCursor(multi, { open: function (paths) { openedTargets = paths } })
    check("multi-select opens all files together", openedTargets.join(","), "/music/1.mp3,/music/2.mp3")

    var mixed = pane()
    var mixedOpened = []
    mixed.path = "/music"
    mixed.selectedIndices = function () { return [0, 1] }
    mixed.rowFor = function (i) { return i === 0 ? { n: "1.mp3", d: false } : { n: "sub", d: true } }
    mixed.join = function (base, name) { return base + "/" + name }
    Nav.openCursor(mixed, { open: function (paths) { mixedOpened = paths } })
    check("multi-select with a directory refuses opening", mixedOpened.length, 0)
    check("and explains why", mixed.said[0], "Directories cannot be opened alongside files.")

    var arch = pane()
    var archOpened = []
    arch.path = "/music"
    arch.selectedIndices = function () { return [0, 1] }
    arch.rowFor = function (i) { return i === 0 ? { n: "song.mp3", d: false } : { n: "backup.zip", i: "package-x-generic", d: false } }
    arch.join = function (base, name) { return base + "/" + name }
    Nav.openCursor(arch, { open: function (paths) { archOpened = paths } })
    check("a selection holding an archive opens nothing", archOpened.length, 0)
    check("and says archives open alone", arch.said[0], "Archives open on their own, one at a time.")

    var far = pane()
    var farOpened = []
    far.path = "/music"
    far.selectedIndices = function () { return [0, 1, 2, 3, 4, 5] }
    far.rowFor = function (i) { return i < 2 ? { n: "song" + i + ".mp3", d: false } : null }
    far.join = function (base, name) { return base + "/" + name }
    Nav.openCursor(far, { open: function (paths) { farOpened = paths } })
    check("a selection reaching past the held window opens nothing", farOpened.length, 0)
    check("and says so instead of opening the part it can see", far.said[0], "Some selected files are outside the loaded rows.")
}
