.pragma library

// Cursor preview loads stamp distinct moves: due(200, 0, 120) and due(100, 200, 120) load now; due(50, 0, 120) trails.
function due(now, last, interval) {
    var elapsed = now - last
    return elapsed >= interval || elapsed < 0
}

// plan(50, 0, 120, "b", "b", true) keeps the pending timer; callers suppress shown duplicates by identity before scheduling.
// Pending names a cursor that has not rested since its last move (a settle still pending for an earlier row, or the rest of a load made at once),
// however late a busy loop delivers the next move: plan(1000, 0, 120, "c", "b", false, true) trails.
// A settle armed for this row before its move reached the clock (ColumnsArea's folder to file step) leaves it to the clock: plan(1000, 0, 120, "c", "b", true, false) is "now".
function plan(now, last, interval, key, lastKey, armedForKey, pending) {
    if (key === lastKey && armedForKey === true)
        return "same"
    if (pending === true)
        return "later"
    return due(now, last, interval) ? "now" : "later"
}

// A PDF opens at once only when the cursor is idle and no page render is in flight.
function pdfOpenNow(now, lastRequestAt, intervalMs, renderLoading) {
    return renderLoading !== true && due(now, lastRequestAt, intervalMs)
}

// A lone selection never keys on the version Marks.follow bumps on every plain move.
function selectionKey(count, version) {
    if (count <= 1)
        return ""
    return count + ":" + version
}
