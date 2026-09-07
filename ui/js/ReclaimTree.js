.pragma library

// One scan, eight readings. The reclaim listing's rows — each a regenerable tree with its
// measured bytes in s and its mtime in m — build one tree, and every view in ui/ReclaimMap.qml
// renders that tree. The layouts here are pure: they take the tree and a box and answer shapes,
// and ui/js/ReclaimPaint.js is the only thing that draws. A shape carries the listing row it was
// built from, so a click on any view lands the cursor on the row it drew.
//
// The views read the window the pane holds, which is the whole listing whenever a scan fits in
// one, because a window is what the viewport rule lets the client hold.

var MONTH = 2592000
var YEAR = 31536000
var MODES = ["treemap", "folders", "sunburst", "flame", "bubbles", "mindmap", "topsizes", "agemap"]
var LABELS = {
    "treemap": "Treemap",
    "folders": "Folders",
    "sunburst": "Sunburst",
    "flame": "Flame",
    "bubbles": "Bubbles",
    "mindmap": "Mind Map",
    "topsizes": "Top Sizes",
    "agemap": "Age Map"
}

// One node per path segment, so a monorepo's apps/web/node_modules nests three deep and its
// aggregates come free: an internal node's bytes are the sum of what is under it.
function build(rows, held, rootPath) {
    var root = { name: leafOf(rootPath), path: "", bytes: 0, m: 0, children: [], leaf: false, row: -1, depth: 0 }
    for (var i = 0; i < rows.length; i++) {
        var r = rows[i]
        if (!r.d) {
            continue
        }
        var segs = String(r.n).split("/")
        var node = root
        var acc = ""
        for (var s = 0; s < segs.length; s++) {
            acc = acc.length === 0 ? segs[s] : acc + "/" + segs[s]
            var next = null
            for (var c = 0; c < node.children.length; c++) {
                if (node.children[c].name === segs[s]) {
                    next = node.children[c]
                    break
                }
            }
            if (next === null) {
                next = { name: segs[s], path: acc, bytes: 0, m: 0, children: [], leaf: false, row: -1, depth: s + 1 }
                node.children.push(next)
            }
            node = next
        }
        node.leaf = true
        node.bytes = r.s > 0 ? r.s : 0
        node.m = r.m || 0
        node.row = held + i
    }
    total(root)
    return root
}

function leafOf(path) {
    var text = String(path)
    var cut = text.lastIndexOf("/")
    return cut < 0 || cut === text.length - 1 ? text : text.substring(cut + 1)
}

function total(node) {
    if (node.leaf) {
        return node.bytes
    }
    var t = 0
    for (var c = 0; c < node.children.length; c++) {
        t += total(node.children[c])
    }
    node.bytes = t
    return t
}

function leaves(node, out) {
    if (out === undefined) {
        out = []
    }
    if (node.leaf) {
        out.push(node)
        return out
    }
    for (var c = 0; c < node.children.length; c++) {
        leaves(node.children[c], out)
    }
    return out
}

function layout(mode, root, w, h, now) {
    if (mode === "treemap") return treemap(root, 0, 0, w, h, [])
    if (mode === "sunburst") return sunburst(root, w / 2, h / 2, Math.min(w, h) / 2 - 6, [])
    if (mode === "flame") return flame(root, 0, 4, w, 18, 3, [])
    if (mode === "bubbles") return bubbles(root, w, h, [])
    if (mode === "mindmap") return mindmap(root, w / 2, h / 2, Math.min(w, h) / 2 - 10, [])
    if (mode === "topsizes") return topsizes(root, 8, 6, w - 16, h - 12, [])
    if (mode === "folders") return folders(root, 6, 6, w - 12, h - 12, [])
    if (mode === "agemap") return agemap(root, 10, 6, w - 20, h - 12, now, [])
    return []
}

