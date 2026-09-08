.pragma library

// The wheel's arithmetic, kept pure so tests/js/scroll.js can drive it without a Flickable.

// One discrete notch is 120 units of angleDelta, Qt's own convention for a wheel click.
var NOTCH_UNITS = 120
// Qt's own lines per notch when the platform reports none: the one fallback, so the handler passes the raw hint.
var DEFAULT_LINES = 3

// How far a wheel event moves the content: a touchpad's pixels one to one, like every other
// application, and a wheel notch as the platform's lines times notchPx times the multiplier.
// Sample input: pixelDeltaY 0, angleDeltaY -120, lines 3, notchPx 24, multiplier 4 gives -288.
function distance(pixelDeltaY, angleDeltaY, lines, notchPx, multiplier) {
    var pixels = Number(pixelDeltaY) || 0
    if (pixels !== 0)
        return pixels
    var perNotch = Number(lines) > 0 ? Number(lines) : DEFAULT_LINES
    return (Number(angleDeltaY) || 0) / NOTCH_UNITS * perNotch * notchPx * multiplier
}

// A content position kept inside the Flickable: never above its origin, never past its last page.
// A content shorter than the view pins to the origin.
function bounded(value, originY, contentHeight, viewHeight) {
    var minimum = Number(originY) || 0
    var maximum = Math.max(minimum, minimum + Math.max(0, Number(contentHeight) || 0) - Math.max(0, Number(viewHeight) || 0))
    return Math.max(minimum, Math.min(maximum, value))
}

// Whether a wheel event is consumed: only when it actually moved the content, so an event at the
// edge keeps propagating to whatever holds this Flickable.
function moved(previous, current) {
    return Math.abs(current - previous) > 0.01
}
