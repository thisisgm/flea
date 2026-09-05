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
    var others = [Qt.Key_Up, Qt.Key_Home, Qt.Key_End, Qt.Key_PageUp, Qt.Key_PageDown]
    var handed = []
    var committed = []
    for (var i = 0; i < others.length; i++) {
        var narrowed = queryPane()
        narrowed.filterTyping = true
        narrowed.filterQuery = "scr"
        narrowed.refresh()
        Focus.handleKey(key(others[i], ""), narrowed, noRail())
        handed.push(narrowed.filterTyping + ":" + narrowed.filterQuery)
        var walks = queryPane()
        walks.searchMode = "typing"
        walks.searchQuery = "scr"
        Focus.handleKey(key(others[i], ""), walks, noRail())
        committed.push(walks.searchMode + ":" + walks.walked.length)
    }
    check("every other cursor key hands the filter's caret back, with its query left standing",
          handed.join("|"), "false:scr|false:scr|false:scr|false:scr|false:scr")
    check("and each commits the search's walk exactly once, the way the down arrow does",
          committed.join("|"), "results:1|results:1|results:1|results:1|results:1")
}
