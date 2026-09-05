.import "../../ui/js/RailKeys.js" as RailKeys

// The rail's own keys, split out of tests/js/focus.js at its 300-line cap the way focus-lines.js
// was: ui/js/Focus.js decides which surface owns a key, and this suite drives the rail surface.

function closed() {
    return { active: false, isMedia: false, isPdf: false }
}

function pane() {
    return { focusView: "list", viewMode: "list", searchMode: "", preview: closed() }
}

// The rail with focus, carrying the sink for what its menu case answers.
function railPane() {
    var p = pane()
    p.focusView = "rail"
    p.said = ""
    p.message = function (text, isError) { p.said = text }
    return p
}

// Only the members RailKeys.act's menu case reads, and a counter for the call it makes.
function rail(entries, cursor) {
    return { entries: entries, cursorIndex: cursor, opened: 0,
             openCursorMenu: function () { this.opened += 1 } }
}

// A pane and a rail for the eject key: the rail's rows and cursor, and a sink for what releaseChosen
// is handed, since that call is the whole of what the key must produce.
function ejectPane(path, entries, cursor) {
    var p = railPane()
    p.opened = 0
    p.openCursorMenu = function () { p.opened += 1; return true }
    p.path = path
    p.sidebar = { entries: entries, cursorIndex: cursor, released: [],
                  releaseChosen: function (action, key) { this.released.push(action + ":" + key) } }
    return p
}

function run(check) {
    var volume = { label: "128GB", group: "device", kind: "volume", device: "/dev/sda1", mounted: true }
    var home = { label: "Home", group: "favorite", kind: "favorite", path: "/home/user" }

    // The rail is a cursored list, so g and G mean there what the sheet says they mean. Both
    // answered nothing until v0.1.3, which is why tests/ui.sh sharebrowser pressed g to reset the
    // rail cursor, landed one row below the entry it wanted, and activated the wrong one.
    var railCursor = { entries: [1, 2, 3, 4], cursorIndex: 2 }
    RailKeys.act("cursorFirst", railPane(), railCursor)
    check("g takes the rail to its first row", railCursor.cursorIndex, 0)
    RailKeys.act("cursorLast", railPane(), railCursor)
    check("G takes the rail to its last row", railCursor.cursorIndex, 3)
    var emptyRail = { entries: [], cursorIndex: 0 }
    RailKeys.act("cursorLast", railPane(), emptyRail)
    check("and an empty rail has no last row to reach", emptyRail.cursorIndex, 0)

    var railing = railPane()
    var mounted = rail([volume], 0)
    RailKeys.act("menu", railing, mounted)
    check("m opens the menu on a mounted volume, and says nothing over it",
          mounted.opened + "|" + railing.said, "1|")
    var favourite = rail([home], 0)
    RailKeys.act("menu", railing, favourite)
    check("a row with nothing to release says why instead of swallowing the key",
          favourite.opened + "|" + railing.said, "0|Home has nothing to eject or unmount.")
    var empty = rail([], 0)
    railing.said = ""
    RailKeys.act("menu", railing, empty)
    check("an empty rail answers nothing at all rather than throwing",
          empty.opened + "|" + railing.said, "0|")

    // Finder's Cmd+E on the rail's own cursor row. The release goes through the same releaseChosen a
    // chosen menu row takes, carrying the row's key and not its index.
    var stick = { label: "128GB", group: "device", kind: "volume", device: "/dev/sda1", path: "/run/media/user/128GB", mounted: true }
    var ejecting = ejectPane("/home/user", [home, stick], 1)
    RailKeys.act("eject", ejecting, ejecting.sidebar)
    check("ctrl e in the rail ejects the cursor row by its key",
          ejecting.sidebar.released.join(",") + "|" + ejecting.said, "eject:/dev/sda1|")
    var favouriteRail = ejectPane("/home/user", [home, stick], 0)
    RailKeys.act("eject", favouriteRail, favouriteRail.sidebar)
    check("ctrl e on a favourite says why, and releases nothing",
          favouriteRail.sidebar.released.length + "|" + favouriteRail.said, "0|Home has nothing to eject or unmount.")
}
