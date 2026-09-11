.pragma library

// A row this listing has dealt with: null means asked and waiting, a string is the answer.
var ASKED = null

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
        if (state.file[key] !== ASKED) {
            continue
        }
        var at = Number(key)
        if (at < first || at > last || !allowed(rows[at - held], mode)) {
            drop.push(at)
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

// The wrapper is new so a QML binding on it re-evaluates; the map inside is mutated in place.
function applied(state, work) {
    for (var i = 0; i < work.drop.length; i++) {
        forget(state, work.drop[i])
    }
    for (var j = 0; j < work.ask.length; j++) {
        state.file[work.ask[j]] = ASKED
        state.order.push(work.ask[j])
    }
    return { file: state.file, order: state.order }
}

// The cap bounds a policy bug, not normal use: only the rows a viewport held are ever recorded.
function remember(state, row, file, cap) {
    if (state.file[row] === undefined) {
        state.order.push(row)
    }
    state.file[row] = file
    while (state.order.length > cap) {
        delete state.file[state.order.shift()]
    }
    return { file: state.file, order: state.order }
}

function fileFor(state, row) {
    var value = state.file[row]
    return typeof value === "string" ? value : ""
}

// Answered with nothing, which is not the same as not answered yet. A thumbnailer that could not
// read the file reports an empty name, and only once that has arrived is it honest to say so.
function refused(state, row) {
    return state.file[row] === ""
}

// Backend icon identity distinguishes image thumbnails from video without opening any extra file.
function allowed(row, mode) {
    if (!row || !row.t || mode === "off") return false
    return mode !== "images" || row.i === "image-x-generic"
}
