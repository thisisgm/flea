.pragma library

// Which columns a list row of a given width draws. The name is the one column a file manager
// cannot do without, so it is the one that never loses: each metadata column drops instead, at
// the width where keeping it would push the name under its floor. ui/Header.qml and ui/Row.qml
// both resolve their set here, from the same width, so the header can never head a column the
// rows below it are not drawing.

// The optional columns, widest first, which is the order they drop in. Kind goes first: the row
// already marks its kind with a glyph and most names carry the extension, so it is the most
// redundant column as well as the widest. Mode goes last, because it is the permanent column.
var DROP_ORDER = ["kind", "date", "size", "mode"]

// The row width each optional column needs before it is drawn, keyed by column. t carries the
// tokens ui/Theme.qml resolved: rowPaddingX, gap, iconSize, nameMin, and one width per column.
// A floor includes every column that outlives it, because they drop in order and a wider column
// never survives a narrower one, so the floors nest; a hidden column draws nothing and adds none (#333).
function floors(t, hidden) {
    // The name's own slot at its floor: the row padding either side, the mark and the gap after it.
    var running = t.rowPaddingX + t.iconSize + t.gap + t.nameMin + t.rowPaddingX
    var out = {}
    for (var i = DROP_ORDER.length - 1; i >= 0; i--) {
        var key = DROP_ORDER[i]
        if ((hidden || []).indexOf(key) < 0) running += t[key] + t.gap
        out[key] = running
    }
    return out
}

// One boolean per optional column, for a row of this width. hidden is the user's own set (keys
// "mode"/"size"/"date"/"kind", from qs module ViewState), subtracted from what the width
// affords: a hidden column never draws, and width still wins over a column the user wants back,
// so the name cannot be crowded out by a column the pane is too narrow to carry.
function set(width, t, hidden) {
    var f = floors(t, hidden)
    var h = {}
    var list = hidden || []
    for (var i = 0; i < list.length; i++) h[list[i]] = true
    return {
        location: false,
        mode: width >= f.mode && !h["mode"],
        size: width >= f.size && !h["size"],
        date: width >= f.date && !h["date"],
        kind: width >= f.kind && !h["kind"]
    }
}

// DualPane protects its name floor without reserving absent Mode and Kind columns.
function dualSet(width, t, hidden) {
    var base = 2 * t.rowPaddingX + t.iconSize + t.gap + t.nameMin
    var showSize = (hidden || []).indexOf("size") < 0 && width >= base + t.size
    var showDate = (hidden || []).indexOf("date") < 0
        && width >= base + (showSize ? t.size : 0) + t.date
    return {mode: false, kind: false, location: false, size: showSize, date: showDate}
}

// Recent protects its pane's name floor and reserves every non-hidden metadata width for Location.
function recentSet(width, t, hidden, dual) {
    var base = 2 * t.rowPaddingX + t.iconSize + t.gap + t.nameMin
    var metadataGap = dual ? 0 : t.gap
    var h = hidden || []
    var showSize = h.indexOf("size") < 0 && width >= base + t.size + metadataGap
    var sizeSlot = showSize ? t.size + metadataGap : 0
    var showDate = h.indexOf("date") < 0 && width >= base + sizeSlot + t.date + metadataGap
    var locationFloor = base + (h.indexOf("size") < 0 ? t.size + metadataGap : 0)
        + (h.indexOf("date") < 0 ? t.date + metadataGap : 0) + t.location + t.gap
    return {mode: false, kind: false,
        size: showSize,
        date: showDate,
        location: width >= locationFloor}
}

// A peeked reply is keyed by what ordered it, so a stale ancestor column never survives the hidden-last toggle: path plus hidden plus hiddenLast.
// Sample input: peekKey("/a", true, true) is "/a\n11".
function peekKey(path, hidden, hiddenLast) {
    return String(path) + "\n" + (hidden === true ? "1" : "0") + (hiddenLast === true ? "1" : "0")
}

// The folder's own remembered order a column asks under and its reply echoes: "" for name order, else the key and 1 when descending.
// Sample input: orderTag("size", true) is "size1", orderTag("", false) is "".
function orderTag(by, desc) {
    return by ? String(by) + (desc === true ? "1" : "0") : ""
}

