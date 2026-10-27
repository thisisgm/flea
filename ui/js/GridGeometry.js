.pragma library

// The grid's tile maths, split out of ui/GridArea.qml so tests/js/gridgeometry.js can pin it.

// Width less one gap and the scroll lane, so no tile sits under the bar.
// Sample input: columnsFor(800, 146, 96, 7, 8) is 5, five tiles across an 800 px grid.
function columnsFor(width, minCellWidth, tilePixels, rowPaddingX, gap, lane) {
    var cell = Math.max(minCellWidth, tilePixels + 2 * rowPaddingX)
    return Math.max(1, Math.floor((width - gap - (Number(lane) || 0)) / cell))
}

// Sample input: cellWidthFor(800, 5, 8) is 158, five cells filling the 792 beside the gap.
function cellWidthFor(width, columns, gap, lane) {
    return Math.max(1, Math.floor((width - gap - (Number(lane) || 0)) / Math.max(1, columns)))
}

// A cell is its slot plus the name gap plus two caption lines plus the pad above and below.
// Sample input: cellHeightFor(128, 9, 33, 14) is 198, the board tile.
function cellHeightFor(thumbPixels, gap, captionHeight, padY) {
    return Math.max(1, thumbPixels + gap + captionHeight + 2 * padY)
}

// Tile rows the view keeps built beyond its edge, the one number ui/GridArea.qml's cacheBuffer and a landing's ask both count.
var cacheRows = 2

// The window a grid holds, in whole tile rows: the list's context in pixels (bufferRows of rowHeight each side) rebuilt in tiles, so a screen of tiles keeps the room a screen of rows keeps.
// Sample input: windowFor(18, 11, 150, 26, 124, 25) is {columns: 18, visible: 11, buffer: 576, size: 1350, margin: 36}, 32 tile rows each side of 11, and a margin of two tile rows.
function windowFor(columns, visibleTileRows, bufferRows, rowHeight, cellHeight, refetchMargin) {
    var across = Math.max(1, columns)
    var rows = Math.max(1, Math.ceil(bufferRows * rowHeight / Math.max(1, cellHeight)))
    var visible = Math.max(1, visibleTileRows)
    return { columns: across, visible: visible, buffer: across * rows, size: across * (visible + 2 * rows), margin: Math.max(refetchMargin, 2 * across) }
}

// The window the visible items owe the backend once they come within the margin of an edge the held rows have a further side to, or when none are held.
// Sample input: driftAsk({buffer: 576, size: 1350, margin: 36}, 1000, 1330, 0, 1350, 2000) is {start: 424, count: 1350}, and null at 1000 to 1200.
function driftAsk(plan, first, last, held, length, total) {
    var end = held + length
    var behind = held > 0 && first - held < plan.margin
    var ahead = end < total && end - last < plan.margin
    return length === 0 || behind || ahead ? { start: Math.max(0, first - plan.buffer), count: plan.size } : null
}

// The window a far cursor landing owes the backend before the view moves, or null for a step or a landing the held rows reach.
// Sample input: landingAsk(plan, 1999, 0, 2000, 0, 352) with the plan above is {start: 1746, count: 254}, the tail of the listing; with 1500 rows held from 700 it is null.
function landingAsk(plan, cursorIndex, firstIndex, total, held, heldLength) {
    var row = Math.floor(cursorIndex / plan.columns)
    var seen = Math.floor(firstIndex / plan.columns)
    if (Math.abs(row - seen) <= plan.visible)
        return null
    var before = row > seen ? plan.visible + 1 : cacheRows + 1
    var after = row > seen ? cacheRows + 1 : plan.visible + 1
    var start = Math.max(0, (row - before) * plan.columns - plan.margin)
    var end = Math.min(total, (row + after) * plan.columns + plan.margin)
    if (start >= held && end <= held + heldLength)
        return null
    return { start: start, count: end - start }
}

// Whether two rows draw the same tile: every field equal, so a reply that repeats a row hands its tile nothing to redraw.
// Sample input: sameRow({n: "a", tg: [2]}, {n: "a", tg: [2]}) is true, sameRow({n: "a"}, null) is false.
function sameRow(a, b) {
    if (a === b)
        return true
    if (a === null || b === null || a === undefined || b === undefined)
        return false
    var keys = Object.keys(a)
    if (keys.length !== Object.keys(b).length)
        return false
    for (var i = 0; i < keys.length; i++) {
        var x = a[keys[i]], y = b[keys[i]]
        if (x !== y && (typeof x !== "object" || typeof y !== "object" || JSON.stringify(x) !== JSON.stringify(y)))
            return false
    }
    return true
}

// The row a tile draws from the held snapshot {start, rows, listing, more}: the held row, else what more (the filter's window, issue 315) holds for the index, else the row it kept at this index on the same listing, else null; memo is the tile's own {row, index, listing}.
// Sample input: heldRow({row: null, index: -1, listing: 0}, {start: 10, rows: [a, b], listing: 1}, 11) is b, and a tile at 5 afterwards keeps what it drew at 5.
function heldRow(memo, held, index) {
    var offset = index - held.start
    var next = offset >= 0 && offset < held.rows.length ? held.rows[offset] : held.more ? held.more(index) : null
    var kept = next === null ? memo.index === index && memo.listing === held.listing : sameRow(memo.row, next)
    memo.index = index
    memo.listing = held.listing
    if (kept)
        return memo.row
    memo.row = next
    return next
}
