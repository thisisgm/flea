.pragma library

.import "Format.js" as Format
.import "Search.js" as Search

// The reclaim scan's behaviour, taking ui/Pane.qml's root the way Search.js does. Deliberately
// the search state machine's own fields: there is one walk, one header and one set of guards, so
// a reclaim is searchMode RESULTS with reclaimWalk naming which walk it is. The wire is
// docs/protocol.md "reclaim"; R starts a scan on the directory the pane stands in.

// A scan asked for on a search's results closes that search first, which returns the pane to
// where the search began, and the scan runs there: one searchFrom and one walk, the same rule a
// second search already answers to. Rows arrive ranked heaviest first, and dd on a row trashes
// that whole tree with undo behind it.
function start(root) {
    if (root.searchMode === Search.RESULTS && !root.reclaimWalk) {
        Search.close(root)
    }
    var scope = root.path
    if (root.searchFrom.length === 0) {
        root.searchFrom = scope
    }
    root.searchMode = Search.RESULTS
    root.reclaimWalk = true
    root.searchRunning = true
    root.searchScanned = 0
    root.searchCancelled = false
    root.reclaimBytes = 0
    root.total = 0
    root.held = 0
    root.rows = []
    root.kindNames = []
    root.cursorIndex = 0
    root.listingState = "loading"
    root.clearSelection()
    root.backend.reclaim(scope)
}

// Esc stops a running scan and leaves what it found listed; a second Esc returns to the listing
// the scan began from, which is Search.close's own move once the flag names no walk.
function cancel(root) {
    if (root.searchRunning) {
        root.backend.reclaimcancel()
        return
    }
    close(root)
}

function close(root) {
    root.reclaimWalk = false
    Search.close(root)
}

// The terminal reclaimed line answers with the same five moves a search's terminal line does: the
// rows were ranked on the wire before it arrived, so the client empties its per-row maps and
// re-reads its window. The backend seeded its own dirsize cache in the same statement, so the
// settle's size asks answer from that cache instead of walking every listed tree a second time.
function settled(root) {
    Search.ranked(root)
}

// The status bar's left half while the scan owns it: the running scan line, then the tally.
function statusLine(running, total, scanned, bytes, ms) {
    if (running) {
        return "Reclaiming, " + Search.grouped(scanned) + " scanned"
    }
    return Search.grouped(total) + (total === 1 ? " tree, " : " trees, ") + Format.size(bytes) + " in " + (ms / 1000).toFixed(1) + " s"
}

function statusKeys(running) {
    return running
        ? "esc cancels, dd trashes a tree, z undoes"
        : "esc returns to the listing"
}
