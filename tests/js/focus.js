.import "../../ui/js/Filter.js" as Filter
.import "../../ui/js/Focus.js" as Focus
.import "filterfixture.js" as Fixture

// Focus.lookup is where a key is discarded for being meaningless in the current state, and a wrong
// gate there is silent: the key simply does nothing, and no suite but this one would notice.

function pane(preview, viewMode) {
    return {
        focusView: "list",
        viewMode: viewMode ? viewMode : "list",
        searchMode: "",
        preview: preview
    }
}

// A pane showing search results, which is the one state the two sort keys are taken away in.
function searching(preview) {
    var p = pane(preview)
    p.searchMode = "results"
    return p
}

// The rail with focus, the one view m means anything in, carrying the sink for what it answers.
function railPane() {
    var p = pane(closed())
    p.focusView = "rail"
    p.said = ""
    p.message = function (text, isError) { p.said = text }
    return p
}

// A pane and a rail for the eject key: the rail's rows and cursor, and a sink for what releaseChosen
// is handed, since that call is the whole of what the key must produce.
function ejectPane(view, path, entries, cursor) {
    var p = listPane(true)
    p.focusView = view
    p.path = path
    p.sidebar = { entries: entries, cursorIndex: cursor, released: [],
                  releaseChosen: function (action, key) { this.released.push(action + ":" + key) } }
    return p
}

// The listing with focus and only what Focus.act's menu case reads: the pane's own opener, which
// answers whether a row was under the cursor, and the sink for what the key says when none was.
function listPane(hasRow) {
    var p = pane(closed())
    p.opened = 0
    p.said = ""
    p.openCursorMenu = function () { p.opened += 1; return hasRow }
    p.message = function (text, isError) { p.said = text }
    return p
}

function closed() {
    return { active: false, isMedia: false, isPdf: false }
}

function pdfOpen() {
    return { active: true, isMedia: false, isPdf: true }
}

function mediaOpen() {
    return { active: true, isMedia: true, isPdf: false }
}

function key(code, text, modifiers) {
    return { key: code, text: text, modifiers: modifiers }
}

// Only the members the escape case reads. Search.cancel and Pane.escapePressed both record rather
// than act, because what is being checked is the order they are reached in.
function escaper(query, retreated) {
    var p = pane(closed())
    p.filterQuery = query
    p.filterTyping = false
    p.retreated = retreated
    p.cancelled = 0
    p.searchRunning = false
    p.backend = { searchcancel: function () { p.cancelled += 1 } }
    p.escapePressed = function () { p.retreated += 1 }
    return p
}

