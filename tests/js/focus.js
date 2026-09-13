.import "../../ui/js/Focus.js" as Focus
.import "../../ui/js/Keymap.js" as Keymap
.import "filterfixture.js" as Fixture

// Focus.lookup is where a key is discarded for being meaningless in the current state, and a wrong
// gate there is silent: the key simply does nothing, and no suite but this one would notice.

function pane(preview, viewMode) {
    return {
        focusView: "list",
        viewMode: viewMode ? viewMode : "list",
        chooseView: function (mode) { this.viewMode = mode },
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

// Only the members Focus.handleKey touches on its way to the terminal chord, and the counter for
// the one call it must make. The button lives in the chrome above both views, so the chord has to
// answer from the rail as well as the list rather than being swallowed by whichever view has focus.
function chromePane(view) {
    return {
        focusView: view, viewMode: "list", searchMode: "", filterTyping: false,
        inputAt: 0, rowsAt: 0, trashArmedAt: 0, asked: 0, copied: 0, said: "", shown: null,
        preview: closed(),
        message: function (text, isError) { this.said = text },
        shareBrowser: { active: false },
        sidebar: { renameEditor: function () { return null } },
        renameEditor: function () { return null },
        // What ui/Pane.qml's own act() does with an action, so a route that reaches the pane's
        // dispatch instead of the interception is visible here rather than throwing.
        act: function (action) { Focus.act(action, this) },
        openTerminal: function () { this.asked += 1 },
        copyDirPath: function () { this.copied += 1 }
    }
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

    var gridPane = Fixture.pane()
    gridPane.viewMode = "grid"
    gridPane.cursorStride = 3
    gridPane.wrapAtEnds = true
    gridPane.cursorIndex = 2
    Focus.act("cursorDown", gridPane)
    check("grid j follows row-major order across a row boundary", gridPane.cursorIndex, 3)
    Focus.act("cursorUp", gridPane)
    check("grid k follows the previous item", gridPane.cursorIndex, 2)
    check("grid j is not a physical arrow", Focus.gridArrow(key(Qt.Key_J, "j", none), "cursorDown", gridPane), false)
    for (var move of [
        [Qt.Key_Right, "cursorRight", 2, 2], [Qt.Key_Left, "cursorLeft", 3, 3],
        [Qt.Key_Down, "cursorDown", 2, 5], [Qt.Key_Down, "cursorDown", 5, 5],
        [Qt.Key_Down, "cursorDown", 3, 6], [Qt.Key_Up, "cursorUp", 6, 3],
        [Qt.Key_Up, "cursorUp", 0, 0], [Qt.Key_Right, "cursorRight", 6, 6]
    ]) {
        gridPane.cursorIndex = move[2]
        Focus.gridArrow(key(move[0], "", none), move[1], gridPane)
        check("grid visual neighbour from " + move[2] + " with " + move[1], gridPane.cursorIndex, move[3])
    }
    gridPane.cursorStride = 2
    gridPane.cursorIndex = 3
    Focus.gridArrow(key(Qt.Key_Down, "", none), "cursorDown", gridPane)
    check("grid arrows use the reflowed column count", gridPane.cursorIndex, 5)

    gridPane.filterQuery = "screen"
    gridPane.refresh()
    gridPane.cursorIndex = 0
    gridPane.cursorStride = 2
    Focus.gridArrow(key(Qt.Key_Down, "", none), "cursorDown", gridPane)
    check("filtered grid arrows address visible cells", gridPane.cursorIndex, 6)
    Focus.gridArrow(key(Qt.Key_Right, "", none), "cursorRight", gridPane)
    check("filtered final row has no right cell", gridPane.cursorIndex, 6)

    // Nothing in keys.toml is bound ahead of its feature now: lookup hands both actions through
    // and handleKey routes each above the views, so neither answers with a sentence any more.
    var colon = key(Qt.Key_Colon, ":", shift)
    check("colon resolves to the path bar", Focus.lookup(colon, pane(closed())), "pathBar")
    var newTab = key(Qt.Key_T, "t", none)
    check("t resolves to a new tab", Focus.lookup(newTab, pane(closed())), "tabNew")

    // GridView includes Filter in its chrome; both supported views narrow their held rows.
    var slash = key(Qt.Key_Slash, "/", none)
    check("slash opens the filter in the list view", Focus.lookup(slash, pane(closed())), "filter")
    check("slash opens the filter in the grid view", Focus.lookup(slash, pane(closed(), "grid")), "filter")
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

    var activeStatus = escaper("needle", 0)
    var statusEscapes = 0
    activeStatus.statusBar = {escapePressed: function () { statusEscapes += 1; return true }}
    activeStatus.searchMode = "results"
    activeStatus.searchRunning = true
    Focus.act("escape", activeStatus)
    check("filter consumes Escape before search or transfer", activeStatus.filterQuery + "|" + activeStatus.cancelled + "|" + statusEscapes, "|0|0")
    Focus.act("escape", activeStatus)
    check("focused search consumes Escape before transfer", activeStatus.cancelled + "|" + statusEscapes, "1|0")
    activeStatus.searchMode = ""
    Focus.act("escape", activeStatus)
    check("status consumes Escape before marks", statusEscapes + "|" + activeStatus.retreated, "1|0")
    activeStatus.statusBar.escapePressed = function () { return false }
    Focus.act("escape", activeStatus)
    check("idle status lets Escape clear marks", activeStatus.retreated, 1)

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

    // Ctrl+K opens the dialog from either view, and so does the bare a: keys.toml promised "either the
    // list or the rail" from the first commit while Focus.js made it rail-only (GM, 2026-09-11).
    // Ctrl+K is the Mac preset's chord, so the preset is named rather than assumed.
    var ctrl = Qt.ControlModifier
    Keymap.setPreset("mac")
    check("ctrl k connects to a server from the list", Focus.lookup(key(Qt.Key_K, "\u000b", ctrl), pane(closed())), "addNetwork")
    check("Mac List Right still opens the cursor", Focus.lookup(right, pane(closed())), "open")
    check("Mac List Left still opens the parent", Focus.lookup(left, pane(closed())), "parent")
    for (var preset of Keymap.PRESETS) {
        Keymap.setPreset(preset)
        check(preset + " Grid Left moves between tiles", Focus.lookup(left, pane(closed(), "grid")), "cursorLeft")
        check(preset + " Grid Right moves between tiles", Focus.lookup(right, pane(closed(), "grid")), "cursorRight")
        check(preset + " Grid PDF Right keeps page navigation", Focus.lookup(right, pane(pdfOpen(), "grid")), "seekForward")
    }
    Keymap.setPreset("default")
    check("bare a adds a network place from the list too", Focus.lookup(key(Qt.Key_A, "a", none), pane(closed())), "addNetwork")
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
    var stick = { label: "128GB", group: "device", kind: "volume", device: "/dev/sda1", path: "/run/media/user/128GB", mounted: true, removable: true }
    var inside = ejectPane("list", "/run/media/user/128GB/photos", [home, stick], 0)
    Focus.act("eject", inside)
    check("ctrl e in a listing inside the volume ejects that volume, whatever the rail cursor is on",
          inside.sidebar.released.join(",") + "|" + inside.said, "eject:/dev/sda1|")
    var outside = ejectPane("list", "/home/user/Documents", [home, stick], 1)
    Focus.act("eject", outside)
    check("ctrl e in a listing on the internal disk says so, even with the rail cursor on the stick",
          outside.sidebar.released.length + "|" + outside.said,
          "0|This is not inside a removable volume.")

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

    // PR 34's chord. The context-menu row and Ctrl+T raise the same terminal, and the rail owns its
    // own keys, so the one route both views share is the interception in handleKey above the views.
    var terminalKey = key(Qt.Key_T, "\u0014", ctrl)
    var fromList = chromePane("list")
    check("ctrl t is consumed in the list", Focus.handleKey(terminalKey, fromList, fromList.sidebar), true)
    check("and opens a terminal there", fromList.asked, 1)
    var fromRail = chromePane("rail")
    Focus.handleKey(terminalKey, fromRail, fromRail.sidebar)
    check("ctrl t opens one from the rail as well", fromRail.asked, 1)
    // The menu row's own route: ui/ContextMenu.qml fires the action into ui/Pane.qml's act(), which
    // never sees handleKey's interception, and this is the dispatch that was missing when it did not.
    var fromMenu = chromePane("list")
    Focus.act("openTerminal", fromMenu)
    check("the menu row reaches the same terminal through act", fromMenu.asked + "|" + fromMenu.said, "1|")

    var menu = listPane(true)
    var menuRequests = []
    menu.path = "/d"
    menu.cursorIndex = 0
    menu.rowFor = function () { return {n: "selected.txt"} }
    menu.selectedIndices = function () { return [] }
    menu.join = function (parent, name) { return parent + "/" + name }
    menu.sticky = function () {}
    menu.backend = {
        duplicate: function (path, id) { menuRequests.push("duplicate:" + id) },
        trash: function (rows, id) { menuRequests.push("trash:" + id) },
        extract: function (path, dest, id) { menuRequests.push("extract:" + id) },
        compress: function (paths, dest, format, id) { menuRequests.push(paths.join(",") + ":" + id) }
    }
    menu.moveToDropbox = function (id) { menuRequests.push("dropbox:" + id) }
    menu.openConvert = function (id) { menuRequests.push("convert:" + id) }
    var selectedPaths = ["/d/captured.txt", "/d/second.txt"]
    Focus.act("copy", menu, 42, selectedPaths)
    check("menu dispatch copies the captured paths without another asynchronous lookup",
          menu.clipboard.paths.join(",") + "|" + menu.clipboard.moving, "/d/captured.txt,/d/second.txt|false")
    Focus.act("cut", menu, 42, selectedPaths)
    check("menu cut keeps the captured paths and move intent", menu.clipboard.moving, true)
    for (var action of ["duplicate", "trash", "extract", "dropbox", "convert", "compress:zip"])
        Focus.act(action, menu, 42, selectedPaths)
    check("menu dispatch keeps the identity through each mutation consumer",
          menuRequests.join("|"),
          "duplicate:42|trash:42|extract:42|dropbox:42|convert:42|/d/captured.txt,/d/second.txt:42")

    // Y copies root.path, the same thing Ctrl+T opens a terminal on, so it answers from the rail
    // too; without the interception RailKeys.act ate it and the key did nothing and said nothing.
    var copyKey = key(Qt.Key_Y, "Y", shift)
    var copyList = chromePane("list")
    check("Y is consumed in the list", Focus.handleKey(copyKey, copyList, copyList.sidebar), true)
    check("and copies the folder path there", copyList.copied, 1)
    var copyRail = chromePane("rail")
    Focus.handleKey(copyKey, copyRail, copyRail.sidebar)
    check("Y copies the folder path from the rail as well", copyRail.copied, 1)

    var shareOwner = chromePane("list")
    var otherPane = chromePane("list")
    var sharedBrowser = {active: true, owner: shareOwner}
    shareOwner.shareBrowser = sharedBrowser
    otherPane.shareBrowser = sharedBrowser
    check("a share listing belongs to the pane that requested it", Focus.shareBrowserHere(shareOwner), true)
    check("another pane keeps its own key context while the shared listing is open", Focus.shareBrowserHere(otherPane), false)
    sharedBrowser.active = false
    check("closing the share listing releases its owner's keys", Focus.shareBrowserHere(shareOwner), false)
}
