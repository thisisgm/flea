.import "../../ui/js/Focus.js" as Focus
.import "filterfixture.js" as Fixture

// Issue 12's own suite, split out of tests/js/focus.js at its 300-line cap the way focus-forward.js
// was: what the search's and the filter's query lines do with a key while they hold the caret.

function closed() {
    return { active: false, isMedia: false, isPdf: false }
}

function key(code, text) {
    return { key: code, text: text, modifiers: Qt.NoModifier }
}

// A pane with the whole handleKey route wired, because issue 12's defect is that a query line
// swallowed the key: only the real route can show a key reaching the rows or failing to.
function queryPane() {
    var p = Fixture.pane()
    p.focusView = "list"
    p.viewMode = "list"
    p.searchMode = ""
    p.searchQuery = ""
    p.searchFrom = ""
    p.searchHere = false
    p.searchRunning = false
    p.searchScanned = 0
    p.searchCancelled = false
    p.home = ""
    p.path = "/d"
    p.showHidden = false
    p.listingState = "ready"
    p.kindNames = []
    p.trashArmedAt = 0
    p.inputAt = 0
    p.rowsAt = 0
    p.cursorStride = 1
    // Half of this is a page key's step, so pageDown and pageUp move a real distance here.
    p.visibleRows = 4
    p.preview = closed()
    p.shareBrowser = { active: false }
    p.said = ""
    p.walked = []
    p.message = function (text, isError) { p.said = text }
    p.clearSelection = function () { p.picked = {} }
    p.renameEditor = function () { return null }
    p.act = function (action) { Focus.act(action, p) }
    p.backend = { search: function (path, query, hidden) { p.walked.push(path + "?" + query) } }
    return p
}

// Focus.handleKey takes the rail only to ask it for a live rename editor.
function noRail() {
    return { renameEditor: function () { return null } }
}

