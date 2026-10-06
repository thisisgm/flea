.import "../../ui/js/Focus.js" as Focus
.import "../../ui/js/Clipboard.js" as Clipboard
.import "../../ui/js/Eject.js" as Eject
.import "../../ui/js/Keymap.js" as Keymap
.import "../../ui/js/Selection.js" as Selection
.import "filterfixture.js" as Fixture
.import "sourcefixture.js" as Source

// Focus.lookup is where a key is discarded for being meaningless in the current state, and a wrong
// gate there is silent: the key simply does nothing, and no suite but this one would notice.

function pane(preview, viewMode) {
    return {
        focusView: "list", shown: null,
        viewMode: viewMode ? viewMode : "list",
        chooseView: function (mode) { this.viewMode = mode },
        searchMode: "",
        recentMode: "",
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
    p.isError = false
    p.openCursorMenu = function () { p.opened += 1; return hasRow }
    p.message = function (text, isError) { p.said = text; p.isError = isError }
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
        focusView: view, viewMode: "list", searchMode: "", recentMode: "", filterTyping: false,
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

// armHandle builds the handleKey members past the escape dispatch: listing view, recording dispatch.
function armHandle(p) {
    p.focusView = "list"
    p.viewMode = "list"
    p.shown = null
    p.renameEditor = function () { return null }
    p.keySequence = ""
    p.keySequenceIdentity = ""
    p.trashArmedAt = 0
    p.message = function (text) { p.said = text }
    p.acted = []
    p.act = function (action) { p.acted.push(action); Focus.act(action, p) }
    return p
}
// Escape-case members only: Search.cancel and Pane.escapePressed record rather than act to pin reach order.
function escaper(query, retreated) {
    var p = pane(closed())
    p.filterQuery = query
    p.filterTyping = false
    p.retreated = retreated
    p.cancelled = 0
    p.searchRunning = false
    p.backend = { searchcancel: function () { p.cancelled += 1 } }
    p.escapePressed = function () { p.retreated += 1 }
    // Issue 29's own members: the setting, the menu, the card, the marks and the listing in flight.
    p.escapeUp = false
    p.menuVisible = false
    p.collide = { opened: false }
    p.listInFlight = false
    p.climbed = 0
    p.openParent = function () { p.climbed += 1 }
    p.selection = { count: function () { return 0 } }
    p.selectionCount = function () { return 0 }
    p.wire = { anchor: null, reloaded: 0 }
    return p
}

function run(check) {
    check("DEL does not mask a cursor key as printable", Focus.leavesLine(key(Qt.Key_Down, "\u007f", Qt.NoModifier)), true)
    check("a normal letter stays on the query line", Focus.leavesLine(key(Qt.Key_J, "j", Qt.NoModifier)), false)
    var printablePane = chromePane("list")
    Focus.handleKey(key(Qt.Key_unknown, "\u007f", Qt.NoModifier), printablePane, printablePane.sidebar)
    check("DEL produces no type-ahead hint", printablePane.said, "")
    Focus.handleKey(key(Qt.Key_unknown, "é", Qt.NoModifier), printablePane, printablePane.sidebar)
    check("a normal unbound letter still produces the filter hint", printablePane.said, "Press / to filter this listing by name.")
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

    // Left and Right serve two previews, the grid's own sideways step, and GM's fix: while browsing they are the letter pair's spelling, so they go up a level and into the row under the cursor.
    check("left turns a PDF page", Focus.lookup(left, pane(pdfOpen())), "seekBack")
    check("right turns a PDF page", Focus.lookup(right, pane(pdfOpen())), "seekForward")
    check("left still seeks media", Focus.lookup(left, pane(mediaOpen())), "seekBack")
    check("left goes up a level in the list", Focus.lookup(left, pane(closed())), "parent")
    check("left still steps a grid tile", Focus.lookup(left, pane(closed(), "grid")), "cursorLeft")
    check("right still steps a grid tile", Focus.lookup(right, pane(closed(), "grid")), "cursorRight")
    // Issue 114, muellan: the letters the presets spell the arrows with mean the arrows in the grid.
    var hKey = key(Qt.Key_H, "h", none)
    var lKey = key(Qt.Key_L, "l", none)
    check("h steps a grid tile rather than climbing", Focus.lookup(hKey, pane(closed(), "grid")), "cursorLeft")
    check("l steps a grid tile rather than browsing in", Focus.lookup(lKey, pane(closed(), "grid")), "cursorRight")
    // Only h is read back in the list: l's answer there depends on the row under the cursor.
    check("and in the list h is still the tree's own", Focus.lookup(hKey, pane(closed())), "parent")

    // Nothing in keys.toml is bound ahead of its feature now: lookup hands both actions through
    // and handleKey routes each above the views, so neither answers with a sentence any more.
    var colon = key(Qt.Key_Colon, ":", shift)
    check("colon resolves to the path bar", Focus.lookup(colon, pane(closed())), "pathBar")
    var newTab = key(Qt.Key_T, "t", none)
    check("t resolves to a new tab", Focus.lookup(newTab, pane(closed())), "tabNew")

    // GridView includes Filter in its chrome; every view narrows its held rows.
    var slash = key(Qt.Key_Slash, "/", none)
    check("slash opens the filter in the list view", Focus.lookup(slash, pane(closed())), "filter")
    check("slash opens the filter in the grid view", Focus.lookup(slash, pane(closed(), "grid")), "filter")
    check("slash opens the filter in the columns view", Focus.lookup(slash, pane(closed(), "columns")), "filter")
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

    // A hop out of Recent while a listing is out is refused before the mode is touched.
    var held = escaper("", 0)
    held.focusView = "list"
    held.recentMode = "results"
    held.recentFrom = "/home/gm/Work"
    held.recentPaths = ["/home/gm/a.txt"]
    held.listInFlight = true
    held.said = ""
    held.message = function (text) { held.said = text }
    held.opened = []
    held.openWithoutHistory = function (next) { held.opened.push(next) }
    held.backend = { sortBy: "name", sortDesc: false }
    Focus.act("escape", held)
    check("Escape refuses the hop out of Recent while a listing is out", held.said, "A directory is already loading.")
    check("and keeps the mode standing", held.recentMode + "|" + held.recentFrom, "results|/home/gm/Work")
    check("and asks for nothing", held.opened.length, 0)
    var stuck = escaper("", 0)
    stuck.recentMode = "results"
    stuck.listInFlight = true
    stuck.path = "/"
    stuck.held = 0
    stuck.cursorIndex = 0
    stuck.rows = [{ n: "home/gm/Docs/a.txt", d: false }]
    stuck.rowFor = function (i) { return stuck.rows[i] || null }
    stuck.join = function (base, name) { return base === "/" ? "/" + name : base + "/" + name }
    stuck.said = ""
    stuck.message = function (text) { stuck.said = text }
    stuck.opened = []
    stuck.openWithoutHistory = function (next) { stuck.opened.push(next) }
    Focus.act("reveal", stuck)
    check("o refuses the reveal while a listing is out", stuck.said, "A directory is already loading.")
    check("and keeps the mode standing", stuck.recentMode, "results")
    check("and asks for nothing", stuck.opened.length, 0)

    // Pane.qml's own Back, Up and re-list entries refuse the same hop before touching the mode.
    var backLine = Source.slice(Source.source("ui/Pane.qml"), "function goBack()", "function goForward()")
    check("goBack refuses before it closes", backLine.indexOf("root.listInFlight") >= 0, true)
    var upLine = Source.slice(Source.source("ui/Pane.qml"), "function openParent()", "function openRecent(paths, visits)")
    check("openParent refuses before it closes", upLine.indexOf("root.listInFlight") >= 0, true)
    var relist = Source.slice(Source.source("ui/Pane.qml"), "function openWithoutHistory(newPath, options)", "function toggleHidden()")
    check("a re-list refuses before it clears",
          relist.indexOf("if (root.listInFlight)") >= 0 && relist.indexOf("if (root.listInFlight)") < relist.indexOf("RecentMode.leave(root)"), true)

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

    // Issue 29: Escape climbs while the setting is on, and stays put while off, which is 0.3.4.
    var stays = escaper("", 0)
    Focus.act("escape", stays)
    check("Escape with the setting off clears marks instead of climbing",
          stays.retreated + "|" + stays.climbed, "1|0")
    var climbs = escaper("", 0)
    climbs.escapeUp = true
    Focus.act("escape", climbs)
    check("Escape with the setting on climbs to the parent", climbs.climbed + "|" + climbs.retreated, "1|0")
    var filtered = escaper("scr", 0)
    filtered.escapeUp = true
    Focus.act("escape", filtered)
    check("a standing filter still goes first", filtered.filterQuery + "|" + filtered.climbed, "|0")
    var marked = escaper("", 0)
    marked.escapeUp = true
    marked.selectionCount = function () { return 2 }
    Focus.act("escape", marked)
    check("a selection is cleared before any climb", marked.retreated + "|" + marked.climbed, "1|0")
    var menued = escaper("", 0)
    menued.escapeUp = true
    menued.menuVisible = true
    Focus.act("escape", menued)
    check("an open menu keeps the key", menued.climbed + "|" + menued.retreated, "0|1")
    var loading = escaper("", 0)
    loading.escapeUp = true
    loading.listInFlight = true
    Focus.act("escape", loading)
    check("a listing out keeps the key too", loading.climbed + "|" + loading.retreated, "0|1")
    var carded = escaper("", 0)
    carded.escapeUp = true
    carded.collide = { opened: true }
    Focus.act("escape", carded)
    check("and so does the collision card", carded.climbed + "|" + carded.retreated, "0|1")

    // A climb landing mark is Flea's, so it never blocks the next climb.
    var landed = escaper("", 0)
    landed.escapeUp = true
    landed.selection = Selection.create()
    landed.selection.only(4, true)
    landed.selectionCount = function () { return landed.selection.count() }
    Focus.act("escape", landed)
    check("a climb's landing mark never blocks the next climb", landed.climbed + "|" + landed.retreated, "1|0")
    var deliberate = escaper("", 0)
    deliberate.escapeUp = true
    deliberate.selection = Selection.create()
    deliberate.selection.toggle(4)
    deliberate.selectionCount = function () { return deliberate.selection.count() }
    Focus.act("escape", deliberate)
    check("a deliberate lone mark still unwinds first", deliberate.climbed + "|" + deliberate.retreated, "0|1")
    var clicked = escaper("", 0)
    clicked.escapeUp = true
    clicked.selection = Selection.create()
    clicked.selection.only(4)
    clicked.selectionCount = function () { return clicked.selection.count() }
    Focus.act("escape", clicked)
    check("a plain click's lone mark still unwinds first", clicked.climbed + "|" + clicked.retreated, "0|1")

    // Escape cancels an armed trash or vim pair and stops instead of climbing.
    var armedKey = key(Qt.Key_Escape, "", none)
    var armedTrash = escaper("", 0)
    armedTrash.escapeUp = true
    armHandle(armedTrash)
    armedTrash.trashArmedAt = Date.now()
    check("Escape with an armed trash is consumed", Focus.handleKey(armedKey, armedTrash, null), true)
    check("and cancels the arm instead of climbing",
          armedTrash.trashArmedAt + "|" + armedTrash.climbed + "|" + armedTrash.retreated, "0|0|0")
    var armedOff = escaper("", 0)
    armHandle(armedOff)
    armedOff.trashArmedAt = Date.now()
    check("Escape with an armed trash and the setting off is consumed", Focus.handleKey(armedKey, armedOff, null), true)
    check("and retreats instead of standing in for a climb",
          armedOff.trashArmedAt + "|" + armedOff.climbed + "|" + armedOff.retreated, "0|0|1")
    var armedPair = escaper("", 0)
    armedPair.escapeUp = true
    armHandle(armedPair)
    armedPair.path = "/d"
    armedPair.cursorIndex = 3
    armedPair.selectionVersion = 1
    armedPair.keySequence = "copyArm"
    armedPair.keySequenceIdentity = '["/d",3,1,"list"]'
    check("Escape with an armed vim pair is consumed too", Focus.handleKey(armedKey, armedPair, null), true)
    check("and drops the pair instead of climbing",
          armedPair.keySequence + "|" + armedPair.climbed + "|" + armedPair.retreated, "|0|0")
    var armedPairOff = escaper("", 0)
    armHandle(armedPairOff)
    armedPairOff.path = "/d"
    armedPairOff.cursorIndex = 3
    armedPairOff.selectionVersion = 1
    armedPairOff.keySequence = "copyArm"
    armedPairOff.keySequenceIdentity = '["/d",3,1,"list"]'
    check("the off pair holds a live identity", armedPairOff.keySequenceIdentity, Focus.stampOf(armedPairOff))
    check("Escape with an armed vim pair and the setting off is consumed too", Focus.handleKey(armedKey, armedPairOff, null), true)
    check("and blanks the pair before running Escape's own action",
          armedPairOff.keySequence + "|" + armedPairOff.climbed + "|" + armedPairOff.retreated, "|0|1")
    var stalePair = escaper("", 0)
    stalePair.escapeUp = true
    armHandle(stalePair)
    stalePair.path = "/d"
    stalePair.cursorIndex = 7
    stalePair.selectionVersion = 1
    stalePair.keySequence = "copyArm"
    stalePair.keySequenceIdentity = '["/d",3,1,"list"]'
    check("Escape with a stale vim pair climbs instead", Focus.handleKey(armedKey, stalePair, null), true)
    check("and leaves the climb to the parent",
          stalePair.keySequence + "|" + stalePair.climbed + "|" + stalePair.retreated, "|1|0")
    var unarmed = escaper("", 0)
    unarmed.escapeUp = true
    armHandle(unarmed)
    check("an unarmed Escape still climbs", Focus.handleKey(armedKey, unarmed, null), true)
    check("through the same climb", unarmed.climbed + "|" + unarmed.retreated, "1|0")

    // Recent is a history, so Paste as and its leaves are refused like Paste.
    var historyPaste = escaper("", 0)
    historyPaste.recentMode = "results"
    historyPaste.message = function (text) { historyPaste.said = text }
    historyPaste.openPasteAs = function () { historyPaste.pasted = true }
    historyPaste.pasteLink = function () { historyPaste.pasted = true }
    historyPaste.pasted = false
    Focus.act("pasteAs", historyPaste)
    check("Paste as is refused in Recent", historyPaste.said + "|" + historyPaste.pasted,
          "This listing is a history, and cannot take a paste.|false")
    Focus.act("pasteLink", historyPaste)
    check("and its relative leaf is refused there too", historyPaste.said + "|" + historyPaste.pasted,
          "This listing is a history, and cannot take a paste.|false")
    Focus.act("pasteAbsoluteLink", historyPaste)
    Focus.act("pasteHardLink", historyPaste)
    check("and so are the absolute and hard leaves", historyPaste.said + "|" + historyPaste.pasted,
          "This listing is a history, and cannot take a paste.|false")

    // F5 and Ctrl+R re-list the folder they are on through the reload key.
    var f5 = key(Qt.Key_F5, "", none)
    check("F5 resolves to reload while browsing", Focus.lookup(f5, pane(closed())), "reload")
    var ctrlR = key(Qt.Key_R, "\u0012", Qt.ControlModifier)
    check("ctrl r resolves to reload too", Focus.lookup(ctrlR, pane(closed())), "reload")
    check("reload goes quiet over search results, the way the sort keys do",
          Focus.lookup(f5, searching(closed())), "")
    var reloading = escaper("", 0)
    reloading.total = 10
    reloading.held = 0
    reloading.windowSize = 40
    reloading.path = "/d"
    reloading.cursorIndex = 0
    reloading.rowFor = function () { return null }
    reloading.openWithoutHistory = function (path) { reloading.listed = path }
    reloading.backend.window = function () {}
    reloading.wire = { anchor: null }
    Focus.act("reload", reloading)
    check("reload re-lists the folder it is on", reloading.listed, "/d")
    check("and remembers the count its notice answers against", reloading.reloadFrom, 10)

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
    // Issue 133: the key follows the menu row, so Delete on an SMB share asks gio for no trash it can only refuse.
    var onShare = pane(closed())
    onShare.path = "/run/user/1000/gvfs/smb-share:server=192.168.21.25,share=data"
    check("Delete in an SMB share folder names the refusal instead of silence",
          Focus.lookup(key(Qt.Key_Delete, "", none), onShare), "trashRefused")
    check("and so does the first d of the pair", Focus.lookup(key(Qt.Key_D, "d", none), onShare), "trashRefused")
    var refused = listPane(true)
    refused.path = "/run/user/1000/gvfs/smb-share:server=192.168.21.25,share=data"
    Focus.act("trashRefused", refused)
    check("and the refusal names the place with the key that still removes the rows",
          refused.said, Focus.noTrashLine())
    check("and it takes the error role with its hint from the live keymap",
          refused.isError + "|" + Focus.noTrashHint(),
          true + "|" + Keymap.hintFor("deletePermanently") + " deletes")
    // A preset with no deletePermanently row leaves hintFor empty, so the refusal reads bare rather than dangling a separator.
    var keepHint = Keymap.hintFor
    Keymap.hintFor = function (action) { return action === "deletePermanently" ? "" : keepHint(action) }
    check("with no key for deletePermanently the refusal reads bare", Focus.noTrashLine(), Focus.NO_TRASH)
    check("and its hint is empty rather than a bare verb", Focus.noTrashHint(), "")
    Keymap.hintFor = keepHint
    var onDisk = pane(closed())
    onDisk.path = "/home/gm/Downloads"
    check("while Delete in a local folder still does", Focus.lookup(key(Qt.Key_Delete, "", none), onDisk), "trash")
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
    // RailAdditions rule 1's switch builds this row, so it carries the board's own menu
    // (Open, Unmount, Eject) rather than the single Eject 0.2.1 drew: without volumeMenu the
    // release below cannot tell rows[0] from the release.
    var stick = { label: "128GB", group: "device", kind: "volume", device: "/dev/sda1", path: "/run/media/user/128GB", mounted: true, removable: true, volumeMenu: true }
    var inside = ejectPane("list", "/run/media/user/128GB/photos", [home, stick], 0)
    Focus.act("eject", inside)
    check("ctrl e in a listing inside the volume ejects that volume, whatever the rail cursor is on",
          inside.sidebar.released.join(",") + "|" + inside.said, "eject:/dev/sda1|")
    var outside = ejectPane("list", "/home/user/Documents", [home, stick], 1)
    Focus.act("eject", outside)
    check("ctrl e in a listing on the internal disk says so, even with the rail cursor on the stick",
          outside.sidebar.released.length + "|" + outside.said,
          "0|This is not inside a removable volume.")

    // Ctrl+E with the rail hidden: the Loader unloads the Sidebar, so root.sidebar is null and
    // there are no entries to resolve against. The key hands to the pane's transient one-shot
    // flow instead of throwing on the missing rail.
    var hidden = listPane(true)
    hidden.path = "/run/media/user/128GB/photos"
    hidden.sidebar = null
    hidden.hiddenAsked = []
    hidden.ejectHidden = function () { hidden.hiddenAsked.push(hidden.path) }
    var hiddenThrew = ""
    try { Focus.act("eject", hidden) } catch (e) { hiddenThrew = String(e) }
    check("ctrl e with the rail hidden takes the hidden-rail release instead of throwing",
          hiddenThrew + "|" + hidden.hiddenAsked.join(","), "|/run/media/user/128GB/photos")

    // The transient host's completion runs the same release with device-only entries: network
    // shares and phones carry no path and favourites offer no release, so only a volume resolves.
    var adapter = { entries: [stick], deviceEntries: [stick], networkEntries: [],
                    released: [],
                    releaseChosen: function (action, key) { this.released.push(action + ":" + key) } }
    var hiddenInside = listPane(true)
    hiddenInside.path = "/run/media/user/128GB/photos"
    Eject.release(hiddenInside, adapter, false)
    check("the hidden-rail completion ejects the holding volume through releaseChosen",
          adapter.released.join(",") + "|" + hiddenInside.said, "eject:/dev/sda1|")
    var hiddenOutside = listPane(true)
    hiddenOutside.path = "/home/user/Documents"
    var noRail = { entries: [], deviceEntries: [], networkEntries: [],
                   releaseChosen: function (action, key) {} }
    Eject.release(hiddenOutside, noRail, false)
    check("the hidden-rail completion outside any volume says so instead of releasing",
          hiddenOutside.said, "This is not inside a removable volume.")

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
    menu.clipPending = null
    menu.clipQueue = []
    menu.clipSequence = 0
    menu.clipboardState = Clipboard.state()
    menu.clipboardWatchFailed = false
    menu.listInFlight = false
    menu.cursorIndex = 0
    menu.rowFor = function () { return {n: "selected.txt"} }
    menu.selectedIndices = function () { return [] }
    menu.join = function (parent, name) { return parent + "/" + name }
    menu.sticky = function () {}
    menu.backend = {
        heldListing: 1,
        send: function (request) { menu.clipRequests.push(request) },
        duplicate: function (path, id) { menuRequests.push("duplicate:" + id) },
        trash: function (rows, id) { menuRequests.push("trash:" + id) },
        extract: function (path, dest, id) { menuRequests.push("extract:" + id) },
        compress: function (paths, dest, format, id) { menuRequests.push(paths.join(",") + ":" + id) }
    }
    menu.moveToDropbox = function (id) { menuRequests.push("dropbox:" + id) }
    menu.clipRequests = []
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

    // Ctrl+B answers from the rail too, so hiding a focused rail hands the keyboard to the listing.
    var hiding = chromePane("rail")
    hiding.railHidden = false
    hiding.listArea = { focused: false, forceActiveFocus: function () { this.focused = true } }
    hiding.toggleRail = function () { this.railHidden = !this.railHidden }
    var ctrlB = key(Qt.Key_B, "", ctrl)
    check("ctrl b resolves to the sidebar from the rail", Focus.lookup(ctrlB, hiding), "sidebar")
    check("ctrl b from the rail is consumed", Focus.handleKey(ctrlB, hiding, hiding.sidebar), true)
    check("and it hides the rail", hiding.railHidden, true)
    check("hiding a focused rail is a function the pane can call", typeof Focus.railHidden, "function")
    if (typeof Focus.railHidden === "function") Focus.railHidden(hiding)
    check("hiding a focused rail moves the logical view to the list", hiding.focusView, "list")
    check("and the actual focus follows it", hiding.listArea.focused, true)
    var j = key(Qt.Key_J, "j", none)
    check("j then resolves in the list", Focus.lookup(j, hiding), "cursorDown")
    check("the next chord is consumed too", Focus.handleKey(ctrlB, hiding, hiding.sidebar), true)
    check("and it shows the rail again", hiding.railHidden, false)
    var settled = chromePane("list")
    settled.listArea = { focused: false, forceActiveFocus: function () { this.focused = true } }
    if (typeof Focus.railHidden === "function") Focus.railHidden(settled)
    check("a hide with the list focused moves nothing", settled.focusView + "|" + settled.listArea.focused, "list|false")
    // Wire pin, not execution: the helper checks above prove the behavior, this proves Pane calls it.
    var paneSrc = Source.source("ui/Pane.qml")
    check("wire pin: Pane forwards railHidden changes to Focus.railHidden", paneSrc.indexOf("onRailHiddenChanged: if (root.railHidden) Focus.railHidden(root)") >= 0, true)
    check("wire pin: Pane imports the Focus library it forwards through", paneSrc.indexOf('import "js/Focus.js" as Focus') >= 0, true)
    check("tab with a hidden rail stays in the list", Focus.next("list", false), "list")
    check("tab with an auto-hide rail reveals it", Focus.next("list", true), "rail")

    // Both panes share the hidden state, so only the active pane answers a hide with its own listing.
    for (var mode of ["list", "grid", "columns"]) {
        var acting = chromePane("rail")
        acting.viewMode = mode
        acting.paneFocused = true
        acting.listArea = { focused: false, forceActiveFocus: function () { this.focused = true } }
        Focus.railHidden(acting)
        check("an active " + mode + " pane takes its own listing on a hide", acting.focusView + "|" + acting.listArea.focused, "list|true")
    }
    var idle = chromePane("rail")
    idle.paneFocused = false
    idle.listArea = { focused: false, forceActiveFocus: function () { this.focused = true } }
    Focus.railHidden(idle)
    check("an inactive pane keeps its rail view on a hide", idle.focusView, "rail")
    check("and takes no actual focus for it", idle.listArea.focused, false)
    var calm = chromePane("list")
    calm.paneFocused = true
    calm.listArea = { focused: false, forceActiveFocus: function () { this.focused = true } }
    Focus.railHidden(calm)
    check("an already-list pane moves nothing on a hide", calm.focusView + "|" + calm.listArea.focused, "list|false")

    // Recent is a history, not a directory: pasting or creating there would land in the root it stands on.
    function recentPane() {
        var p = listPane(true)
        p.recentMode = "results"
        p.path = "/"
        p.clipboard = { paths: ["/a.txt"], moving: false }
        p.collide = { asked: [], ask: function (req) { this.asked.push(req.op || req.c) } }
        p.made = []
        p.backend = { mkdir: function (path) { p.made.push(path) } }
        return p
    }
    var recentPaste = recentPane()
    Focus.act("paste", recentPaste)
    check("paste over Recent says the history line", recentPaste.said, "This listing is a history, and cannot take a paste.")
    check("and reaches neither Ops nor collide", recentPaste.collide.asked.length + "|" + recentPaste.made.length, "0|0")
    var recentMove = recentPane()
    Focus.act("movePaste", recentMove)
    check("move-paste over Recent says the history line", recentMove.said, "This listing is a history, and cannot take a paste.")
    check("and asks no transfer", recentMove.collide.asked.length, 0)
    var recentFolder = recentPane()
    Focus.act("newFolder", recentFolder)
    check("new folder over Recent says the history line", recentFolder.said, "This listing is a history, and cannot take a new folder.")
    check("and asks the backend for nothing", recentFolder.made.length, 0)
    var recentReveal = recentPane()
    recentReveal.cursorIndex = 0
    recentReveal.rowFor = function () { return { n: "home/gm/Work/notes.txt" } }
    recentReveal.join = function (base, name) { return base + name }
    recentReveal.opened = ""
    recentReveal.openWithoutHistory = function (path) { recentReveal.opened = path }
    Focus.act("reveal", recentReveal)
    check("o over Recent reveals the holding folder", recentReveal.opened + "|" + recentReveal.recentMode, "/home/gm/Work|")

    // MenuAdditions040: one dispatch check each, so a key that loses its route goes red here.
    var copyAsPane = listPane(true)
    copyAsPane.openedCopyAs = 0
    copyAsPane.openCopyAs = function () { copyAsPane.openedCopyAs += 1 }
    Focus.act("copyAs", copyAsPane)
    check("c opens Copy as at the cursor", copyAsPane.openedCopyAs, 1)
    var pasteAsPane = listPane(true)
    pasteAsPane.openedPasteAs = 0
    pasteAsPane.openPasteAs = function () { pasteAsPane.openedPasteAs += 1 }
    Focus.act("pasteAs", pasteAsPane)
    check("P opens Paste as", pasteAsPane.openedPasteAs, 1)
    var copyPathPane = listPane(true)
    copyPathPane.copied = ""
    copyPathPane.opener = { copyText: function (text) { copyPathPane.copied = text } }
    Focus.act("copyPath", copyPathPane, 0, ["/d/a.txt"])
    check("copy path copies at once", copyPathPane.copied, "/d/a.txt")
    var pasteLinkPane = listPane(true)
    pasteLinkPane.linked = []
    pasteLinkPane.pasteLink = function (kind) { pasteLinkPane.linked.push(kind + ":" + arguments.length) }
    Focus.act("pasteLink", pasteLinkPane, 0, ["/d/a.txt"])
    check("paste link asks for a relative link without captured row paths", pasteLinkPane.linked.join("|"), "relative:1")
}
