.import "../../ui/js/Search.js" as Search

function run(check) {
    // Rule 6: a count that is still growing says so, beside the count.
    check("a running walk with matches pairs the count with the state", Search.note(9, true, false), "9 found · still scanning")
    check("and a finished one is the count alone", Search.note(9, false, false), "9 found")
    check("and a four-figure find groups", Search.note(1204, true, false), "1,204 found · still scanning")
    check("a running walk with nothing yet says it is working", Search.note(0, true, false), "searching")
    check("a finished walk with nothing says done", Search.note(0, false, false), "done")
    check("a cancelled walk with nothing says stopped", Search.note(0, false, true), "stopped")

    // V7: the SearchFilter board's own status cell, the count beside the state that changes it.
    check("a running walk pairs what it found with what it scanned",
          Search.statusLine(true, 12, 4120, 300), "12 found · Searching, 4,120 scanned")
    check("a finished walk reports what it found and how long it took",
          Search.statusLine(false, 1, 18204, 412), "1 found in 0.4 s")
    check("and nothing found still says so", Search.statusLine(false, 0, 18204, 412), "0 found in 0.4 s")


    check("the home prefix reads as a tilde", Search.scope("/home/gm/Work/claude/flea", "/home/gm"), "~/Work/claude/flea")
    check("home itself is the bare tilde", Search.scope("/home/gm", "/home/gm"), "~")
    check("a path outside home keeps its own form", Search.scope("/usr/share", "/home/gm"), "/usr/share")

    // The scope rule: the backend walks whatever path this picks, so this is the whole of
    // "universal search across the home folder" and the only place the policy lives.
    check("a search anywhere under home walks the whole home directory",
          Search.scopeRoot("/home/u/Downloads/deep", "/home/u"), "/home/u")
    check("home itself is already the scope", Search.scopeRoot("/home/u", "/home/u"), "/home/u")
    check("a mount outside home searches the mount, never home instead",
          Search.scopeRoot("/run/media/u/NAS/photos", "/home/u"), "/run/media/u/NAS/photos")
    check("a sibling whose name merely starts with home's is outside it",
          Search.scopeRoot("/home/under", "/home/u"), "/home/under")
    check("with no home in the environment the pane searches where it stands",
          Search.scopeRoot("/d", ""), "/d")

    // The strip's own right edge, which the board draws as the pair esc really is from here.
    check("the strip says esc stops the walk and then leaves", Search.wayOut(true), "esc cancels, then returns")
    check("and once the walk is done esc only leaves", Search.wayOut(false), "esc returns")


    // The pointer's own contract: a double click on a result reveals it, and opens a row anywhere
    // else. ui/js/Tap.js asks this rather than deciding it, so the rule lives beside the reveal.
    check("a double click on a search result takes the operator to the file",
          Search.activateAction({ searchMode: "results" }), "reveal")
    check("a double click on an ordinary row still opens it",
          Search.activateAction({ searchMode: "" }), "open")
    check("a double click while the query line is still up opens the row under it",
          Search.activateAction({ searchMode: "typing" }), "open")

    // A walk with no matches yet is still working, so the area keeps the crawl rather than flashing empty.
    check("matches make the listing ready", Search.listingState({ searchRunning: true }, 3), "ready")
    check("a running walk with nothing yet is still loading", Search.listingState({ searchRunning: true }, 0), "loading")
    check("a stopped walk with nothing is empty", Search.listingState({ searchRunning: false }, 0), "empty")

    // The query line's own keys, beside the transitions they drive. Only the members run, close,
    // typed and backspace touch are stubbed, and open counts so a re-list can be told from none.
    function typing(query) {
        var sent = []
        return {
            searchMode: "typing", searchQuery: query, searchRunning: false, searchScanned: 0,
            searchCancelled: false, path: "/d", showHidden: false, total: 0, held: 0, rows: [],
            kindNames: [], cursorIndex: 0, listingState: "ready", opened: 0, sent: sent,
            home: "", searchFrom: "", relisted: "", searchHere: false,
            clearSelection: function () {},
            open: function (path) { this.opened += 1 },
            openWithoutHistory: function (path) { this.relisted = path },
            backend: { search: function (path, query, hidden) { sent.push(path + "?" + query) },
                       askFsInfo: function () { sent.push("fsinfo") }, askGitStatus: function () { sent.push("gitstatus") } }
        }
    }
    function press(code, text) { return { key: code, text: text, modifiers: Qt.NoModifier } }
    var line = typing("scr")
    Search.typeKey(press(Qt.Key_E, "e"), line)
    check("a printable key extends the query", line.searchQuery, "scre")
    Search.typeKey(press(Qt.Key_Backspace, ""), line)
    check("backspace shortens it", line.searchQuery, "scr")
    check("a key that means nothing on the line is still consumed by it",
          Search.typeKey(press(Qt.Key_Left, ""), line) + "|" + line.searchQuery, "true|scr")
    Search.typeKey(press(Qt.Key_Return, ""), line)
    check("enter commits the walk", line.searchMode + "|" + line.sent.join(","), "results|/d?scr,fsinfo")
    var abandoned = typing("scr")
    Search.typeKey(press(Qt.Key_Escape, ""), abandoned)
    check("escape abandons the line without re-listing, since no walk ran",
          abandoned.searchMode + "|" + abandoned.searchQuery + "|" + abandoned.opened, "||0")
    var underHome = typing("scr")
    underHome.home = "/home/u"
    underHome.path = "/home/u/Downloads"
    Search.typeKey(press(Qt.Key_Return, ""), underHome)
    check("the walk is sent the home scope, and the pane takes it as its listing base",
          underHome.sent.join(",") + "|" + underHome.path, "/home/u?scr,fsinfo|/home/u")
    check("where the search was started from is remembered", underHome.searchFrom, "/home/u/Downloads")
    Search.close(underHome)
    check("leaving the results returns there, and never as a history entry",
          underHome.relisted + "|" + underHome.opened, "/home/u/Downloads|0")

    var again = typing("scr")
    again.home = "/home/u"
    again.path = "/home/u/Downloads"
    Search.typeKey(press(Qt.Key_Return, ""), again)
    Search.start(again)
    again.searchQuery = "other"
    Search.typeKey(press(Qt.Key_Return, ""), again)
    check("a second search from the results still remembers the first one's origin",
          again.searchFrom, "/home/u/Downloads")
    Search.close(again)
    check("so esc returns where the operator began, not to the scope",
          again.relisted, "/home/u/Downloads")

    // Issue 30: the scope is chosen on the query line itself rather than in the settings, because it
    // belongs to this search and not to the application; the strip's own "in <scope>" is the readout,
    // and nothing is persisted, which is the whole reason this needs no settings row.
    check("with the scope set to here the walk stays in the pane's own directory",
          Search.scopeRoot("/home/u/Downloads/deep", "/home/u", true), "/home/u/Downloads/deep")
    check("and with it off the walk is still the whole home directory",
          Search.scopeRoot("/home/u/Downloads/deep", "/home/u", false), "/home/u")
    var here = typing("scr")
    here.home = "/home/u"
    here.path = "/home/u/Downloads"
    Search.typeKey(press(Qt.Key_Tab, "\t"), here)
    check("tab on the query line points the walk at the directory the pane is in", here.searchHere, true)
    // Both presses land while the line still has the caret, which is the only state ui/js/Focus.js
    // handleKey routes to typeKey at all: after enter the mode is results and the key is the rail's.
    Search.typeKey(press(Qt.Key_Backtab, "\t"), here)
    check("and the key flips back, so home is one press away again", here.searchHere, false)
    Search.typeKey(press(Qt.Key_Tab, "\t"), here)
    Search.typeKey(press(Qt.Key_Return, ""), here)
    check("and the walk goes there instead of to home",
          here.sent.join(",") + "|" + here.path, "/home/u/Downloads?scr,fsinfo|/home/u/Downloads")

    var blank = typing("")
    Search.typeKey(press(Qt.Key_Return, ""), blank)
    check("enter on an empty line closes it rather than walking for nothing",
          blank.searchMode + "|" + blank.sent.length, "|0")

    // The terminal searched line. The backend ranks the rows in the statement before it writes that
    // line, so a client still drawing the walk's discovery order resolves every destructive key
    // against a listing it is not showing: trash on the highlighted row took another file.
    // Only the members Search.ranked touches, and the backend stub records the wire, because the
    // window request is the whole of it. The cursor starts off row 0 so a reset can be told from none.
    function results() {
        var p = {
            windowSize: 200,
            thumbState: "stale",
            dirSizeState: "stale",
            cursor: 7,
            cleared: 0,
            sent: []
        }
        p.clearSelection = function () { p.cleared += 1 }
        p.setCursor = function (index) { p.cursor = index }
        p.backend = { window: function (start, count) { p.sent.push("window " + start + " " + count) } }
        return p
    }

    var reordered = results()
    Search.ranked(reordered)
    check("a ranked walk drops the thumbnail cache, whose keys are row indices",
          reordered.thumbState === "stale", false)
    check("and the directory-size cache, keyed the same way",
          reordered.dirSizeState === "stale", false)
    check("and clears a selection of indices that now name other files", reordered.cleared, 1)
    check("and puts the cursor on the highest-ranked row rather than a stale index", reordered.cursor, 0)
    check("and re-reads the window, which is what keeps trash on the row that is drawn",
          reordered.sent.join(","), "window 0 200")
}
