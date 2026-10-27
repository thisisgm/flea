.pragma library

// A row this listing has dealt with: null means asked and waiting, a string is the answer.
// A cache-only ask carries a mark, so its empty answer means "not tried" and a class switch forgets it.
var ASKED = null
var CACHE_ASKED = "cache-asked"
var CACHE_MISS = "cache-missed"

// Asked and not answered yet, a plain ask or a cache-only one; only these have a job to cancel.
function pending(value) {
    return value === ASKED || value === CACHE_ASKED
}

function empty() {
    return { file: {}, order: [] }
}

// The rows a request may name: the viewport in row indices, clamped to the last row of the listing.
function viewport(contentY, rowHeight, visibleRows, total) {
    var first = Math.floor(contentY / rowHeight)
    // A window that is not row aligned straddles one more row than it holds, so the bottom comes off its pixel extent.
    var bottom = contentY + visibleRows * rowHeight
    return { first: first, last: Math.min(total - 1, Math.ceil(bottom / rowHeight) - 1) }
}

// Only visible rows, only rows the backend called thumbnailable, only rows never asked.
function plan(state, rows, held, first, last, mode) {
    var ask = []
    for (var i = first; i <= last; i++) {
        if (state.file[i] !== undefined) {
            continue
        }
        var row = rows[i - held]
        if (allowed(row, mode)) {
            ask.push(i)
        }
    }
    var drop = []
    for (var key in state.file) {
        if (!pending(state.file[key])) {
            continue
        }
        var at = Number(key)
        if (at < first || at > last || !allowed(rows[at - held], mode)) {
            drop.push(at)
        }
    }
    return { ask: ask, drop: drop }
}

// The same plan over the rows a filter draws (issue 315), each naming its listing row and the row the pane
// holds for it, or null for one not landed yet, which is not asked for. Those rows are a set and not a run,
// so a pending row is dropped when it is not among them, scrolled off and hidden alike, or no longer allowed.
function planDrawn(state, drawn, mode) {
    var ask = []
    var kept = {}
    for (var i = 0; i < drawn.length; i++) {
        var at = drawn[i].index
        var ok = allowed(drawn[i].row, mode)
        kept[at] = ok
        if (ok && state.file[at] === undefined) {
            ask.push(at)
        }
    }
    var drop = []
    for (var key in state.file) {
        if (pending(state.file[key]) && kept[key] !== true) {
            drop.push(Number(key))
        }
    }
    return { ask: ask, drop: drop }
}

// A cancelled row is forgotten here too, because the backend forgets it when it drops the job.
function forget(state, row) {
    delete state.file[row]
    var at = state.order.indexOf(row)
    if (at >= 0) {
        state.order.splice(at, 1)
    }
}

// A miss on a generating class returns to unasked; on a cache-only class it stays a miss.
function miss(state, row, generating, cap) {
    if (generating === true) {
        forget(state, row)
        return { file: state.file, order: state.order }
    }
    return remember(state, row, "", cap)
}

// The wrapper is new so a QML binding on it re-evaluates; the map inside is mutated in place.
function applied(state, work) {
    for (var i = 0; i < work.drop.length; i++) {
        forget(state, work.drop[i])
    }
    var mark = work.cacheOnly === true ? CACHE_ASKED : ASKED
    for (var j = 0; j < work.ask.length; j++) {
        state.file[work.ask[j]] = mark
        state.order.push(work.ask[j])
    }
    return { file: state.file, order: state.order }
}

// A live answer re-lists some rows: each finished thumbnail follows its row, by name, to that row's new index. Asked and missed rows do not follow.
function carried(state, oldRows, oldHeld, newRows, newHeld) {
    var at = Object.create(null)
    for (var j = 0; j < newRows.length; j++) {
        if (newRows[j].t) {
            at[newRows[j].n] = newHeld + j
        }
    }
    var next = empty()
    for (var k = 0; k < state.order.length; k++) {
        var file = state.file[state.order[k]]
        var row = oldRows[state.order[k] - oldHeld]
        if (file && file !== CACHE_ASKED && file !== CACHE_MISS && row && at[row.n] !== undefined) {
            next.file[at[row.n]] = file
            next.order.push(at[row.n])
        }
    }
    return next
}

// The cap bounds a policy bug, not normal use: only the rows a viewport held are ever recorded.
function remember(state, row, file, cap) {
    if (state.file[row] === undefined) {
        state.order.push(row)
    }
    // An empty answer to a cache-only ask was never a decode, so it keeps its mark.
    state.file[row] = (file === "" && state.file[row] === CACHE_ASKED) ? CACHE_MISS : file
    while (state.order.length > cap) {
        delete state.file[state.order.shift()]
    }
    return { file: state.file, order: state.order }
}

function fileFor(state, row) {
    var value = state.file[row]
    if (value === CACHE_ASKED || value === CACHE_MISS) return ""
    return typeof value === "string" ? value : ""
}

// Answered with nothing, which is not the same as not answered yet. A thumbnailer that could not
// read the file reports an empty name, and only once that has arrived is it honest to say so.
function refused(state, row) {
    return state.file[row] === ""
}

// src/backend/thumbs.rs THUMB_SIZE: the longest side of a cache file made from a larger original.
var CACHE_SIZE = 256

// How much a picture may be enlarged to fit its box, and never past limit: 1 for an original, which
// draws at its own size when it is smaller than the box (decided 2026-09-24). Sample input: (754, 471, 120, 68, 1) is 1.
function fitScale(boxWidth, boxHeight, width, height, limit) {
    return Math.min(limit, boxWidth / Math.max(1, width), boxHeight / Math.max(1, height))
}

// A cache file stands in for its original, so it is enlarged at most to the original's own size. The
// longest side is compared because EXIF can turn either one. Unknown dimensions leave a full-size cache
// file free to fill the box, while a smaller one already is the original. Sample input: (256, 171, 300, 200) is 300/256.
function thumbLimit(thumbWidth, thumbHeight, originalWidth, originalHeight) {
    var thumbLongest = Math.max(1, thumbWidth, thumbHeight)
    var originalLongest = Math.max(Number(originalWidth) || 0, Number(originalHeight) || 0)
    if (originalLongest > 0)
        return originalLongest / thumbLongest
    return thumbLongest < CACHE_SIZE ? 1 : Infinity
}

// A video poster fills the frame; an image keeps the never-enlarge cap.
function posterLimit(isVideo, thumbWidth, thumbHeight, originalWidth, originalHeight) {
    if (isVideo)
        return Infinity
    return thumbLimit(thumbWidth, thumbHeight, originalWidth, originalHeight)
}

// Backend icon identity distinguishes image thumbnails from video without opening any extra file; an AppImage's own icon is its identity, so Images draws a row the backend flagged a.
function allowed(row, mode) {
    if (!row || !row.t || mode === "off") return false
    return mode !== "images" || row.i === "image-x-generic" || row.a === true
}
