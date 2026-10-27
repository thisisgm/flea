import QtQuick
import "js/RecentMode.js" as RecentMode
import "js/Anchor.js" as Anchor
import "js/BackReturn.js" as BackReturn
import "js/DirSizes.js" as DirSizes
import "js/FilterWire.js" as FilterWire
import "js/Nav.js" as Nav
import "js/Reload.js" as Reload
import "js/Search.js" as Search
import "js/Startup.js" as Startup
import "js/Swap.js" as Swap
import "js/Tabs.js" as Tabs
import "js/Thumbs.js" as Thumbs

// Where a listing's listed and rows replies land, holding the old rows until they do; see AGENTS.md "The listing swap".
Item {
    id: root

    property var pane: null
    // ui/PaneWire.qml, which owns the anchor, the retry and the rename a landing listing settles.
    property var wire: null
    // The cursor a Back or Forward owes the asked listing, or null; hold() arms each listing's own.
    property var restore: null
    // The first window a landing set aside while its locate is out, or null.
    property var parked: null
    // A locate that never answers is a listing that never answers, so past Startup's listing wait the parked rows land at the top.
    property int parkedWaitMs: Startup.LISTING_WAIT_MS
    // Built on the first park, so a window that never parks a Back starts no object for it.
    property var parkedWait: null
    onParkedChanged: {
        if (!root.parked) {
            if (root.parkedWait !== null) root.parkedWait.stop()
            return
        }
        if (root.parkedWait === null) {
            var made = Qt.createComponent("ParkedWait.qml")
            if (made.status !== Component.Ready) throw new Error("ParkedWait.qml did not load: " + made.errorString())
            root.parkedWait = made.createObject(root, { swap: root })
        }
        root.parkedWait.restart()
    }
    property var phase: Swap.idle()
    readonly property bool holding: root.phase.holding
    readonly property bool fellBack: root.phase.fellBack

    // What tests/ui.sh noblank reads through ui/Ipc.qml swapState: how each hold ended and every empty frame.
    property int holds: 0
    property int fallbacks: 0
    // Rows lines that waited out the loading mark's minimum, which a rows line landing late does not count.
    property int waits: 0
    property int blankFrames: 0
    // The gen whose rows last landed: a new gen parks the cursor on its best match, a re-answer keeps the file.
    property int landedGen: 0
    // The query the drawn rows answered, set in the turn they land; rows read it, never the live text.
    property string answeredQuery: ""
    property int loadingFrames: 0
    property var last: ({ ms: 0, end: "" })
    // When ui/PaneStates.qml drew the loading mark, 0 while none is drawn, and how long the last one stood.
    property double markSince: 0
    property int shownMs: 0
    // Rows lines that came while the mark still owed time, landed in arrival order once it has stood its minimum.
    property var waiting: []
    // True from the turn after the search strip's first frame until the session closes: the views build what an answer draws then, unseen.
    property bool searchWarm: false

    // ui/js/Nav.js asks for every listing here; false means nothing is held, and one after a live answer holds the walk's state unless clearAtOnce.
    function hold(ask) {
        root.restore = ask.restore ? Object.assign({}, ask.restore) : null
        root.parked = null
        var now = Date.now()
        root.phase = Swap.begin(root.phase, root.pane.listingState, root.pane.searchMode, ask, now)
        if (!root.holding)
            return false
        root.holds += 1
        cap.interval = Swap.capLeft(root.phase, now)
        cap.restart()
        return true
    }

    function markShown(shown) {
        if (!shown && root.markSince > 0)
            root.shownMs = Date.now() - root.markSince
        root.markSince = shown ? Date.now() : 0
    }

    // The listed line held or fallen-back rows, or a drawn mark, wait for is kept for them, an earlier request's is dropped, any other applied.
    function takeListed(total, readMs, sortMs, path, changed, gen) {
        if (Swap.dropSessionLine(root.pane.searchMode, gen, root.pane.searchGen, root.pane.searchLeaving === true, root.pane.searchOpenedGen)) {
            return
        }
        var marked = Swap.marked(root.markSince, root.pane.listInFlight, root.pane.searchMode, root.phase.walk)
        var action = Swap.onListed(root.phase, root.pane.listInFlight, path, root.pane.listingPath, marked)
        if (action === Swap.DROP) {
            return
        }
        if (Swap.keepLive(root.pane.searchMode, root.pane.listInFlight) || action === Swap.KEEP) {
            root.phase = Swap.kept(root.phase, { total: total, readMs: readMs, sortMs: sortMs, path: path, changed: changed })
            root.pane.listedSeen = true
            return
        }
        root.phase = Swap.heard(root.phase, root.pane.searchMode === Search.RESULTS)
        root.applyListed(total, readMs, sortMs, path, changed)
    }

    // A rows line lands with the listed line kept for it, or alone; one ahead of its listed line is dropped as it always was.
    function takeRows(start, items, kinds, listing, gen) {
        if (Swap.dropSessionLine(root.pane.searchMode, gen, root.pane.searchGen, root.pane.searchLeaving === true, root.pane.searchOpenedGen)) {
            return
        }
        var marked = Swap.marked(root.markSince, root.pane.listInFlight, root.pane.searchMode, root.phase.walk)
        var left = marked ? Swap.showLeft(root.markSince, Date.now()) : 0
        var action = Swap.onRows(root.phase, root.pane.listInFlight, root.pane.listedSeen, left)
        if (action === Swap.DROP) {
            return
        }
        if (action === Swap.WAIT) {
            root.waiting.push({ start: start, items: items, kinds: kinds, listing: listing, gen: gen })
            root.waits += 1
            minShow.interval = left
            minShow.restart()
            return
        }
        if (gen > 0 && gen === root.pane.searchGen && root.pane.searchFirstRowsAt === 0) {
            root.pane.searchFirstRowsAt = Date.now()
        }
        // A re-sort renumbers every row, so a selection made while it was in flight names other files under the new numbering.
        if (Swap.clearsSelection(root.pane.backend.heldListing, listing) && root.pane.selectionCount() > 0)
            root.pane.clearSelection()
        root.pane.backend.heldListing = listing
        // A standing filter answer names the old numbering's rows, so it goes before a row of the new one is read (issue 315).
        FilterWire.renumbered(root.pane, listing)
        root.stampTags(items, action === Swap.LAND || root.pane.listInFlight)
        // Only a live answer names its query; a plain window reply answers none, so a scroll keeps the marks.
        if (gen > 0)
            root.answeredQuery = root.pane.searchQuery
        if (action === Swap.LAND) {
            // The owed name sits outside this window, so these rows wait for the window its locate names.
            if (BackReturn.deferForLocate(root.pane, root.restore, items, root.phase.listed)) {
                root.parked = { start: start, items: items, kinds: kinds }
                return
            }
            root.parked = null
            root.land(start, items, kinds, gen > 0 && gen !== root.landedGen && (root.pane.searchQuery || "").length > 0)
            root.landedGen = gen
            return
        }
        root.pane.held = start
        root.pane.rows = RecentMode.stampRows(root.pane, items)
        root.pane.kindNames = kinds
        root.rowsLanded()
    }

    // A rows reply is the held window: a tagged row builds the tag state, which stamps each row with its folder's slot.
    function stampTags(items, fresh) {
        if (root.pane.tagState === null && !items.some(function (row) { return row.tg !== undefined }))
            return
        root.pane.tags().take(items, fresh)
    }

    // Reset, path and rows land in one turn (past the cap the reset already ran); a re-answer keeps the cursor's file, a new query takes row 0.
    function land(start, items, kinds, fresh) {
        var reply = root.phase.listed
        var keep = root.pane.searchMode.length > 0 && start === 0 && !fresh && root.pane.rowFor(root.pane.cursorIndex)
        // A live answer on the base the rows already stand on keeps the folder sizes and thumbnails it lists again; the first answer and an exit start clean.
        var again = root.pane.searchLeaving !== true && root.pane.searchBase === reply.path
            && (root.pane.searchMode === Search.TYPING || root.pane.searchMode === Search.RESULTS)
        var sizes = DirSizes.inlined(again ? DirSizes.carried(root.pane.dirSizeState, root.pane.rows, root.pane.held, items, start) : null, items, start)
        var thumbs = again ? Thumbs.carried(root.pane.thumbState, root.pane.rows, root.pane.held, items, start) : null
        if (root.holding)
            root.release(Swap.landed(root.phase), "landed")
        else
            root.phase = Swap.landed(root.phase)
        // The rows go in before the count, so each delegate the count builds is built on its own row.
        root.pane.held = start
        root.pane.rows = RecentMode.stampRows(root.pane, items)
        root.pane.kindNames = kinds
        root.applyListed(reply.total, reply.readMs, reply.sortMs, reply.path, reply.changed)
        if (sizes)
            root.pane.dirSizeState = sizes
        if (thumbs)
            root.pane.thumbState = thumbs
        if (keep) {
            root.pane.setCursor(Search.keptIndex(keep.n, items, start))
        } else if (fresh) {
            root.pane.setCursor(start)
        }
        root.rowsLanded()
    }

    // A refused located window, any other failure or the wait running out lands the rows set aside, at the top, so the listing still ends.
    function landParked() {
        var parked = root.parked
        if (!parked || root.phase.listed === null)
            return false
        root.parked = null
        root.restore = null
        root.land(parked.start, parked.items, parked.kinds, false)
        return true
    }

    // A failed or refused listing, or a kept live answer, lets the held rows go at once; the failure draws what it always did.
    function drop() {
        if (root.holding)
            root.release(Swap.dropped(root.phase), "dropped")
        else
            root.phase = Swap.dropped(root.phase)
    }

    function release(next, end) {
        var query = root.phase.query
        root.last = { ms: Date.now() - root.phase.since, end: end }
        cap.stop()
        root.phase = next
        Nav.forget(root.pane, query)
    }

    function stopWaiting() {
        minShow.stop()
        root.waiting = []
    }

    function applyListed(total, readMs, sortMs, path, changed) {
        var pane = root.pane
        pane.thumbState = Thumbs.empty()
        pane.dirSizeState = DirSizes.empty()
        pane.reloadChanged = (changed === undefined || changed === null) ? -1 : changed
        if (!pane.dualMode && !pane.listInFlight && pane.searchMode.length === 0) {
            // With remember on each folder keeps its own order, so the fallback every other folder opens with stays untouched.
            if (ViewState.state.rememberSort === false)
                ViewState.changeLeaf("sort", { key: pane.backend.sortBy === "mtime" ? "date" : pane.backend.sortBy,
                                             reverse: pane.backend.sortDesc })
            pane.appliedListingPreferences = pane.listingPreferences
        }
        if (pane.listInFlight) {
            pane.listedSeen = true
        }
        pane.total = total
        // A live answer replaces rows in place with listed.path as the base in the same turn.
        if (pane.searchMode === Search.TYPING || pane.searchMode === Search.RESULTS) {
            pane.searchBase = path
            pane.listingState = Search.listingState(pane, total)
            pane.stateMessage = ""
            return
        }
        if (path.length > 0) pane.path = path  // the listing landed, so this is where the pane moves
        pane.listingState = total === 0 ? "empty" : "ready"
        pane.stateMessage = total === 0 ? "This folder is empty." : ""
        pane.opened(pane.path)
    }

    // Everything a rows reply does once its rows are in the pane, whether they came alone or with a swap.
    function rowsLanded() {
        // A landed live answer marks the phase a search, so an exit re-list forgets at once.
        if (root.pane.searchMode.length > 0) {
            root.phase = Swap.heard(root.phase, true)
        }
        var pane = root.pane
        if (pane.rowsAt === 0 && pane.inputAt > 0 && pane.rowFor(pane.cursorIndex))
            pane.rowsAt = Date.now()
        pane.applyPendingSelect()
        root.wire.anchor = Anchor.apply(pane, root.wire.anchor, Theme.fileRowHeight)
        if (pane.preferenceAnchor)
            pane.preferenceAnchor = Anchor.applyPreference(pane, pane.preferenceAnchor)
        // A manual reload's notice, said only when rows changed; every other listing owes none.
        Reload.landed(pane)
        Tabs.applyPending(pane)
        // The Back or Forward cursor lands with the rows, in this same turn, or not at all.
        BackReturn.landRestore(pane, root.restore)
        root.restore = null
        pane.listArea.restartSettle()
        if (pane.listInFlight) {
            pane.listInFlight = false
            pane.listedSeen = false
        }
        // Rows of another numbering under a standing query, by a swap or alone, ask the folder again, now that no listing is out (issue 315).
        FilterWire.relisted(pane, pane.backend.heldListing)
        root.wire.locateRetry()
        root.wire.openRenameOnArrival()
    }

    function describe() {
        return { holding: root.holding, fellBack: root.fellBack, holds: root.holds, fallbacks: root.fallbacks, waits: root.waits,
                 blankFrames: root.blankFrames, loadingFrames: root.loadingFrames, last: root.last, shownMs: root.shownMs }
    }

    // A slow listing falls back to the loading state, which stands until its rows land with their listed line.
    Timer {
        id: cap
        repeat: false
        onTriggered: {
            if (!root.holding)
                return
            root.fallbacks += 1
            root.release(Swap.expired(root.phase), "expired")
        }
    }

    // The mark has stood its minimum, so the rows that came meanwhile land now.
    Timer {
        id: minShow
        repeat: false
        onTriggered: {
            var rows = root.waiting
            root.waiting = []
            // Every line that arrived inside the minimum lands now, in the order it arrived.
            for (var i = 0; i < rows.length; i++)
                root.takeRows(rows[i].start, rows[i].items, rows[i].kinds, rows[i].listing, rows[i].gen)
        }
    }

    Connections {
        target: root.pane
        function onListInFlightChanged() {
            if (root.pane.listInFlight)
                return
            cap.stop()
            root.stopWaiting()
            root.restore = null
            root.parked = null
            root.phase = Swap.ended(root.phase)
        }
        function onSearchModeChanged() { if (root.pane.searchMode.length === 0) root.searchWarm = false }
    }

    // Runs one turn after the frame that drew the strip, so the strip never waits for what it builds.
    function warmSearch() { root.searchWarm = root.pane.searchMode.length > 0 }

    // afterAnimating runs on this thread just before each frame is synchronised, so it sees what that frame draws.
    Connections {
        target: root.Window.window
        function onAfterAnimating() {
            // Search and "/" never put a list in flight, so their blank is no row drawn while the count still names rows.
            var live = Search.live(root.pane) || root.pane.filterQuery.length > 0
            var kind = Swap.frameKind(root.pane.listInFlight, root.pane.listingState, root.fellBack,
                root.pane.rows.length, root.pane.total, live)
            if (kind === "blank")
                root.blankFrames += 1
            else if (kind === "loading")
                root.loadingFrames += 1
            if (root.pane.searchMode.length > 0 && !root.searchWarm)
                Qt.callLater(root.warmSearch)
        }
    }
}
