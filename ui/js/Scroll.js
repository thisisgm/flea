.pragma library

// Touchpad pixel deltas are already in the compositor's direction; Qt Quick's built-in Flickable
// path discarded their useful magnitude on this box. Four times the delivered pixels matches the
// browser feel the operator chose. A click-wheel event has no pixel delta, so it moves six rows.
var PIXEL_MULTIPLIER = 4
var WHEEL_ROWS = 6

function delta(pixelY, angleY, rowHeight) {
    if (pixelY !== 0)
        return -pixelY * PIXEL_MULTIPLIER
    return -(angleY / 120) * rowHeight * WHEEL_ROWS
}

function position(contentY, originY, contentHeight, height, pixelY, angleY, rowHeight) {
    var end = originY + Math.max(0, contentHeight - height)
    return Math.max(originY, Math.min(end, contentY + delta(pixelY, angleY, rowHeight)))
}