// Squarified, one header strip per level, three levels deep: a scan nests deeper than that only
// through monorepos, and the listing itself is one click away for the rest.
function treemap(node, x, y, w, h, out) {
    out.push({ k: "rect", x: x, y: y, w: w, h: h, node: node, depth: node.depth })
    if (node.depth < 3 && node.children.length > 0) {
        var kids = node.children.slice().sort(function (a, b) { return b.bytes - a.bytes })
        squarify(kids, x + 1, y + 14, w - 2, h - 15, out)
    }
    out.push({ k: "label", x: x + 4, y: y + 11, w: Math.max(0, w - 8), node: node })
    return out
}

function squarify(items, x, y, w, h, out) {
    var total = 0
    for (var i = 0; i < items.length; i++) {
        total += items[i].bytes
    }
    if (total <= 0 || w <= 2 || h <= 2) {
        return
    }
    var rest = items.slice()
    while (rest.length > 0) {
        var side = Math.min(w, h)
        var row = [rest.shift()]
        var rowBytes = row[0].bytes
        while (rest.length > 0 && rowBytes > 0) {
            if (worst(row, rowBytes, side, total) < worst(row.concat([rest[0]]), rowBytes + rest[0].bytes, side, total)) {
                break
            }
            rowBytes += rest[0].bytes
            row.push(rest.shift())
        }
        var frac = rowBytes / total
        var across = w >= h
        var rw = across ? w * frac : w
        var rh = across ? h : h * frac
        var off = 0
        for (var r = 0; r < row.length; r++) {
            var share = rowBytes > 0 ? row[r].bytes / rowBytes : 0
            var ix = across ? x : x + off
            var iy = across ? y + off : y
            var iw = across ? rw : rw * share
            var ih = across ? rh * share : rh
            treemap(row[r], ix, iy, Math.max(1, iw), Math.max(1, ih), out)
            off += across ? ih : iw
        }
        if (across) {
            x += rw
            w -= rw
        } else {
            y += rh
            h -= rh
        }
        total -= rowBytes
    }
}

function worst(row, rowBytes, side, total) {
    if (rowBytes <= 0) {
        return Infinity
    }
    var t = side * (rowBytes / total)
    if (t <= 0) {
        return Infinity
    }
    var worst = 1
    for (var i = 0; i < row.length; i++) {
        var d = side * (row[i].bytes / rowBytes)
        if (d <= 0) {
            return Infinity
        }
        var ratio = d > t ? d / t : t / d
        if (ratio > worst) {
            worst = ratio
        }
    }
    return worst
}

// Concentric rings, one per depth, angle by bytes. The root is the disc at the middle.
function sunburst(node, cx, cy, rmax, out) {
    out.push({ k: "disc", cx: cx, cy: cy, r: 13, node: node })
    ring(node, cx, cy, 18, (rmax - 18) / 3, 0, 3, -Math.PI / 2, Math.PI * 2, out)
    return out
}

function ring(node, cx, cy, r0, thick, depth, maxDepth, a0, span, out) {
    if (depth >= maxDepth || node.children.length === 0) {
        return
    }
    var a = a0
    for (var c = 0; c < node.children.length; c++) {
        var ch = node.children[c]
        var s = node.bytes > 0 ? span * (ch.bytes / node.bytes) : 0
        if (s > 0.004) {
            out.push({ k: "arc", cx: cx, cy: cy, r0: r0, r1: r0 + thick, a0: a, a1: a + s, node: ch, depth: depth })
            ring(ch, cx, cy, r0 + thick, thick, depth + 1, maxDepth, a, s, out)
        }
        a += s
    }
}

