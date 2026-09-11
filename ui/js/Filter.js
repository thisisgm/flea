.pragma library

.import "Match.js" as Match
.import "Thumbs.js" as Thumbs

// The filter narrows the listing already on screen: no walk, no round trip, and every row it keeps
// is one the backend has already sent. ui/js/Search.js is its bigger sibling, which walks the
// subtree and replaces the listing; the design canvas draws the two as different things and this
// file never reaches ui/Backend.qml at all.
//
// Two index spaces meet here. A "listing row" is what the backend numbers, what a selection holds
// and what every write operation sends; a "view position" is where a row is drawn. With no filter
// up the two are the same number, and at() and viewOf() below are the only places that convert.

// The listing rows a query leaves standing, in the order the backend sent them, or null when there
// is no filter at all. Only the rows the pane holds can be tested, so the answer is a subsequence
// of the held window; scope() below is what says so when the window is not the whole listing.
// It is a subsequence and never a re-ranking, which is what keeps directories ahead of files.
function shown(rows, held, query) {
    if (query.length === 0) {
        return null
    }
    var out = []
    for (var i = 0; i < rows.length; i++) {
        // The same run ui/js/Match.js paints in the name, so a row can never match without showing why.
        if (Match.run(rows[i].n, query).start >= 0) {
            out.push(held + i)
        }
    }
    return out
}

function at(list, view) {
    if (list === null) {
        return view
    }
    // A delegate can outlive the view position it was built for by a frame when the match list
    // shrinks, so a position past the end answers -1: a listing row nothing holds and nothing draws.
    return view < list.length ? list[view] : -1
}

// -1 for a row the filter hides, which is what the callers below clamp against.
function viewOf(list, row) {
    return list === null ? row : list.indexOf(row)
}

// The canvas's own line under the last row it left standing, States.dc.html "Filter active".
function note(list, loaded, query) {
    if (list === null) {
        return ""
    }
    if (list.length === 0) {
        return "Nothing matches " + query
    }
    var dropped = loaded - list.length
    if (dropped === 0) {
        return ""
    }
    return dropped + (dropped === 1 ? " row" : " rows") + " hidden by the filter"
}

// The pane holds a window around the viewport, not the directory, so on a listing bigger than that
// window the filter has only seen the rows it holds and the strip says which ones. Empty when the
// window is the whole listing, which is the OEM rule of saying nothing when there is nothing to say.
function scope(loaded, total) {
    return total > loaded ? "in the " + loaded + " rows loaded" : ""
}

// The rows drawn between two ends, which is not the range between them: a plain index range would
// sweep up every row the filter hid in the gap, and nothing on screen would say it had.
function between(list, a, b) {
    var lo = Math.min(a, b)
    var hi = Math.max(a, b)
    var out = []
    for (var i = 0; i < list.length; i++) {
        if (list[i] >= lo && list[i] <= hi) {
            out.push(list[i])
        }
    }
    return out
}

// A request cut back to the rows still drawn. A filtered viewport covers a set and not a run, so
// the range-shaped planners hand back rows the filter hides, and asking for those would fetch work
// nothing draws.
function keep(asked, list) {
    if (list === null) {
        return asked
    }
    var drawn = {}
    for (var i = 0; i < list.length; i++) {
        drawn[list[i]] = true
    }
    var out = []
    for (var j = 0; j < asked.length; j++) {
        if (drawn[asked[j]] === true) {
            out.push(asked[j])
        }
    }
    return out
}

// A pending thumbnail can be hidden inside the planner's span as well as outside its viewport.
function cut(work, list, state) {
    var drop = work.drop.slice()
    if (list !== null) {
        var drawn = {}
        for (var i = 0; i < list.length; i++) drawn[list[i]] = true
        var dropping = {}
        for (var j = 0; j < drop.length; j++) dropping[drop[j]] = true
        for (var key in state.file) {
            var row = Number(key)
            if (state.file[key] === Thumbs.ASKED && drawn[row] !== true && dropping[row] !== true)
                drop.push(row)
        }
    }
    return { ask: keep(work.ask, list), drop: drop }
}

// The listing rows a view range covers, for the two planners that take a first and a last.
function span(list, first, last) {
    if (list === null) {
        return { first: first, last: last }
    }
    if (list.length === 0) {
        return { first: 0, last: -1 }
    }
    var lo = Math.max(0, Math.min(list.length - 1, first))
    var hi = Math.max(0, Math.min(list.length - 1, last))
    return { first: list[lo], last: list[hi] }
}

// The transitions, taking ui/Pane.qml's root the way ui/js/Search.js and ui/js/Sort.js do: the pane
// holds the state, this holds what the state does.

// "/" opens the query line. Unlike the search's, nothing is committed to start a walk: the rows are
// already here, so the listing narrows on the keystroke itself.
function start(pane) {
    pane.filterTyping = true
}

function typed(pane, character) {
    apply(pane, pane.filterQuery + character)
}

function backspace(pane) {
    apply(pane, pane.filterQuery.substring(0, pane.filterQuery.length - 1))
}

