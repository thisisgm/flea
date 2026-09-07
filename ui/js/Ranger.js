.pragma library

// The last screenshot the operator named is 1097 px wide. At that width and below, the previous
// Miller column gives its space to the current and next columns instead of squeezing all three.
var NARROW_WINDOW_WIDTH = 1097

function layout(windowWidth, areaWidth) {
    var previousVisible = windowWidth > NARROW_WINDOW_WIDTH
    var count = previousVisible ? 3 : 2
    var columnWidth = Math.floor(areaWidth / count)
    return {
        previousVisible: previousVisible,
        columnWidth: columnWidth,
        lastWidth: areaWidth - columnWidth * (count - 1)
    }
}
