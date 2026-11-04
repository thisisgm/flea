.pragma library

// Cursor preview loads stamp distinct moves: due(200, 0, 120) and due(100, 200, 120) load now; due(50, 0, 120) trails.
function due(now, last, interval) {
    var elapsed = now - last
    return elapsed >= interval || elapsed < 0
}

// A settle armed for this row before its move reached the clock (ColumnsArea's folder to file step) is left to the clock.
// plan(50, 0, 120, "b", "b", true) keeps the pending timer; plan(1000, 0, 120, "c", "b", true, false) is "now".
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