function run(check) {
    var none = Qt.NoModifier
    var shift = Qt.ShiftModifier

    var minus = key(Qt.Key_Minus, "-", none)
    var plus = key(Qt.Key_Plus, "+", shift)
    var e = key(Qt.Key_E, "e", none)
    var left = key(Qt.Key_Left, "", none)
    var right = key(Qt.Key_Right, "", none)

    check("e expands an open PDF", Focus.lookup(e, pane(pdfOpen())), "expand")
    check("minus zooms an open PDF", Focus.lookup(minus, pane(pdfOpen())), "zoomOut")
    check("plus zooms an open PDF", Focus.lookup(plus, pane(pdfOpen())), "zoomIn")

    // The three are silent everywhere else, so none of them acts while the list has the keys.
    check("e is discarded while browsing", Focus.lookup(e, pane(closed())), "")
    check("minus is discarded while browsing", Focus.lookup(minus, pane(closed())), "")
    check("plus is discarded while browsing", Focus.lookup(plus, pane(closed())), "")
    check("e is discarded over a media preview", Focus.lookup(e, pane(mediaOpen())), "")
    check("minus is discarded over a media preview", Focus.lookup(minus, pane(mediaOpen())), "")

    // Left and Right now serve two previews, and must still serve the grid and nothing else.
    check("left turns a PDF page", Focus.lookup(left, pane(pdfOpen())), "seekBack")
    check("right turns a PDF page", Focus.lookup(right, pane(pdfOpen())), "seekForward")
    check("left still seeks media", Focus.lookup(left, pane(mediaOpen())), "seekBack")
    check("left is discarded in the list", Focus.lookup(left, pane(closed())), "")
    check("left still steps a grid tile", Focus.lookup(left, pane(closed(), "grid")), "cursorLeft")
    check("right still steps a grid tile", Focus.lookup(right, pane(closed(), "grid")), "cursorRight")

    // Nothing in keys.toml is bound ahead of its feature now: lookup hands both actions through
    // and handleKey routes each above the views, so neither answers with a sentence any more.
    var colon = key(Qt.Key_Colon, ":", shift)
    check("colon resolves to the path bar", Focus.lookup(colon, pane(closed())), "pathBar")
    var newTab = key(Qt.Key_T, "t", none)
    check("t resolves to a new tab", Focus.lookup(newTab, pane(closed())), "tabNew")

    // The filter narrows rows already on screen, which only the list view draws; the GridView and
    // Columns boards draw no filter, so / is dropped there rather than narrowing a view nothing shows.
    var slash = key(Qt.Key_Slash, "/", none)
    check("slash opens the filter in the list view", Focus.lookup(slash, pane(closed())), "filter")
    check("slash is discarded in the grid", Focus.lookup(slash, pane(closed(), "grid")), "")
    check("slash is discarded in the columns view", Focus.lookup(slash, pane(closed(), "columns")), "")
    // A walk replaces the listing a filter would be narrowing, and its strip covers the header, so
    // / goes quiet there exactly as s and S do.
    check("slash is discarded while a search owns the header",
          Focus.lookup(slash, searching(closed())), "")

    // And starting one drops a filter that was standing, or the results are narrowed by a query that
    // was written against the directory listing they just replaced.
    var walker = escaper("scr", 0)
    walker.started = 0
    walker.searchMode = ""
    Focus.act("search", walker)
    check("f clears a standing filter before the query line opens",
          walker.filterQuery + "|" + walker.searchMode, "|typing")

    // Esc unwinds one thing at a time, least destructive first. A filter costs nothing to clear, a
    // selection is work, so the filter goes first and a second esc is what drops the selection.
    var unwind = escaper("scr", 0)
    Focus.act("escape", unwind)
    check("esc clears a standing filter before it touches the selection",
          unwind.filterQuery + "|" + unwind.retreated, "|0")
    Focus.act("escape", unwind)
    check("and a second esc reaches the selection", unwind.retreated, 1)
    var walking = escaper("", 0)
    walking.searchMode = "results"
    walking.searchRunning = true
    Focus.act("escape", walking)
    check("a running search still outranks both", walking.cancelled, 1)
    var typing = escaper("", 0)
    typing.filterTyping = true
    Focus.act("escape", typing)
    check("esc while the query line has the caret closes it", typing.filterTyping, false)

    // The search strip covers the header whole, so its mark cannot be seen moving, and a sort ends
    // the walk in the backend. Both keys go silent while a search is up rather than cancelling one
    // from a key the sheet never advertised there.
    var sortNext = key(Qt.Key_S, "s", none)
    var sortReverse = key(Qt.Key_S, "S", shift)
    check("s sorts while browsing", Focus.lookup(sortNext, pane(closed())), "sortNext")
    check("S reverses while browsing", Focus.lookup(sortReverse, pane(closed())), "sortReverse")
    check("s is discarded over a search result", Focus.lookup(sortNext, searching(closed())), "")
    check("S is discarded over a search result", Focus.lookup(sortReverse, searching(closed())), "")

    // m is the one key into the menu a right click raises, in both views: the rail's rows and the
    // listing's row menu, which had no key at all, so it is routed by which view has focus.
    var m = key(Qt.Key_M, "m", none)
    var home = { label: "Home", group: "favorite", kind: "favorite", path: "/home/user" }
    check("m raises the menu while the rail has focus", Focus.lookup(m, railPane()), "menu")
    check("m raises the menu in the list too, so the row menu has a key", Focus.lookup(m, pane(closed())), "menu")

    // Finder's Cmd+K with Cmd read as Ctrl opens the dialog from either view; the bare a stays a rail
    // key, because in the list the letter is not bound at all.
    var ctrl = Qt.ControlModifier
    check("ctrl k connects to a server from the list", Focus.lookup(key(Qt.Key_K, "\u000b", ctrl), pane(closed())), "addNetwork")
    check("bare a is still nothing in the list", Focus.lookup(key(Qt.Key_A, "a", none), pane(closed())), "")
    var dialled = listPane(true)
    dialled.sidebar = { asked: 0, addRequested: function () { this.asked += 1 } }
    Focus.act("addNetwork", dialled)
    check("and act opens it through the rail's own signal", dialled.sidebar.asked, 1)

    // Finder's Cmd+1/2/3: the same property the chrome's three buttons write, so they follow.
    var viewed = listPane(true)
    Focus.act("viewGrid", viewed)
    var grid = viewed.viewMode
    Focus.act("viewColumns", viewed)
    var cols = viewed.viewMode
    Focus.act("viewList", viewed)
    check("ctrl 3, 2 and 1 pick the grid, the columns and the list", grid + "|" + cols + "|" + viewed.viewMode, "grid|columns|list")

    // The seam itself. A case that only messaged was indistinguishable from a wired one on this
    // side of the suite, which is how the whole feature stayed unreachable through a green run, so
    // the check names the request that has to reach the backend and asserts no message replaces it.
    var folder = listPane(true)
    folder.path = "/d"
    folder.made = []
    folder.backend = { mkdir: function (path) { folder.made.push(path) } }
    Focus.act("newFolder", folder)
    check("ctrl shift n asks the backend for a folder in the listed directory, and says nothing",
          folder.made.join(",") + "|" + folder.said, "/d|")

    // Finder's Cmd+E in a listing: the removable volume the listing is inside, whose verdict is
    // Mounts.railMenu's, released through the same releaseChosen a chosen menu row takes. The rail's
    // own half of the key is ui/js/RailKeys.js's, and tests/js/railkeys.js drives it.
    var stick = { label: "128GB", group: "device", kind: "volume", device: "/dev/sda1", path: "/run/media/user/128GB", mounted: true }
    var inside = ejectPane("list", "/run/media/user/128GB/photos", [home, stick], 0)
    Focus.act("eject", inside)
    check("ctrl e in a listing inside the volume ejects that volume, whatever the rail cursor is on",
          inside.sidebar.released.join(",") + "|" + inside.said, "eject:/dev/sda1|")
    var outside = ejectPane("list", "/home/user/Documents", [home, stick], 1)
    Focus.act("eject", outside)
    check("ctrl e in a listing on the internal disk says so, even with the rail cursor on the stick",
          outside.sidebar.released.length + "|" + outside.said,
          "0|This directory is not inside a removable volume, so there is nothing to eject.")

    // Issue 27: what a cursor step past an end does. The state file's wrapAtEnds is off by default,
    // so the shipped answer is still the clamp the operator who reported the jump wanted; with the
    // key on the cursor comes round, which is what the operator who asked for it wanted.
    var stopping = Fixture.pane()
    Focus.step(stopping, -1)
    check("with the key off, up at the top leaves the cursor where it was", stopping.cursorIndex, 0)
    var wrapping = Fixture.pane()
    wrapping.wrapAtEnds = true
    Focus.step(wrapping, -1)
    check("with the key on, up at the top lands on the last row", wrapping.cursorIndex, 6)
    Focus.step(wrapping, 1)
    check("and down from the last row comes back to the first", wrapping.cursorIndex, 0)
    // A page key overshoots on purpose, so only a step taken FROM an end may wrap.
    wrapping.cursorIndex = 3
    Focus.step(wrapping, 20)
    check("a page key from the middle still stops at the end it was heading for", wrapping.cursorIndex, 6)
    // The selection keys keep ui/js/Filter.js moveCursor's plain clamp: an extend that wrapped would
    // run the anchor to the other end of the listing and select every row between the two.
    var extending = Fixture.pane()
    extending.wrapAtEnds = true
    Filter.moveCursor(extending, -1)
    check("moveCursor, which shift+j and shift+k move through, never wraps", extending.cursorIndex, 0)

    // The listing's m goes through the pane, which says whether a delegate was under the cursor; an
    // empty directory and a filter that hides every row both get the sentence rather than silence.
    var listing = listPane(true)
    Focus.act("menu", listing)
    check("m opens the menu under the cursor row, and says nothing over it",
          listing.opened + "|" + listing.said, "1|")
    var bare = listPane(false)
    Focus.act("menu", bare)
    check("m with no row under the cursor says why instead of swallowing the key",
          bare.opened + "|" + bare.said, "1|No row under the cursor to open a menu on.")
}
