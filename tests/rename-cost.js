.pragma library

// The rename cost test's cursor rows, row field counters, delegate snapshots and phase matching, taking the test's root, its pane or the window's content item.

function indexOfName(pane, name) {
    for (var i = 0; i < pane.total; i++)
        if (pane.rowFor(i) && String(pane.rowFor(i).n) === name) return i
    return -1
}
// The row a far rename lands on: every row before the held window sorts below a far name, so the landing row is the held rows sorting below the new name, minus the renamed row itself.
function farHome(pane, newName) {
    var below = pane.held
    for (var i = 0; i < pane.rows.length; i++)
        if (String(pane.rows[i].n) < newName) below++
    return below - 1
}
// The before phase that saw the same rests and planner passes as an after phase, the one with the fewest row reads; null when none did.
function matching(befores, b) {
    var best = null
    for (var i = 0; i < befores.length; i++) {
        var a = befores[i]
        if (a.keys.rests === b.keys.rests && a.keys.plans === b.keys.plans && (best === null || a.keys.rowReads < best.keys.rowReads)) best = a
    }
    return best
}
function idleWork(phases) { return phases.map(function (p) { return p.keys.rests + "/" + p.keys.plans }).join(" ") }
// A key that clamps at an edge leaves the walk off its start row, so a repeat phase moves the cursor back a tick before its idle second, as the first phase's own move did.
function homed(root) {
    if (root.pane.cursorIndex === root.home) return true
    root.pane.setCursor(root.home)
    return false
}
// rowFor is a QML method and cannot be wrapped, so every field a held row gives out is counted instead; a rows reply arms its rows on arrival, snapshots arm the rest.
function armRows(root) {
    var armed = 0
    for (var i = 0; i < root.pane.rows.length; i++)
        for (var field in root.pane.rows[i])
            if (armField(root, root.pane.rows[i], field)) armed++
    return armed
}
function armItems(root, items) {
    var armed = 0
    for (var i = 0; i < items.length; i++)
        for (var field in items[i])
            if (armField(root, items[i], field)) armed++
    return armed
}
function armField(root, row, field) {
    var desc = Object.getOwnPropertyDescriptor(row, field)
    if (desc.get !== undefined) return false
    var value = row[field]
    Object.defineProperty(row, field, { get: function () { root.tally.rowReads++; return value }, enumerable: true, configurable: true })
    return true
}
// Every row delegate under the window, pooled ones included: List's Row, the Grid's tile and Columns' ColumnRow carry listingIndex.
function cellSnap(top) {
    var out = { items: [], at: [] }
    var stack = [top]
    while (stack.length > 0) {
        var o = stack.pop()
        if (o === null || o === undefined) continue
        if (o.listingIndex !== undefined) {
            out.items.push(o)
            out.at.push(o.index)
            continue
        }
        var kids = o.children !== undefined ? o.children : []
        for (var i = 0; i < kids.length; i++) stack.push(kids[i])
    }
    return out
}
// Delegates built since the last snapshot, destroyed since, and kept ones that now draw another position (a reuse rebinds them).
function cellDelta(a, b) {
    var out = { built: 0, destroyed: 0, rebound: 0 }
    for (var i = 0; i < b.items.length; i++) {
        var was = a.items.indexOf(b.items[i])
        if (was < 0) out.built++
        else if (a.at[was] !== b.at[i]) out.rebound++
    }
    for (var j = 0; j < a.items.length; j++)
        if (b.items.indexOf(a.items[j]) < 0) out.destroyed++
    return out
}
