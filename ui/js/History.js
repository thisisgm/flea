.pragma library

// Back and Forward put the cursor back on the row it left, by name. ui/js/Nav.js keeps the public
// verbs and their guards; this holds the entries and the restore that ui/Pane.qml historyRestore
// carries until ui/PaneSwap.qml lands the rows and, past the held window, the locate reply.

// One stack entry: the directory and the row its cursor named. Frozen, because ui/js/Tabs.js
// snapshots the stacks with slice() and a shared entry must not change under another tab.
function entry(path, name, index) {
    return Object.freeze({ path: String(path), name: String(name || ""), index: Math.max(0, Math.floor(Number(index) || 0)) })
}

// The departing cursor. A walk's rows, its results or a query retyped over them (searchFrom stays
// set), name matches under its scope rather than rows of the directory the entry records, so a walk
// leaves no name and the top row. While a locate is still owed the cursor sits on a placeholder row,
// so the entry it owes is recorded again instead.
function capture(pane) {
    var held = owed(pane)
    if (held)
        return held.entry
    var walk = pane.searchMode === "results" || (pane.searchFrom || "").length > 0
    var row = walk ? null : pane.rowFor(pane.cursorIndex)
    return entry(pane.path, row ? row.n : "", walk ? 0 : pane.cursorIndex)
}

// One step through the stacks, called after Nav's in-flight guard: the departing cursor goes onto
// the other stack, and the pop happens before the open, which would otherwise push it straight back.
function travel(pane, from, to) {
    var stack = pane[from]
    var target = stack[stack.length - 1]
    pane[to] = (pane[to] || []).concat([capture(pane)])
    pane[from] = stack.slice(0, -1)
    pane.openWithoutHistory(target.path, { restore: target })
}

// Every listing that passed its guard comes through here, so a newer navigation, refresh, re-read
// or tab hop replaces what an older Back or Forward still owed. Three stages share the one record:
// the listing's first rows (asked "", not waiting), a re-sort's rows (waiting, listing the numbering
// it replaces) and a locate in flight (asked, the path sent).
function arm(pane, restore) {
    pane.historyRestore = restore ? record(restore, "", 0, -1, -1, false) : null
}

function record(entry, asked, listing, cursor, selection, waiting) {
    return { entry: entry, asked: asked, listing: listing, cursor: cursor, selection: selection, waiting: waiting }
}

function cancel(pane) {
    pane.historyRestore = null
}

// A same-path tab hop that re-sorts: the cursor waits for the reordered rows, see landed().
function wait(pane, entry) {
    pane.historyRestore = entry ? record(entry, "", pane.backend.heldListing, -1, pane.selectionVersion, true) : null
}

// ui/PaneSwap.qml, when ui/js/Tabs.js applyPending answered the listing's first rows with a sort:
// those rows are in the order being replaced, so the restore waits for the sort's own.
function defer(pane) {
    var want = pane.historyRestore
    if (want && want.asked.length === 0)
        wait(pane, want.entry)
}

// A same-path tab hop that re-reads nothing: the tab's owed cursor resolves over the rows already
// landed, by name, through the same held-window and locate route a listing takes.
function resume(pane, entry) {
    pane.historyRestore = null
    if (entry && !pane.listInFlight && pane.path === entry.path && pane.total > 0 && (pane.searchMode || "").length === 0)
        resolve(pane, entry)
}

// The restore still owed after its listing landed, or null once anything it relied on changed: the
// directory, a search, a filter, and for a locate in flight a listing, the numbering, the cursor or
// the selection. The cursor check catches writers that bypass ui/Pane.qml showRow. capture(),
// ui/js/Tabs.js snapshot() and ui/js/Anchor.js busy() read this too.
function owed(pane) {
    var want = pane.historyRestore
    if (!want || (want.asked.length === 0 && !want.waiting))
        return null
    if (pane.path !== want.entry.path || (pane.searchMode || "").length > 0
            || (pane.filterQuery || "").length > 0 || pane.filterTyping === true)
        return null
    if (want.waiting)
        return pane.selectionVersion === want.selection ? want : null
    if (pane.listInFlight || pane.backend.heldListing !== want.listing || pane.cursorIndex !== want.cursor
            || pane.selectionVersion !== want.selection)
        return null
    return want
}

function owedEntry(pane) {
    var want = owed(pane)
    return want ? want.entry : null
}

// ui/PaneSwap.qml onListInFlightChanged: a listing that ended without its rows landing (a failure,
// a refusal) owes nothing, so a later rows reply cannot spend it on another directory.
function ended(pane) {
    var want = pane.historyRestore
    if (want && want.asked.length === 0 && !want.waiting)
        pane.historyRestore = null
}

// The listing's own first rows, before ui/PaneSwap.qml clears listInFlight, or for a waiting restore
// the first rows in a numbering other than the one the sort replaced.
function landed(pane) {
    var want = pane.historyRestore
    if (!want || want.asked.length > 0)
        return
    if (want.waiting && (pane.listInFlight || pane.backend.heldListing === want.listing))
        return
    // A filter or selection made while the reordered rows were out is newer intent than the row the tab owed.
    if (want.waiting && ((pane.filterQuery || "").length > 0 || pane.filterTyping === true
            || pane.selectionVersion !== want.selection)) {
        pane.historyRestore = null
        return
    }
    if ((!want.waiting && (!pane.listInFlight || pane.listingPath !== want.entry.path))
            || pane.path !== want.entry.path || pane.total === 0 || (pane.searchMode || "").length > 0) {
        pane.historyRestore = null
        return
    }
    resolve(pane, want.entry)
}

// A name in the held window lands at once; a listing held whole cannot hold it anywhere else;
// otherwise the backend's name-only locate answers where it is, and no entry is stat'ed.
function resolve(pane, entry) {
    var name = entry.name
    if (name.length > 0) {
        for (var i = 0; i < pane.rows.length; i++) {
            if (String(pane.rows[i].n) === name) {
                settle(pane, pane.held + i)
                return
            }
        }
        if (pane.held > 0 || pane.rows.length < pane.total) {
            var asked = pane.join(pane.path, name)
            pane.historyRestore = record(entry, asked, pane.backend.heldListing, pane.cursorIndex, pane.selectionVersion, false)
            pane.backend.send({ c: "locate", path: asked })
            return
        }
    }
    settle(pane, entry.index)
}

// The single-path located line, routed by ui/PaneWire.qml. The batch forms carry matches and belong
// to the transfer retry and the survivor reader. A reply is spent once and moves nothing unless the
// restore is still owed() for the directory it names; a deliberate cursor move, a wheel's clamp
// included, already dropped it through ui/Pane.qml.
function located(pane, message) {
    var want = pane.historyRestore
    if (!want || want.asked.length === 0 || message.matches !== undefined || message.path !== want.asked)
        return
    var valid = owed(pane) !== null && message.directory === pane.path
    pane.historyRestore = null
    if (valid)
        settle(pane, message.index >= 0 ? message.index : want.entry.index)
}

// A missing row falls back to the saved index, clamped to the listing; setCursor's own scroll asks
// for the viewport-sized window around a row outside the held one.
function settle(pane, index) {
    pane.historyRestore = null
    if (pane.total > 0)
        pane.setCursor(Math.min(index, pane.total - 1))
}
