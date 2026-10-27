.pragma library
.import "Input.js" as Input

.import "DirSizes.js" as DirSizes
.import "Format.js" as Format
.import "Match.js" as Match
.import "Thumbs.js" as Thumbs

// The strip's two sentences and the no-match tile's heading, each filled by Format.fill.
var WORDS = {
    noMatchOne: "No match among 1 item",
    noMatchMany: "No match among %1 items",
    shown: "%1 of %2 shown",
    shownOfFolder: "%1 of %2 shown · of %3 in this folder",
    nothingMatched: "Nothing matched %1"
}

// The filter narrows the listing already on screen: no walk, no round trip, and every row it keeps
// is one the backend has already sent. ui/js/Search.js is its bigger sibling, which walks the
// subtree and replaces the listing; the design canvas draws the two as different things and this
// file never reaches ui/Backend.qml at all. ui/js/FilterWire.js is what asks the backend to test the
// rest of a folder the held window does not cover, and takes its answers back (issue 315).
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

// -1 for a row the filter hides, which is what the callers below clamp against. The list is ascending,
// so this is a binary search: every drawn delegate asks, and a folder's answer can be 100,000 rows.
function viewOf(list, row) {
    if (list === null) return row
    var lo = 0, hi = list.length - 1
    while (lo <= hi) {
        var mid = (lo + hi) >> 1
        if (list[mid] === row) return mid
        if (list[mid] < row) lo = mid + 1; else hi = mid - 1
    }
    return -1
}

// Issue 92, nixfred: a query matching nothing leaves the cursor on a row the filter no longer draws.
function cursorShown(pane) { return viewOf(pane.shown, pane.cursorIndex) >= 0 }

// The strip's own sentence, SearchFilter rules 1 and 2: what the filter kept, out of the rows it
// could test, and the directory those rows are a window on. The scope is the half that must not be
// dropped, because zero matches among 350 loaded rows is not zero matches in a 104,812-file
// directory; the tail goes when the window is the whole listing, which is nothing left to say.
function summary(list, loaded, total) {
    if (list === null) {
        return ""
    }
    if (total > loaded) {
        return Format.fill(WORDS.shownOfFolder, Format.count(list.length), Format.count(loaded), Format.count(total))
    }
    return Format.fill(WORDS.shown, Format.count(list.length), Format.count(loaded))
}

// States' no-match tile: the pane names the query back and says how many rows it tested, because
// the query is up in a strip the reader may have looked away from. ui/StateMessage.qml draws it only
// once the whole folder was tested (issue 315: 321 held rows once stood as "No match among 5,000 items").
function noMatch(tested) {
    return tested === 1 ? WORDS.noMatchOne : Format.fill(WORDS.noMatchMany, Format.count(tested))
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
    var out = []
    for (var j = 0; j < asked.length; j++) {
        if (viewOf(list, asked[j]) >= 0) {
            out.push(asked[j])
        }
    }
    return out
}

// A pending thumbnail can be hidden inside the planner's span as well as outside its viewport. Each
// pending row is looked up in the list, not the list spread into a table: the list can be a folder's
// 100,000 matches and this runs on every keystroke and settle (issue 315).
function cut(work, list, state) {
    var drop = work.drop.slice()
    if (list !== null) {
        var dropping = {}
        for (var j = 0; j < drop.length; j++) dropping[drop[j]] = true
        for (var key in state.file) {
            var row = Number(key)
            if (Thumbs.pending(state.file[key]) && viewOf(list, row) < 0 && dropping[row] !== true)
                drop.push(row)
        }
    }
    return { ask: keep(work.ask, list), drop: drop }
}

// The rows a view range draws under a filter, each the row the pane holds for its listing row, from the
// held window or the filter's (issue 315), or null for one not landed yet: what the two planners below
// walk, since the drawn rows are a set and not a run, and two matches a hundred thousand rows apart are
// two steps and not a walk of the rows between.
function drawn(pane, first, last) {
    var list = pane.shown
    var out = []
    if (list.length === 0) return out
    var lo = Math.max(0, Math.min(list.length - 1, first))
    var hi = Math.max(0, Math.min(list.length - 1, last))
    for (var v = lo; v <= hi; v++) out.push({ index: list[v], row: pane.rowFor(list[v]) })
    return out
}

// The thumbnail plan over what a view range draws: the held window's run with no filter up, else the
// rows drawn, so a matched row only the filter window holds gets its thumbnail too, and every pending
// row the range does not draw, hidden or scrolled off, is dropped. first and last are view positions.
function planThumbs(pane, first, last, mode) {
    if (pane.shown === null) return Thumbs.plan(pane.thumbState, pane.rows, pane.held, first, last, mode)
    return Thumbs.planDrawn(pane.thumbState, drawn(pane, first, last), mode)
}

