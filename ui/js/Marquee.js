.pragma library
.import "Selection.js" as Selection

// Cell coordinates avoid inspecting or instantiating delegates outside the viewport.
function cells(x1, y1, x2, y2, width, height, columns, count) {
    var left = Math.min(x1, x2), right = Math.max(x1, x2)
    var top = Math.min(y1, y2), bottom = Math.max(y1, y2)
    if (width <= 0 || height <= 0 || count <= 0 || left === right || top === bottom
            || right <= 0 || bottom <= 0 || left >= columns * width)
        return null
    var box = { left: Math.max(0, Math.floor(left / width)),
                right: Math.min(columns - 1, Math.ceil(right / width) - 1),
                top: Math.max(0, Math.floor(top / height)),
                bottom: Math.min(Math.ceil(count / columns) - 1, Math.ceil(bottom / height) - 1) }
    return box.top <= box.bottom ? box : null
}

function same(a, b) {
    return a === b || (a !== null && b !== null && a.left === b.left && a.right === b.right
        && a.top === b.top && a.bottom === b.bottom)
}

// Visit only strips entering or leaving the rectangle; a one-row move never scans all old marks.
function difference(box, other, columns, count, backwardsX, backwardsY, visit) {
    if (box === null) return
    var strips = [box]
    if (other !== null && other.left <= box.right && other.right >= box.left
            && other.top <= box.bottom && other.bottom >= box.top) {
        var top = Math.max(box.top, other.top), bottom = Math.min(box.bottom, other.bottom)
        strips = [
            {left: box.left, right: box.right, top: box.top, bottom: top - 1},
            {left: box.left, right: Math.min(box.right, other.left - 1), top: top, bottom: bottom},
            {left: Math.max(box.left, other.right + 1), right: box.right, top: top, bottom: bottom},
            {left: box.left, right: box.right, top: bottom + 1, bottom: box.bottom}
        ]
        strips.sort(function (a, b) {
            return (backwardsY ? b.bottom - a.bottom : a.top - b.top)
                || (backwardsX ? b.right - a.right : a.left - b.left)
        })
    }
    var rowStep = backwardsY ? -1 : 1
    var columnStep = backwardsX ? -1 : 1
    for (var s = 0; s < strips.length; s++) {
        var strip = strips[s]
        if (strip.left > strip.right) continue
        for (var row = backwardsY ? strip.bottom : strip.top;
             row >= strip.top && row <= strip.bottom; row += rowStep) {
            for (var column = backwardsX ? strip.right : strip.left;
                 column >= strip.left && column <= strip.right; column += columnStep) {
                var index = row * columns + column
                if (index < count) visit(index)
            }
        }
    }
}

function begin(pane, additive) {
    var selection = Selection.create()
    if (additive) {
        var saved = pane.selectedIndices()
        for (var i = 0; i < saved.length; i++) selection.toggle(saved[i])
    }
    var state = { before: pane.selection, cursor: pane.cursorIndex, anchor: pane.selectionAnchor,
                  additive: additive, box: null, last: -1 }
    pane.selection = selection
    pane.selectionVersion++
    return state
}

function update(pane, state, box, columns, count, backwardsX, backwardsY, listingIndex) {
    if (same(state.box, box)) return
    difference(state.box, box, columns, count, false, false, function (viewIndex) {
        var index = listingIndex(viewIndex)
        if (index >= 0 && !(state.additive && state.before.has(index))) pane.selection.toggle(index)
    })
    difference(box, state.box, columns, count, backwardsX, backwardsY, function (viewIndex) {
        var index = listingIndex(viewIndex)
        if (index < 0) return
        if (!pane.selection.has(index)) pane.selection.toggle(index)
        state.last = index
    })
    state.box = box
    pane.selectionVersion++
}

function finish(pane, state, cancelled) {
    if (cancelled) {
        pane.selection = state.before
        pane.cursorIndex = state.cursor
        pane.selectionAnchor = state.anchor
        pane.selectionVersion++
    } else if (state.last >= 0) {
        pane.setCursor(state.last)
        pane.selectionAnchor = state.last
    }
}