// The row count and the folder order join the key only for an outstanding ask, so a resize or a re-sort never orphans a stored column.
// Sample input: sentKey("/a\n00", 35) is "/a\n00\n35", and sentKey("/a\n00", 35, "size1") is "/a\n00\n35\nsize1".
function sentKey(key, first, order) {
    return String(key) + "\n" + Math.floor(Number(first)) + (order ? "\n" + String(order) : "")
}

// An ask this view made and still awaits, counted per sentKey (path, flags, row count, order), so another client's reply never lands in its column and a column re-asked in flight lands both replies.
// Sample input: trackAsk(trackAsk({}, "/a\n10\n35"), "/a\n10\n35") holds that key twice.
function trackAsk(pending, key) {
    var next = {}
    var src = pending || {}
    for (var k in src) next[k] = src[k]
    next[String(key)] = (next[String(key)] || 0) + 1
    return next
}

// True only for a reply this view asked for; anything else is another client's.
// Sample input: hasAsk(trackAsk({}, "/a\n10"), "/a\n10") is true.
function hasAsk(pending, key) {
    return ((pending || {})[String(key)] || 0) > 0
}

// One ask answered or superseded; a new listing drops them all.
// Sample input: dropAsk(trackAsk({}, "/a\n10"), "/a\n10") holds nothing, and dropAsk of a key asked twice holds it once.
function dropAsk(pending, key) {
    var next = {}
    var want = String(key)
    var src = pending || {}
    for (var k in src) {
        var left = k === want ? src[k] - 1 : src[k]
        if (left > 0) next[k] = left
    }
    return next
}

// The columns view's count follows the window width, capped by Settings View's limit.
// Sample input: (899, 5) answers 2.
var COUNT_4_AT = 1700
var COUNT_5_AT = 2300
var COUNT_NARROW_AT = 900
var COUNT_MIN = 2
var COUNT_MAX = 5
// The shipped View limit: a default install holds 3 columns from 900 px up.
var COUNT_DEFAULT = 3

// A stored number clamps to 2 to 5, anything else reads as the shipped default.
function cappedLimit(limit) {
    if (typeof limit !== "number")
        return COUNT_DEFAULT
    var n = Math.floor(limit)
    if (!(n >= COUNT_MIN) && !(n <= COUNT_MAX))
        return COUNT_DEFAULT
    if (n < COUNT_MIN)
        return COUNT_MIN
    if (n > COUNT_MAX)
        return COUNT_MAX
    return n
}

// One integer for a window of this width; resizing re-lays columns without re-reading.
function columnCountForWidth(width, limit) {
    var count = width < COUNT_NARROW_AT ? 2 : width < COUNT_4_AT ? 3
        : width < COUNT_5_AT ? 4 : 5
    var cap = cappedLimit(limit)
    if (count > cap)
        count = cap
    if (count < COUNT_MIN)
        count = COUNT_MIN
    return count
}

// Extra columns are ancestors, read with the same viewport-scoped peek as the parent today.
function ancestorsForCount(count) {
    var n = Math.floor(Number(count))
    if (!(n >= COUNT_MIN))
        return 0
    return Math.min(n, COUNT_MAX) - 2
}

// The parent one step up, with a trailing slash trimmed except at the root itself.
// Sample input: ancestorParent("/home/") is "/".
function ancestorParent(path) {
    var text = String(path)
    if (text.length > 1 && text.charAt(text.length - 1) === "/")
        text = text.substring(0, text.length - 1)
    var cut = text.lastIndexOf("/")
    return cut <= 0 ? "/" : text.substring(0, cut)
}

// The nearest distinct ancestors of a path, oldest first, stopping where the climb stops.
// Sample input: ancestors("/home/gm", 3) is ["/", "/home"].
function ancestors(path, count) {
    var n = Math.floor(Number(count))
    if (!(n >= 1))
        return []
    var text = String(path)
    if (text.length === 0)
        return []
    if (text.length > 1 && text.charAt(text.length - 1) === "/")
        text = text.substring(0, text.length - 1)
    var near = []
    var cur = text
    for (var i = 0; i < n; i++) {
        var p = ancestorParent(cur)
        if (p === cur || near.indexOf(p) >= 0)
            break
        near.push(p)
        cur = p
    }
    return near.reverse()
}