// The folder-size plan the same way.
function planDirSizes(pane, first, last) {
    if (pane.shown === null) return DirSizes.plan(pane.dirSizeState, pane.rows, pane.held, first, last)
    return DirSizes.planDrawn(pane.dirSizeState, drawn(pane, first, last))
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

// Esc itself. The view sat at the top of the matches while the held window may be a thousand rows
// down the folder, so the whole listing comes back parked on the cursor's row; left at the top it
// drew a screen of rows the pane does not hold, which is the blank list in issue 315.
function escape(pane) {
    var cursor = pane.cursorIndex
    close(pane)
    park(pane, cursor)
}

// The listing back whole, parked on the row the cursor held under the filter. The count growing moved
// the view to its top (ui/StableModel.qml resize) and the view pulled the cursor onto a row there
// (clampCursor below), all inside the write of the query, so the cursor is read before and put back after.
function park(pane, cursor) {
    if (pane.cursorIndex !== cursor) {
        pane.cursorIndex = cursor
        pane.cursorSeq += 1
    }
    pane.showRow(cursor)
}

// The new query is applied here rather than by writing the property alone, because the selection has
// to lose the rows that just dropped out before anything can act on rows nobody can see. The next
// match list is computed rather than read back off the pane's binding, so the order is not a guess.
// The cursor is settled first, because the write of the query asks the backend around the cursor
// (ui/js/FilterWire.js ask) and moves the view, which pulls the cursor the way park above says.
function apply(pane, query) {
    var next = shown(pane.rows, pane.held, query)
    // Over a window smaller than the folder, only the held rows were tested; the folder's answer prunes the rest when it lands.
    prune(pane, next, pane.rows.length < pane.total ? { first: pane.held, last: pane.held + pane.rows.length - 1 } : null)
    var cursor = pane.cursorIndex
    // A cursor the keystroke hid takes the first row still standing, so it is never off screen. Hidden is
    // tested and failed: a held row not among the matches, or a row only the filter window holds whose
    // name fails the query. A row the held matches do not name and no test failed is untested, outside
    // every window the pane holds or matching in the filter's: it stays for the folder's answer to judge,
    // so a letter never moves the cursor off a row that still matches (issue 315).
    var row = pane.rowFor(cursor)
    var kept = next === null || viewOf(next, cursor) >= 0
    var hidden = !kept && next.length > 0 && !!row && Match.run(row.n, query).start < 0
    if (hidden) {
        pane.cursorIndex = next[0]
        pane.cursorSeq += 1
    }
    var emptied = query.length === 0 && pane.filterQuery.length > 0
    if (query !== pane.filterQuery) pane.filterKeyAt = Date.now()
    pane.filterQuery = query
    if (query.length > 0) pane.filterNarrowedAt = Date.now()
    if (emptied) {
        park(pane, cursor)
    } else if (hidden) {
        pane.showRow(0)
    } else if (!kept && pane.cursorIndex !== cursor) {
        // The view's pull took an untested cursor; it goes back, and the folder's answer scrolls to it or moves it.
        pane.cursorIndex = cursor
        pane.cursorSeq += 1
    }
}

// A selection you cannot see is one you can act on by accident, and trash, cut and copy all take the
// selection with no confirmation at all: what the filter hides, it takes out of the selection too.
// span is the listing rows the list was tested over, or null for the whole folder: a selected row
// outside it is untested rather than hidden, and stays until the folder's answer says (issue 315).
// Each selected row is looked up in the list, so a folder's 100,000 matches cost nothing with nothing selected.
function prune(pane, list, span) {
    if (list === null) {
        return
    }
    var was = pane.selectedIndices()
    var dropped = 0
    for (var j = 0; j < was.length; j++) {
        if (span && (was[j] < span.first || was[j] > span.last)) continue
        if (viewOf(list, was[j]) < 0) {
            pane.selection.toggle(was[j])
            dropped += 1
        }
    }
    if (dropped > 0) {
        pane.selectionVersion += 1
    }
}

// Cursor is a listing row but moves through drawn rows, so both steps convert.
function setCursor(pane, index, context) {
    setCursorView(pane, viewOf(pane.shown, index), context)
}

function setCursorView(pane, view, context) {
    if (pane.shownTotal === 0) {
        return
    }
    var to = Math.max(0, Math.min(pane.shownTotal - 1, view))
    pane.cursorIndex = at(pane.shown, to)
    // Every cursor setter bumps cursorSeq, so a plain move ends a Shift gesture.
    pane.cursorSeq += 1
    pane.showRow(to, context)
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
        var held = pulledFrom(pane)
        var from = held >= 0 ? held : pane.cursorIndex
        pane.cursorIndex = at(pane.shown, to)
        pane.cursorSeq = (pane.cursorSeq || 0) + 1
        pane.cursorPulled = { from: from, to: pane.cursorIndex, seq: pane.cursorSeq, version: pane.selectionVersion }
    }
}

// The row the cursor held before the wheel pulled it, while no move, mark or new listing came since (issue 286); else -1.
function pulledFrom(pane) {
    var pull = pane.cursorPulled
    var current = pull && pull.seq === (pane.cursorSeq || 0) && pull.to === pane.cursorIndex
        && pull.version === pane.selectionVersion
    return current ? pull.from : -1
}

// The query line's own keys while it has the caret, the shape ui/js/Search.js typeKey uses for the
// search's. Enter commits, escape abandons, backspace shortens, every printable character narrows.
function typeKey(event, pane) {
    // Ctrl+V and Shift+Insert type the clipboard's first line as one narrowing (issue 287).
    if (Input.isPasteKey(event))
        return Input.pasteInto(pane, function (text) { typed(pane, text) })
    if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
        commit(pane)
        return true
    }
    if (event.key === Qt.Key_Escape) {
        escape(pane)
        return true
    }
    if (event.key === Qt.Key_Backspace) {
        backspace(pane)
        return true
    }
    if (Input.isPrintable(event.text)) {
        typed(pane, event.text)
        return true
    }
    return true
}
