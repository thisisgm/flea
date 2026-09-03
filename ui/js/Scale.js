.pragma library

// The interface scale Ctrl+Shift+Plus and Ctrl+Shift+Minus step, issue 9, the way foot's own zoom
// does. Flea still reads Omarchy's base size for its type: this multiplies what the theme already
// resolved, so a box on base-size 12 stays on 12 until the operator asks for something else.

var MIN = 0.8
var MAX = 2.0
var STEP = 0.1

// Rounded to one decimal, or a walk up and back down lands on 0.9999999 and never reads as reset.
function stepped(scale, direction) {
    var from = scale > 0 ? scale : 1
    var next = Math.round((from + direction * STEP) * 10) / 10
    return Math.max(MIN, Math.min(MAX, next))
}

// The sentence the status bar shows, because a scale with no visible number is a state you cannot
// get back out of deliberately; Ctrl+Shift+0 is what the sentence names.
function announce(scale) {
    return "Interface scale " + Math.round(scale * 100) + " percent. Ctrl+Shift+0 resets."
}