// True while an ancestor column of this depth names a directory of its own.
// Sample input: ancestorShown("/home", 2) is false.
function ancestorShown(path, depth) {
    var d = Math.floor(Number(depth))
    if (!(d >= 1))
        return false
    return ancestors(path, d).length >= d
}

// Sample input: neighbourAsks("/home/gm", 1200, 5) is ["/home"], the shown ancestors oldest first.
function neighbourAsks(path, width, limit) {
    var want = ancestorsForCount(columnCountForWidth(Number(width), limit))
    if (!(want >= 1))
        return []
    return ancestors(path, want)
}

// Sample input: keepAsks({ drawn: ["/a"] }, ["/a"], "/a/b") answers [{path "/a", again false}, {path "/a/b", again true}] and leaves drawn ["/a", "/a/b"]; again names a column not drawn last refresh.
function keepAsks(keep, asks, child) {
    var shown = child.length > 0 && asks.indexOf(child) < 0 ? asks.concat([child]) : asks.slice()
    var plan = shown.map(function (path) { return { path: path, again: keep.drawn.indexOf(path) < 0 } })
    keep.drawn = shown
    return plan
}

// ui.json carries whatever a hand edit wrote, so only a finite number is a stored width; anything else keeps the measured one.
// Sample input: storedNumber("120") is NaN, storedNumber(120.6) is 121.
function storedNumber(raw) {
    if (typeof raw !== "number" || !isFinite(raw) || raw < 0)
        return NaN
    return Math.round(raw)
}

// ListColumns040 board: a dragged edge clamps to the rails, and a double click fits the
// widest value the window holds, never a directory-wide scan. Sample input: 9999 clamps.
var MIN_LIST_WIDTH = 48
var MAX_LIST_WIDTH = 480

// One whole pixel inside the rails, whatever the drag handed in.
function clampListWidth(px) {
    var n = Math.round(Number(px))
    if (!(n >= 0))
        return MIN_LIST_WIDTH
    if (n < MIN_LIST_WIDTH)
        return MIN_LIST_WIDTH
    if (n > MAX_LIST_WIDTH)
        return MAX_LIST_WIDTH
    return n
}

// The widest held cell, clamped; an empty window keeps the column it has, so a fit can narrow as well as widen.
function autofitWidth(widths, fallback) {
    var cells = widths || []
    if (cells.length === 0)
        return clampListWidth(fallback)
    var best = MIN_LIST_WIDTH
    for (var i = 0; i < cells.length; i++) {
        var w = Math.round(Number(cells[i]))
        if (w >= 0 && w > best)
            best = w
    }
    return clampListWidth(best)
}

function names(s) {
    var out = ["name"]
    if (s.location) out.push("location")
    for (var i = DROP_ORDER.length - 1; i >= 0; i--) {
        var key = DROP_ORDER[i]
        if (s[key])
            out.push(key)
    }
    return out.join(",")
}

// An unanswered folder keeps the old column by data, never by picture.
// Sample input: folderDataHold(true, false) is true.
function folderDataHold(cursorIsDir, answered) {
    return cursorIsDir === true && answered !== true
}

// A file load needs a real file row: null never loads and a folder waits by data.
// Sample input: isFileRow({d: false}) is true.
function isFileRow(row) {
    return row !== null && row !== undefined && row.d !== true
}

// A landed peek shows the cursor folder while it is still the one waiting.
// Sample input: showFolderOnPeek("/a", "/b", true) is true.
function showFolderOnPeek(childPath, shownChildPath, answered) {
    return answered === true && childPath.length > 0 && childPath !== shownChildPath
}

// A data-held empty folder lands settled, the whole frame the picture hold revealed.
// Sample input: shouldSettleHero(true, "/a", "/a", false, 0) is true.
function shouldSettleHero(cursorIsDir, path, childPath, readFailed, rowCount) {
    if (cursorIsDir !== true || readFailed === true)
        return false
    if (String(path) !== String(childPath))
        return false
    return Math.floor(Number(rowCount)) === 0
}