function run(check) {
    // Issue 12: both query lines owned every key while they had the caret, so an arrow never reached
    // the rows and a listing with more than one match could not be walked from the keyboard.
    var narrowing = queryPane()
    narrowing.filterTyping = true
    narrowing.filterQuery = "scr"
    narrowing.refresh()
    Focus.handleKey(key(Qt.Key_Down, ""), narrowing, noRail())
    check("an arrow on the filter's query line hands the caret back to the rows",
          narrowing.filterTyping, false)
    check("and leaves the filter standing, so the narrowed rows are what is moved through",
          narrowing.filterQuery, "scr")
    check("and moves the cursor on that same press, to the second row the filter kept",
          narrowing.cursorIndex, 1)
    // The line is still a line: every printable character narrows it rather than reaching the rows.
    var typingOn = queryPane()
    typingOn.filterTyping = true
    typingOn.filterQuery = "scr"
    typingOn.refresh()
    Focus.handleKey(key(Qt.Key_J, "j"), typingOn, noRail())
    check("and j is still typed into the query rather than moving the cursor",
          typingOn.filterQuery + "|" + typingOn.filterTyping, "scrj|true")

    // The search's line commits the walk the way enter does, so one press walks once: a walk per
    // keystroke is the sweep the design refused, and it is not what an arrow asks for either.
    var walking = queryPane()
    walking.searchMode = "typing"
    walking.searchQuery = "scr"
    Focus.handleKey(key(Qt.Key_Down, ""), walking, noRail())
    check("an arrow on the search's query line commits the walk and leaves the line",
          walking.searchMode + "|" + walking.walked.join(","), "results|/d?scr")
    var stillTyping = queryPane()
    stillTyping.searchMode = "typing"
    stillTyping.searchQuery = "scr"
    Focus.handleKey(key(Qt.Key_E, "e"), stillTyping, noRail())
    check("and a printable key still extends the query without walking anything",
          stillTyping.searchQuery + "|" + stillTyping.walked.length, "scre|0")

    // Issue 28's four keys leaving the line was claimed in prose and in a comment and driven by
    // nothing, and the up arrow was untested too. ui/SearchStrip.qml draws the query as a Text with
    // the caret pinned after it, so Home, End, PageUp and PageDown have no caret to move on the line
    // and the listing's own meaning is the only one they can carry: ui/js/Focus.js LEAVES_LINE.
    // "scr" keeps rows 0, 1, 5 and 6, so the cursor starts on the third of the four and every one
    // of these five has somewhere different to take it: leaving the line is half the behaviour and
    // the key still reaching the listing is the other half, which nothing here used to assert.
    var others = [Qt.Key_Up, Qt.Key_Home, Qt.Key_End, Qt.Key_PageUp, Qt.Key_PageDown]
    var handed = []
    var landed = []
    var committed = []
    for (var i = 0; i < others.length; i++) {
        var narrowed = queryPane()
        narrowed.filterTyping = true
        narrowed.filterQuery = "scr"
        narrowed.refresh()
        narrowed.cursorIndex = 5
        Focus.handleKey(key(others[i], ""), narrowed, noRail())
        handed.push(narrowed.filterTyping + ":" + narrowed.filterQuery)
        landed.push(narrowed.cursorIndex)
        var walks = queryPane()
        walks.searchMode = "typing"
        walks.searchQuery = "scr"
        Focus.handleKey(key(others[i], ""), walks, noRail())
        committed.push(walks.searchMode + ":" + walks.walked.length)
    }
    check("every other cursor key hands the filter's caret back, with its query left standing",
          handed.join("|"), "false:scr|false:scr|false:scr|false:scr|false:scr")
    check("and each one then means what it means in the listing, off the row it started on",
          landed.join("|"), "1|0|6|0|6")
    check("and each commits the search's walk exactly once, the way the down arrow does",
          committed.join("|"), "results:1|results:1|results:1|results:1|results:1")

    // Issue 30's only control is tab on the search's query line, and it was driven straight into
    // Search.typeKey. Nothing said Focus.handleKey routes Qt.Key_Tab there rather than resolving it
    // through Keymap to focusNext, which is what the very same key means one state away.
    var scoping = queryPane()
    scoping.searchMode = "typing"
    scoping.searchQuery = "scr"
    Focus.handleKey(key(Qt.Key_Tab, "\t"), scoping, noRail())
    check("tab on the search's query line reaches the scope and not the focus switch",
          scoping.searchHere + "|" + scoping.focusView, "true|list")
    var switching = queryPane()
    Focus.handleKey(key(Qt.Key_Tab, "\t"), switching, noRail())
    check("and with no query line up the same key is the focus switch the sheet draws",
          switching.searchHere + "|" + switching.focusView, "false|rail")

    // ui/js/Search.js says the scope is a property of the window: close() and reveal() leave
    // searchHere standing on purpose, so it holds until it is pressed again. That is a claim about
    // state outliving the search that set it, and it was written in a comment and driven by nothing.
    var sticky = queryPane()
    sticky.home = "/d"
    sticky.path = "/d/sub"
    sticky.searchMode = "typing"
    sticky.searchQuery = "scr"
    Focus.handleKey(key(Qt.Key_Tab, "\t"), sticky, noRail())
    Focus.handleKey(key(Qt.Key_Escape, ""), sticky, noRail())
    check("escape off the query line closes the search and leaves the scope where tab put it",
          sticky.searchMode + "|" + sticky.searchHere, "|true")
    sticky.searchMode = "typing"
    sticky.searchQuery = "de"
    Focus.handleKey(key(Qt.Key_Return, ""), sticky, noRail())
    check("so the next search walks the pane's own directory with no second press",
          sticky.walked.join(","), "/d/sub?de")
    // The denominator: without that flip the same fixture walks home, so the check above is not
    // reading a scope the pane would have chosen anyway.
    var wide = queryPane()
    wide.home = "/d"
    wide.path = "/d/sub"
    wide.searchMode = "typing"
    wide.searchQuery = "de"
    Focus.handleKey(key(Qt.Key_Return, ""), wide, noRail())
    check("and a window that never pressed tab still walks the whole home directory",
          wide.walked.join(","), "/d?de")
}