// Enter hands the keyboard back to the list with the filter standing, which is what makes y, x, d
// and r work over the narrowed rows; an empty query has nothing to stand, so it closes instead.
function commit(pane) {
    if (pane.filterQuery.length === 0) {
        close(pane)
        return
    }
    pane.filterTyping = false
}

// Esc, and every fresh listing. The selection is deliberately not cleared: prune() below has already
// cut it to rows the filter was showing, so what is left is exactly what was on screen.
function close(pane) {
    pane.filterQuery = ""
    pane.filterTyping = false
}

// The new query is applied here rather than by writing the property alone, because the selection has
// to lose the rows that just dropped out before anything can act on rows nobody can see. The next
// match list is computed rather than read back off the pane's binding, so the order is not a guess.
function apply(pane, query) {
    var next = shown(pane.rows, pane.held, query)
    prune(pane, next)
    pane.filterQuery = query
    // A cursor the filter just hid takes the first row still standing, so it is never off screen.
    if (next !== null && next.length > 0 && viewOf(next, pane.cursorIndex) < 0) {
        pane.cursorIndex = next[0]
        pane.showRow(0)
    }
}

// A selection you cannot see is one you can act on by accident, and trash, cut and copy all take the
// selection with no confirmation at all: what the filter hides, it takes out of the selection too.
function prune(pane, list) {
    if (list === null) {
        return
    }
    var drawn = {}
    for (var i = 0; i < list.length; i++) {
        drawn[list[i]] = true
    }
    var was = pane.selectedIndices()
    var dropped = 0
    for (var j = 0; j < was.length; j++) {
        if (drawn[was[j]] !== true) {
            pane.selection.toggle(was[j])
            dropped += 1
        }
    }
    if (dropped > 0) {
        pane.selectionVersion += 1
    }
}

// The cursor is a listing row everywhere else in the app but it moves through what is drawn, so both
// steps convert. ui/Pane.qml keeps the scroll itself, because ListView.Contain has no name here.
function setCursor(pane, index) {
    setCursorView(pane, viewOf(pane.shown, index))
}

function setCursorView(pane, view) {
    if (pane.shownTotal === 0) {
        return
    }
    var to = Math.max(0, Math.min(pane.shownTotal - 1, view))
    pane.cursorIndex = at(pane.shown, to)
    pane.showRow(to)
}

function moveCursor(pane, delta) {
    setCursorView(pane, viewOf(pane.shown, pane.cursorIndex) + delta)
}

// The wheel moved the viewport and the cursor follows it. Both ends are view positions, because
// under a filter the listing rows they name are not a run and a numeric clamp would be meaningless.
function clampCursor(pane, first, last) {
    var was = viewOf(pane.shown, pane.cursorIndex)
    var to = Math.max(first, Math.min(last, was))
    if (to !== was) {
        pane.cursorIndex = at(pane.shown, to)
    }
}

// Ctrl+A takes what is drawn, never what is listed: under a filter that is the matches alone.
function selectAll(pane) {
    if (pane.shown === null) {
        pane.selection.all(pane.total)
        return
    }
    pane.selection.clear()
    for (var i = 0; i < pane.shown.length; i++) {
        pane.selection.toggle(pane.shown[i])
    }
}

// Shift+J and Shift+K, the whole gesture: the cursor moves through what is drawn and the selection
// follows it. corner: the anchor only re-latches to the cursor once the selection is empty, so a
// plain j/k move never has to special-case a shift+j/k chain already in progress.
function extend(pane, delta) {
    if (pane.selection.count() === 0) {
        pane.selectionAnchor = pane.cursorIndex
    }
    moveCursor(pane, delta)
    extendTo(pane, pane.selectionAnchor)
    pane.selectionVersion += 1
}

// Shift+click, the absolute twin of extend() above: the cursor lands on the clicked row and the
// selection covers the drawn rows between it and the anchor the gesture started from.
function extendToRow(pane, index) {
    if (pane.selection.count() === 0) {
        pane.selectionAnchor = pane.cursorIndex
    }
    setCursor(pane, index)
    extendTo(pane, pane.selectionAnchor)
    pane.selectionVersion += 1
}

// The rows drawn between the cursor and the anchor, which extend() above is the only caller of.
function extendTo(pane, anchor) {
    if (pane.shown === null) {
        pane.selection.extendTo(pane.cursorIndex, anchor)
        return
    }
    pane.selection.clear()
    var range = between(pane.shown, pane.cursorIndex, anchor)
    for (var i = 0; i < range.length; i++) {
        pane.selection.toggle(range[i])
    }
}

// The query line's own keys while it has the caret, the shape ui/js/Search.js typeKey uses for the
// search's. Enter commits, escape abandons, backspace shortens, every printable character narrows.
function typeKey(event, pane) {
    if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
        commit(pane)
        return true
    }
    if (event.key === Qt.Key_Escape) {
        close(pane)
        return true
    }
    if (event.key === Qt.Key_Backspace) {
        backspace(pane)
        return true
    }
    if (event.text.length === 1 && event.text >= " ") {
        typed(pane, event.text)
        return true
    }
    return true
}
