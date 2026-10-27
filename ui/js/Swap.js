.pragma library

// The listing swap's decisions, see AGENTS.md "The listing swap"; ui/PaneSwap.qml holds the state and the timer.

// Both swaps' cap, the pane mark's hold-off and a PDF page render's cap: with four 120 Hz frames for input, the Timer and the draw, the mark shows by 100 ms.
var CAP_MS = 66

// The hold-off of a loading state nothing keeps MIN_SHOW_MS: LoadingState outside the pane, the folder fallback, a search walk's mark.
var HOLD_MS = 150

// A drawn loading state stands this long before content replaces it; under 100 ms the two changes read as one blink.
var MIN_SHOW_MS = 100

// Answered while a listing is out: the navigations take its place, escape touches no row, the rest change the window.
var ANSWERED_WHILE_LISTING = ["open", "parent", "historyBack", "historyForward", "escape",
                              "viewList", "viewColumns", "viewGrid", "viewDual", "sidebar", "windowNew", "togglePreview",
                              "reload"]

// What a swallowed key says, the sentence every refused navigation already gives.
var LOADING = "A directory is already loading."

// What a listed or rows line does: dropped, kept for the rows behind it, applied alone, or landed with the kept line.
var DROP = "drop"
var KEEP = "keep"
var APPLY = "apply"
var LAND = "land"
var WAIT = "wait"

function idle() {
    return { holding: false, listed: null, fellBack: false, waits: false, walk: false, query: "", since: 0, relist: false }
}

// Only a settled listing is held, plus a walk's drawn state for a listing after it; see ui/PaneSwap.qml hold.
function begin(state, listingState, searchMode, ask, now) {
    // The exit holds the whole search state, strip and rows, until the folder lands in one turn.
    var exit = ask.holdWalk === true
    var afterWalk = state.walk === true && !exit
    var holding = ask.clearAtOnce !== true && listingState !== "loading" && (searchMode === "" || exit) && !afterWalk
    // An unheld Back or Forward still keeps its listed line, so the count, the rows and the cursor land in one turn.
    var waits = !holding && !!ask.restore && String(ask.restore.name || "").length > 0
    return { holding: holding, listed: null, fellBack: false, waits: waits, walk: state.walk, query: ask.keptQuery || "", since: now,
             relist: ask.relist === true }
}

// A session line lands only on its open session's newest sent gen; a closed exit's lines never land either.
function dropSessionLine(searchMode, gen, searchGen, leaving, openedGen) {
    if (gen > 0 && leaving === true) {
        return true
    }
    return gen > 0 && ((searchMode || "").length === 0 || gen !== searchGen || gen <= (openedGen || 0))
}

// A live answer holds old rows until its own rows land, no listing out.
function keepLive(searchMode, inFlight) {
    return (searchMode || "").length > 0 && inFlight !== true
}

// While a list is out, only a listed line naming the directory asked for answers it; an earlier sort's or search's is dropped.
// Held rows, a fallback or a loading mark already drawn keep it for the rows, so the mark never gives way to a count over none.
function onListed(state, inFlight, path, asked, marked) {
    if (inFlight === true && path.length > 0 && path !== asked)
        return DROP
    return state.holding || state.fellBack || state.waits || marked === true ? KEEP : APPLY
}

// A rows line ahead of its listed line answers a listing already replaced; one behind a kept listed line lands with it,
// once the loading mark it replaces has stood waitMs more.
function onRows(state, inFlight, listedSeen, waitMs) {
    if (inFlight === true && listedSeen !== true)
        return DROP
    if (state.listed === null)
        return APPLY
    return waitMs > 0 ? WAIT : LAND
}

// How much longer a loading state drawn at since stands before it may go; 0 when none is drawn.
function showLeft(since, now) {
    return since > 0 ? Math.max(0, since + MIN_SHOW_MS - now) : 0
}

// How long a held listing begun at state.since still holds before it falls back; -1 when nothing is held.
function capLeft(state, now) {
    return state.holding === true ? Math.max(0, state.since + CAP_MS - now) : -1
}

// The pane mark's hold-off: a directory listing keeps its mark MIN_SHOW_MS, so it draws at the cap; a walk's does not.
function markHoldOff(searchMode) {
    return (searchMode || "") === "" ? CAP_MS : HOLD_MS
}

// A directory listing out under a drawn mark; a walk's or a search's matches stream into a loading pane, never waiting on it.
function marked(since, inFlight, searchMode, walk) {
    return since > 0 && inFlight === true && (searchMode || "") === "" && walk !== true
}

function kept(state, listed) {
    return Object.assign({}, state, { listed: listed })
}

// Every listed line that is applied rather than kept says whether the rows it brings are a walk's.
function heard(state, walk) {
    return Object.assign({}, state, { walk: walk === true })
}

// A held listing is only ever a directory's, which is what begin() asked for.
function landed(state) {
    return Object.assign({}, state, { holding: false, listed: null, fellBack: false, waits: false, walk: false, relist: false })
}

// The cap ran out: the pane shows the loading state until the rows land, and a listed line already kept waits for them.
function expired(state) {
    return Object.assign({}, state, { holding: false, fellBack: true })
}

// A failed or refused listing ends the hold without a swap; the failure draws what it always did.
function dropped(state) {
    return Object.assign({}, state, { holding: false, listed: null, fellBack: false, waits: false, relist: false })
}

// The listing is no longer out, however it ended.
function ended(state) {
    return Object.assign({}, state, { holding: false, listed: null, fellBack: false, waits: false, relist: false })
}

// A stale, window or filterwindow refusal ended only its own request, and so does a scan failure naming a folder no longer asked for; any other failure ends the listing, letting held rows go while it is still out.
// A filter window's stat on a stuck mount can fail seconds after the user went elsewhere, so it must not end the listing out for the new folder (issue 315).
function failListing(pane, where, path) {
    if (where === "stale" || where === "window" || where === "filterwindow")
        return false
    if (where === "scan" && !!path && path !== pane.listingPath)
        return false
    // An Esc relist that died leaves no cursor file armed for whatever listing comes next.
    if (pane.swap.phase.relist === true)
        pane.pendingSelect = ""
    pane.swap.drop()
    pane.listInFlight = false
    pane.listedSeen = false
    return true
}

// Sample input: {"c":"transfer","op":"move","rows":[3],"dest":"/home/gm/omarchy"} with held 7 gains "listing":7, unless it names one.
function named(request, held) {
    if (request.rows === undefined || request.listing !== undefined || !(held > 0))
        return request
    return Object.assign({}, request, { listing: held })
}

// A key swallowed while a listing is out, held or loading: the backend already numbers rows for the directory asked for.
function swallows(inFlight, action) {
    return inFlight === true && action.length > 0 && ANSWERED_WHILE_LISTING.indexOf(action) < 0
}

// A re-sort renumbers every row without a new list, so a selection made while it was in flight names other files once the new numbering lands.
function clearsSelection(oldHeld, listing) {
    return oldHeld > 0 && listing > 0 && listing !== oldHeld
}

// A blank frame: a listing out and loading before the cap, or no row drawn while a live count still names rows.
function frameKind(inFlight, listingState, fellBack, drawn, named, live) {
    if (inFlight === true && listingState === "loading") {
        return fellBack === true ? "loading" : "blank"
    }
    if (live === true && drawn === 0 && named > 0) {
        return "blank"
    }
    return ""
}