// Depth stacks downward, width by bytes inside the parent's own width: the scan's call stack.
function flame(node, x, y, w, rowH, maxDepth, out) {
    out.push({ k: "rect", x: x, y: y, w: Math.max(1, w), h: rowH - 2, node: node, depth: node.depth === 0 ? 1 : node.depth })
    out.push({ k: "label", x: x + 4, y: y + 11, w: Math.max(0, w - 8), node: node })
    if (node.depth >= maxDepth || node.children.length === 0) {
        return
    }
    var off = 0
    for (var c = 0; c < node.children.length; c++) {
        var ch = node.children[c]
        var cw = node.bytes > 0 ? w * (ch.bytes / node.bytes) : 0
        if (cw >= 2) {
            flame(ch, x + off, y + rowH, cw, rowH, maxDepth, out)
        }
        off += cw
    }
    return out
}

// Circles sized by area, biggest first, packed in rows so two big trees can never overlap.
function bubbles(root, w, h, out) {
    var items = leaves(root).slice().sort(function (a, b) { return b.bytes - a.bytes })
    var total = root.bytes
    var x = 10, y = 10, rowH = 0
    var cap = Math.max(24, Math.min(w, h) / 3)
    for (var i = 0; i < items.length; i++) {
        var it = items[i]
        var r = total > 0 ? cap * Math.sqrt(it.bytes / total) : 3
        if (r < 3) {
            r = 3
        }
        if (x + 2 * r > w - 10 && x > 10) {
            x = 10
            y += rowH + 10
            rowH = 0
        }
        if (y + 2 * r > h - 10) {
            r = Math.max(3, (h - 10 - y) / 2)
        }
        out.push({ k: "dot", cx: x + r, cy: y + r, r: r, node: it, depth: 0 })
        if (r >= 11) {
            out.push({ k: "clabel", cx: x + r, cy: y + r, w: r * 1.7, node: it })
        }
        x += 2 * r + 8
        if (2 * r > rowH) {
            rowH = 2 * r
        }
    }
    return out
}
// The root at the middle, its trees around it, each heavy tree's own biggest children beyond it.
function mindmap(root, cx, cy, R, out) {
    out.push({ k: "disc", cx: cx, cy: cy, r: 15, node: root })
    out.push({ k: "clabel", cx: cx, cy: cy, w: 64, node: root })
    var kids = root.children.slice().sort(function (a, b) { return b.bytes - a.bytes })
    for (var i = 0; i < kids.length; i++) {
        var a = (i / kids.length) * Math.PI * 2 - Math.PI / 2
        var x = cx + Math.cos(a) * R * 0.6
        var y = cy + Math.sin(a) * R * 0.6
        out.push({ k: "line", x1: cx, y1: cy, x2: x, y2: y })
        out.push({ k: "dot", cx: x, cy: y, r: bubbleR(kids[i], root, 16), node: kids[i], depth: 1 })
        out.push({ k: "clabel", cx: x, cy: y, w: 84, node: kids[i] })
        var grand = kids[i].children.slice().sort(function (a, b) { return b.bytes - a.bytes }).slice(0, 3)
        for (var g = 0; g < grand.length; g++) {
            var ga = a + (g - (grand.length - 1) / 2) * 0.4
            var gx = cx + Math.cos(ga) * R
            var gy = cy + Math.sin(ga) * R
            out.push({ k: "line", x1: x, y1: y, x2: gx, y2: gy })
            out.push({ k: "dot", cx: gx, cy: gy, r: 3, node: grand[g], depth: 2 })
        }
    }
    return out
}

function bubbleR(node, parent, cap) {
    return parent.bytes > 0 ? Math.max(3, Math.min(cap, 16 * Math.sqrt(node.bytes / parent.bytes))) : 3
}

// The ranked listing drawn as bars: heaviest at the top, the number at the end of each bar.
function topsizes(root, x, y, w, h, out) {
    var items = leaves(root).slice().sort(function (a, b) { return b.bytes - a.bytes })
    var max = 0
    for (var i = 0; i < items.length; i++) {
        if (items[i].bytes > max) {
            max = items[i].bytes
        }
    }
    var n = Math.min(items.length, Math.max(1, Math.floor(h / 20)))
    for (var b = 0; b < n; b++) {
        var it = items[b]
        var bw = max > 0 ? Math.max(2, w * 0.55 * (it.bytes / max)) : 2
        out.push({ k: "rect", x: x, y: y + b * 20, w: bw, h: 14, node: it, depth: 1 })
        out.push({ k: "slabel", x: x + bw + 6, y: y + b * 20 + 11, w: Math.max(0, w - bw - 6), node: it })
    }
    return out
}

