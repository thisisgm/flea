.pragma library

// Shared motion constants for Flea's structural transitions: preview open/close, share browser
// open/close, the Network dialog open/close, the empty state's appearance. The controller's
// ruling: scroll, the cursor and hover fills stay animation-free forever (see the 60ms
// CursorSurface note in the KB); only these four gain motion, 150-250ms, one easing curve.

// Material 3's "emphasized decelerate", the standard CSS cubic-bezier(0.05, 0.7, 0.1, 1.0)
// control points; QML's BezierSpline wants the implicit (1,1) end point appended.
var bezierCurve = [0.05, 0.7, 0.1, 1.0, 1, 1]

var durMs = {
    // Mid-band of the ruled 150-250ms window.
    open: 200,
    // Close reads faster than open, ~60% of it, asymmetric per the controller's ruling.
    close: 120
}

// Open rises into place from this far below its resting position; close does not translate,
// opacity only (see the Preview/ShareBrowser/NetworkDialog verticalCenterOffset/y bindings).
var translateUpPx = 10
