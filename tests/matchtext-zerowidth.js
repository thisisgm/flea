// Probes for tests/matchtext-zerowidth.qml: each reads the live objects a view holds, after the frame that grabbed it.

// The first MatchText under item that draws text, or null.
function findMatch(item, text) {
    var stack = [item]
    while (stack.length > 0) {
        var o = stack.pop()
        if (o.elideMiddle !== undefined && o.matchStart !== undefined && o.text === text) return o
        var kids = o.children !== undefined ? o.children : []
        for (var i = 0; i < kids.length; i++) stack.push(kids[i])
    }
    return null
}

// The row that owns a MatchText: the nearest ancestor that carries a row.
function rowOf(item) {
    var o = item.parent
    while (o !== null && o.row === undefined) o = o.parent
    return o
}

// Every Text under mt that holds text, as its x inside mt, its width, what it paints, and whether it is visible.
function runsOf(mt) {
    var out = []
    var stack = [mt]
    while (stack.length > 0) {
        var o = stack.pop()
        var kids = o.children !== undefined ? o.children : []
        for (var i = 0; i < kids.length; i++) stack.push(kids[i])
        if (o !== mt && o.maximumLineCount !== undefined && o.text.length > 0) {
            out.push({ text: o.text, x: o.mapToItem(mt, 0, 0).x, w: o.width, painted: o.paintedWidth, vis: o.visible })
        }
    }
    return out
}

// The slot and every run under mt, for the verdict line: each run as text@x+width/painted, hidden ones marked.
function infoOf(mt) {
    var parts = []
    var runs = runsOf(mt)
    for (var i = 0; i < runs.length; i++) {
        var r = runs[i]
        parts.push(r.text + (r.vis ? "" : "(hidden)") + "@" + Math.round(r.x) + "+" + Math.round(r.w) + "/" + Math.round(r.painted))
    }
    return "slot=" + Math.round(mt.width) + " runs=[" + parts.join(" | ") + "]"
}

// A run that draws must have a width, paint inside it, and sit inside the name slot.
function runFails(mt) {
    var fails = []
    var runs = runsOf(mt).filter(function (r) { return r.vis })
    if (runs.length === 0) fails.push("no run drawn")
    for (var i = 0; i < runs.length; i++) {
        var r = runs[i]
        if (r.w <= 0) fails.push("'" + r.text + "' drawn at width " + Math.round(r.w) + " paints " + Math.round(r.painted))
        else if (r.painted > r.w + 1) fails.push("'" + r.text + "' paints " + Math.round(r.painted) + " past width " + Math.round(r.w))
        if (r.x < -0.5 || r.x + r.w > mt.width + 0.5) fails.push("'" + r.text + "' outside the name slot")
    }
    return fails
}

// The marked name under item, read in view: its run failures, and the box of its row from the name's right edge on.
function markGeom(item, view, text) {
    var mt = findMatch(item, text)
    if (mt === null || !mt.visible || mt.width <= 0) return { fails: ["no MatchText draws the name"], x0: 0, x1: 0, y0: 0, y1: 0 }
    var row = rowOf(mt)
    var at = row.mapToItem(view, 0, 0)
    return {
        info: infoOf(mt),
        fails: runFails(mt),
        x0: Math.floor(mt.mapToItem(view, mt.width, 0).x),
        x1: Math.floor(at.x + row.width),
        y0: Math.floor(at.y),
        y1: Math.ceil(at.y + row.height)
    }
}

// The box of a row with no mark, from x0 on, so it compares cell for cell with a marked row.
function plainBox(item, view, x0) {
    var at = item.mapToItem(view, 0, 0)
    return { x0: x0, x1: Math.floor(at.x + item.width), y0: Math.floor(at.y), y1: Math.ceil(at.y + item.height) }
}

function boxOf(g) { return { x0: g.x0, x1: g.x1, y0: g.y0, y1: g.y1 } }

function inside(g, box) { return box.x0 >= 0 && box.y0 >= 0 && box.x1 <= g.w && box.y1 <= g.h }

// Pixels whose channels moved more than tol between box a of grab ga and box b of grab gb. -1 when a grab is missing, -2 for an empty box, a box off its image, or boxes of different sizes.
function diff(ga, a, gb, b, tol) {
    if (!ga || !gb) return -1
    var w = a.x1 - a.x0, h = a.y1 - a.y0
    if (w <= 0 || h <= 0 || w !== b.x1 - b.x0 || h !== b.y1 - b.y0 || !inside(ga, a) || !inside(gb, b)) return -2
    var n = 0
    for (var dy = 0; dy < h; dy++) {
        for (var dx = 0; dx < w; dx++) {
            var oa = ((a.y0 + dy) * ga.w + a.x0 + dx) * 4
            var ob = ((b.y0 + dy) * gb.w + b.x0 + dx) * 4
            var m = Math.max(Math.abs(ga.px[oa] - gb.px[ob]), Math.abs(ga.px[oa + 1] - gb.px[ob + 1]), Math.abs(ga.px[oa + 2] - gb.px[ob + 2]))
            // NaN from an unread pixel counts as a difference, never as a match.
            if (!(m <= tol)) n += 1
        }
    }
    return n
}