// Folder cards in a grid, name and bytes, the listing's own order.
function folders(root, x, y, w, h, out) {
    var items = leaves(root)
    var cols = Math.max(1, Math.floor(w / 170))
    var cw = w / cols
    var ch = Math.min(56, Math.max(40, h / Math.max(1, Math.ceil(items.length / cols)) - 6))
    for (var i = 0; i < items.length; i++) {
        var cx = x + (i % cols) * cw
        var cy = y + Math.floor(i / cols) * (ch + 6)
        if (cy + ch > y + h) {
            break
        }
        out.push({ k: "rect", x: cx, y: cy, w: cw - 8, h: ch, node: items[i], depth: 1 })
        out.push({ k: "clabel", cx: cx + (cw - 8) / 2, cy: cy + ch / 2 - 5, w: cw - 18, node: items[i] })
        out.push({ k: "slabel", x: cx + 8, y: cy + ch - 8, w: cw - 16, node: items[i] })
    }
    return out
}

// Bytes by how long ago the tree was last touched, in twelve buckets across the scan's age span,
// then the big and untouched — over a year old — listed under the chart.
function agemap(root, x, y, w, h, now, out) {
    var items = leaves(root)
    var maxAge = 0
    for (var i = 0; i < items.length; i++) {
        var age = now - items[i].m
        if (age > maxAge) {
            maxAge = age
        }
    }
    var span = Math.min(Math.max(maxAge, MONTH), YEAR * 2) * 1.05
    var buckets = 12
    var sums = []
    for (var b = 0; b < buckets; b++) {
        sums.push(0)
    }
    var maxBytes = 0
    for (var i2 = 0; i2 < items.length; i2++) {
        var bi = Math.floor((now - items[i2].m) / span * buckets)
        if (bi < 0) {
            bi = 0
        }
        if (bi >= buckets) {
            bi = buckets - 1
        }
        sums[bi] += items[i2].bytes
        if (sums[bi] > maxBytes) {
            maxBytes = sums[bi]
        }
    }
    var chartH = h * 0.45
    var bw = w / buckets
    for (var b3 = 0; b3 < buckets; b3++) {
        var bh = maxBytes > 0 ? chartH * (sums[b3] / maxBytes) : 0
        out.push({ k: "rect", x: x + b3 * bw + 2, y: y + chartH - bh, w: bw - 4, h: Math.max(bh, 1), node: null, depth: b3 + 1 })
        out.push({ k: "text", x: x + b3 * bw + 2, y: y + chartH + 11, text: ageLabel(span * (b3 + 0.5)) })
    }
    var untouched = []
    for (var u = 0; u < items.length; u++) {
        if (now - items[u].m > YEAR) {
            untouched.push(items[u])
        }
    }
    untouched.sort(function (a, b) { return b.bytes - a.bytes })
    var lines = Math.min(untouched.length, Math.max(0, Math.floor((h - chartH - 30) / 16)))
    if (untouched.length > 0) {
        out.push({ k: "text", x: x, y: y + chartH + 28, text: "Big & untouched — over a year old:" })
    }
    for (var v = 0; v < lines; v++) {
        out.push({ k: "slabel", x: x, y: y + chartH + 44 + v * 16, w: w, node: untouched[v] })
    }
    return out
}

function ageLabel(seconds) {
    var days = Math.round(seconds / 86400)
    if (days < 31) {
        return days + "d"
    }
    return Math.round(days / 30) + "mo"
}
