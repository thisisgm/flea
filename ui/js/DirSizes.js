.pragma library

// A row this listing has asked about: null means asked and waiting, an object is the answer.
var ASKED = null

function empty() {
    return { file: {}, order: [] }
}

// Only visible rows, only rows the backend called directories, only rows never asked.
function plan(state, rows, held, first, last) {
    var ask = []
    for (var i = first; i <= last; i++) {
        if (state.file[i] !== undefined) {
            continue
        }
        var row = rows[i - held]
        if (row && row.d) {
            ask.push(i)
        }
    }
    return ask
}

// The same plan over the rows a filter draws (issue 315), each naming its listing row and the row the pane
// holds for it, or null for one not landed yet, which is not asked for.
function planDrawn(state, drawn) {
    var ask = []
    for (var i = 0; i < drawn.length; i++) {
        var row = drawn[i].row
        if (state.file[drawn[i].index] === undefined && row && row.d) {
            ask.push(drawn[i].index)
        }
    }
    return ask
}

// The wrapper is new so a QML binding on it re-evaluates; the map inside is mutated in place.
function applied(state, ask) {
    for (var i = 0; i < ask.length; i++) {
        state.file[ask[i]] = ASKED
        state.order.push(ask[i])
    }
    return { file: state.file, order: state.order }
}

// Gates the cancel wire message, since a fling fires onContentYChanged on every pixel and a fresh listing has nothing queued to cancel yet.
function hasPending(state) {
    for (var i = 0; i < state.order.length; i++) {
        if (state.file[state.order[i]] === ASKED) {
            return true
        }
    }
    return false
}

// A scroll cancels every row still waiting, not only the ones off-viewport, matching what the backend's own dirsizecancel means.
function cancelled(state) {
    var file = {}
    var order = []
    for (var i = 0; i < state.order.length; i++) {
        var row = state.order[i]
        if (state.file[row] === ASKED) {
            continue
        }
        file[row] = state.file[row]
        order.push(row)
    }
    return { file: file, order: order }
}

// A live answer re-lists some folders: each answered size follows its row, by name, to that row's new index. Pending asks do not follow.
function carried(state, oldRows, oldHeld, newRows, newHeld) {
    var at = Object.create(null)
    for (var j = 0; j < newRows.length; j++) {
        if (newRows[j].d) {
            at[newRows[j].n] = newHeld + j
        }
    }
    var next = empty()
    for (var k = 0; k < state.order.length; k++) {
        var answer = state.file[state.order[k]]
        var row = oldRows[state.order[k] - oldHeld]
        if (answer && row && at[row.n] !== undefined) {
            next.file[at[row.n]] = answer
            next.order.push(at[row.n])
        }
    }
    return next
}

// Sizes a live answer carried on its rows land with those rows; with none the state comes back as it was.
// Sample input: (null, [{n: "x", d: true}, {n: "c", d: true, z: 96}], 20) answers file {21: {bytes: 96, partial: false}}, order [21].
function inlined(state, rows, held) {
    var next = null
    for (var i = 0; i < rows.length; i++) {
        if (rows[i].z === undefined)
            continue
        next = next || state || empty()
        if (next.file[held + i] === undefined)
            next.order.push(held + i)
        next.file[held + i] = { bytes: rows[i].z, partial: false }
    }
    return next || state
}

// The cap bounds a policy bug, not normal use, the same reason ui/js/Thumbs.js has one; callers pass Pane's own thumbCap.
function remember(state, row, bytes, partial, cap) {
    if (state.file[row] === undefined) {
        state.order.push(row)
    }
    state.file[row] = { bytes: bytes, partial: partial }
    while (state.order.length > cap) {
        delete state.file[state.order.shift()]
    }
    return { file: state.file, order: state.order }
}

// Sample input: ("network") trues, ("usb") falses.
function isRemoteClass(storageClass) {
    return storageClass === "network" || storageClass === "phone"
}

// Sample input: (["size"]) trues, ([]) falses.
function sizeHidden(hiddenCols) {
    return hiddenCols.indexOf("size") >= 0
}

// Sample input: ("list", [], "", true) trues, ("grid", [], "", true) falses.
function wantsSizes(viewMode, hiddenCols, storageClass, storageKnown) {
    if (!storageKnown) return false
    if (isRemoteClass(storageClass)) return false
    if (viewMode === "grid") return false
    if (viewMode === "list" && sizeHidden(hiddenCols)) return false
    return true
}

// null covers both "never asked" and "asked and still waiting": the Size cell renders both as the middle dot.
function sizeFor(state, row) {
    var value = state.file[row]
    return value && typeof value === "object" ? value : null
}
