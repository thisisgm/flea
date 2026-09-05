.pragma library

.import "DirSizes.js" as DirSizes
.import "Format.js" as Format
.import "Thumbs.js" as Thumbs

// The subtree search's behaviour, taking ui/Pane.qml's root the way Focus.js does; Pane holds the
// state, this holds what the state does. The wire is docs/protocol.md "search".
var OFF = ""
var TYPING = "typing"
var RESULTS = "results"

// f opens the query line. The walk does not start here: a subtree walk per keystroke would be a
// sweep, and the design's own ruling is that enter commits the query.
function start(root) {
    root.searchMode = TYPING
}

function typed(root, character) {
    root.searchQuery += character
}

function backspace(root) {
    root.searchQuery = root.searchQuery.substring(0, root.searchQuery.length - 1)
}

// Universal search: the walk covers the whole home directory whenever the pane sits somewhere
// inside it, and the pane's own directory otherwise, so a search started on a NAS mount searches
// that mount rather than silently walking home instead. The backend has no notion of home and
// walks whatever path this picks, see docs/protocol.md "search".
//
// Issue 30 wanted the other one, so here is the operator's own answer: with it set the walk stays
// where the pane is standing. It is a property of this window and not of the application: close()
// and reveal() below leave it alone, so it holds until it is pressed again or the window closes,
// and nothing writes it to ui.json.
function scopeRoot(path, home, here) {
    if (here === true) {
        return path
    }
    if (home.length === 0) {
        return path
    }
    if (path === home || String(path).indexOf(home + "/") === 0) {
        return home
    }
    return path
}

// Enter commits: the walk starts and the keyboard goes back to the results, so j/k move again.
function run(root) {
    if (root.searchQuery.length === 0) {
        close(root)
        return
    }
    var scope = scopeRoot(root.path, root.home, root.searchHere)
    // The scope becomes the pane's path because it is the listing's base: every result name is
    // relative to it, so join, reveal and every per-row facility keep working untouched.
    // A second search started from the results keeps the first one's origin: the pane's path is
    // the scope by then, so overwriting this would send esc to home instead of where it began.
    if (root.searchFrom.length === 0) {
        root.searchFrom = root.path
    }
    root.path = scope
    root.searchMode = RESULTS
    root.searchRunning = true
    root.searchScanned = 0
    root.searchCancelled = false
    root.total = 0
    root.held = 0
    root.rows = []
    root.kindNames = []
    root.cursorIndex = 0
    root.listingState = "loading"
    root.clearSelection()
    root.backend.search(scope, root.searchQuery, root.showHidden)
}

// Esc stops a running walk and leaves the results up; a second Esc is what returns to the listing.
function cancel(root) {
    if (root.searchRunning) {
        root.backend.searchcancel()
        return
    }
    close(root)
}

// Leaving search re-lists the directory the search was started from, which is not the scope it
// walked: a home-wide search begun in Downloads returns to Downloads, not to home. No history
// entry, because entering and leaving a search is not a navigation.
function close(root) {
    var relist = root.searchMode === RESULTS
    var back = root.searchFrom.length > 0 ? root.searchFrom : root.path
    root.searchMode = OFF
    root.searchQuery = ""
    root.searchRunning = false
    root.searchScanned = 0
    root.searchFrom = ""
    if (relist) {
        root.openWithoutHistory(back)
    }
}

// The query line's own keys while it has the caret, the twin of ui/js/Filter.js typeKey: printable
// characters extend it, backspace shortens it, enter commits the walk and escape abandons it.
// Nothing else reaches the list while the caret is up, so every key answers consumed.
function typeKey(event, root) {
    if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
        run(root)
        return true
    }
    if (event.key === Qt.Key_Escape) {
        close(root)
        return true
    }
    if (event.key === Qt.Key_Backspace) {
        backspace(root)
        return true
    }
    // Issue 30's control: tab flips the scope between the whole home directory and the one the pane
    // is standing in. It is the only writer, and nothing resets it, so the flip holds for the window;
    // ui/SearchStrip.qml draws "in <scope>" under the caret, so every search says which one it took.
    if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) {
        root.searchHere = !root.searchHere
        return true
    }
    if (event.text.length === 1 && event.text >= " ") {
        typed(root, event.text)
        return true
    }
    return true
}

// o on a result opens the directory that holds it and puts the cursor on the row, the design's reveal.
function reveal(root) {
    var row = root.rowFor(root.cursorIndex)
    if (!row) {
        return
    }
    var full = root.join(root.path, row.n)
    var cut = full.lastIndexOf("/")
    if (cut <= 0) {
        return
    }
    root.searchMode = OFF
    root.searchQuery = ""
    root.searchRunning = false
    root.searchFrom = ""
    root.pendingSelect = full
    root.open(full.substring(0, cut))
}

// What activating a row means right now. On a search result the operator's ruling is that it takes
// you to the file instead of opening it; the canvas keeps enter on open and o on reveal, so it is
// only the pointer that asks this. ui/js/Tap.js is the one caller.
function activateAction(root) {
    return root.searchMode === RESULTS ? "reveal" : "open"
}

// The terminal searched line: the walk ranks its rows in the statement before writing it, so every
// row index the client still holds names another file, exactly as a re-sort's do. docs/protocol.md
// "searched" states the re-read this discharges; the five moves are ui/js/Sort.js resort's own.
function ranked(root) {
    root.thumbState = Thumbs.empty()
    root.dirSizeState = DirSizes.empty()
    root.clearSelection()
    // Row 0 is the best match once the rank has run, so the reset lands the cursor on the answer.
    root.setCursor(0)
    root.backend.window(0, root.windowSize)
}

// A walk with no matches yet is still working, so the list area keeps the crawl rather than
// flashing the empty state at every directory that happens to hold nothing.
function listingState(root, total) {
    if (total > 0) {
        return "ready"
    }
    return root.searchRunning ? "loading" : "empty"
}

// The strip's right edge: the running count while there is one, the terminal word when there is not.
function note(total, running, cancelled) {
    if (total > 0) {
        return total + " found"
    }
    if (running) {
        return "searching"
    }
    return cancelled ? "stopped" : "done"
}

// The rule itself lives in Format.tilde, because the window chrome draws a path through the same one.
function scope(path, home) {
    return Format.tilde(path, home)
}

// Scanned counts reach six figures on a real subtree, so they are grouped the way the canvas draws them.
function grouped(n) {
    var digits = String(n)
    var out = ""
    for (var i = 0; i < digits.length; i++) {
        if (i > 0 && (digits.length - i) % 3 === 0) {
            out += ","
        }
        out += digits.charAt(i)
    }
    return out
}

// The status bar's own left half while a search is up, the two lines the canvas draws.
function statusLine(running, total, scanned, ms) {
    if (running) {
        return "Searching, " + grouped(scanned) + " scanned"
    }
    return grouped(scanned) + " scanned in " + (ms / 1000).toFixed(1) + " s"
}

// The status bar's right half: what the keys do, which changes the moment the walk stops.
function statusKeys(running) {
    return running
        ? "esc cancels, enter opens, o reveals"
        : "esc returns to the listing"
}
