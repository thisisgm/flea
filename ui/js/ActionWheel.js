.pragma library

// Tangerine's drag wheel, ported to Flea's own actions. The geometry is the landing page's
// (assets/home-*.js): one annulus of equal petals, sector 0 at the top, clockwise, a small angular
// gap between neighbours, and rounded corners cut into the SVG path rather than into the hit test.
var GEOMETRY = {
    inner: 48.64,
    outer: 119.52,
    backdrop: 125.92,
    gap: 4,
    corner: 20.16,
    labelInner: 40
}

// The six actions Tangerine's file wheel stands for. "Shelf" is Flea's staging tray; the other
// five are the listing's own routes, so the wheel adds an entrance and never a second implementation.
var ACTIONS = [
    { id: "copy", label: "Copy", glyph: "copy" },
    { id: "move", label: "Move to", glyph: "move-horizontal" },
    { id: "path", label: "Copy path", glyph: "file-text" },
    { id: "shelf", label: "Shelf", glyph: "shelf" },
    { id: "compress", label: "Compress", glyph: "archive" },
    { id: "terminal", label: "Terminal", glyph: "terminal" }
]

// The middle of a sector, in math coordinates: positive y is up, and sector 0 is at the top.
function angleFor(index, count) {
    return (count === 3 ? Math.PI : Math.PI / 2) - (Math.PI * 2 * index) / count
}

function pointAt(radius, angle) {
    return { x: Math.cos(angle) * radius, y: -Math.sin(angle) * radius }
}

// Half the angular width the drawn gap takes at this radius; used by both the path and the hit test,
// so a click through a visible gap cannot open either neighbour.
function gapAngle(radius) {
    return Math.asin(Math.min(0.95, GEOMETRY.gap / 2 / Math.max(radius, GEOMETRY.gap / 2 + 0.01)))
}

// An annulus sector as an SVG path, in the same rounded shape Tangerine draws. cx/cy move the
// path's origin to the centre of the QML item that paints it.
function sectorPath(index, count, cx, cy) {
    var geometry = GEOMETRY
    var middle = angleFor(index, count)
    var step = (Math.PI * 2) / count
    var left = middle - step / 2
    var right = middle + step / 2
    function corner(radius) {
        return Math.asin(Math.min(0.95, geometry.gap / 2 / Math.max(radius, geometry.gap / 2 + 0.01)))
    }
    var outerLeft = left + corner(geometry.outer)
    var outerRight = right - corner(geometry.outer)
    var innerLeft = left + corner(geometry.inner)
    var innerRight = right - corner(geometry.inner)
    var outerFillet = Math.min(0.15, (outerRight - outerLeft) * 0.24)
    var innerFillet = Math.min(0.18, (innerRight - innerLeft) * 0.18)
    var innerWide = geometry.inner / Math.max(0.01, Math.cos(innerFillet))
    var innerEdge = innerWide + geometry.inner * Math.tan(innerFillet)
    var outerInset = geometry.outer - geometry.corner
    function at(radius, angle) {
        var point = pointAt(radius, angle)
        return (point.x + cx).toFixed(4) + " " + (point.y + cy).toFixed(4)
    }
    var outerArc = outerRight - outerFillet - (outerLeft + outerFillet) > Math.PI ? 1 : 0
    var innerArc = innerRight - innerFillet - (innerLeft + innerFillet) > Math.PI ? 1 : 0
    return "M " + at(innerEdge, left + corner(innerEdge))
        + " L " + at(outerInset, left + corner(outerInset))
        + " Q " + at(geometry.outer, outerLeft) + " " + at(geometry.outer, outerLeft + outerFillet)
        + " A " + geometry.outer + " " + geometry.outer + " 0 " + outerArc + " 0 "
        + at(geometry.outer, outerRight - outerFillet)
        + " Q " + at(geometry.outer, outerRight) + " " + at(outerInset, right - corner(outerInset))
        + " L " + at(innerEdge, right - corner(innerEdge))
        + " Q " + at(innerWide, right - corner(innerWide)) + " " + at(geometry.inner, innerRight - innerFillet)
        + " A " + geometry.inner + " " + geometry.inner + " 0 " + innerArc + " 1 "
        + at(geometry.inner, innerLeft + innerFillet)
        + " Q " + at(innerWide, left + corner(innerWide)) + " " + at(innerEdge, left + corner(innerEdge))
        + " Z"
}

// The sector under a point measured from the wheel's centre, or -1 in the hub, outside the rim and
// in a gap. This is Tangerine's Ke(): nearest sector first, then the gap that belongs to neither.
function indexAt(x, y, count) {
    if (!count || count < 1) return -1
    var radius = Math.hypot(x, y)
    if (radius < GEOMETRY.inner || radius > GEOMETRY.outer)
        return -1
    var angle = Math.atan2(-y, x)
    var step = (Math.PI * 2) / count
    var turned = (((angleFor(0, count) - angle) % (Math.PI * 2)) + Math.PI * 2) % (Math.PI * 2)
    var index = Math.round(turned / step) % count
    var middle = angleFor(index, count)
    var off = Math.abs(Math.atan2(Math.sin(angle - middle), Math.cos(angle - middle)))
    return off > step / 2 - Math.asin(GEOMETRY.gap / 2 / radius) ? -1 : index
}

// Where a petal's caption sits. Tangerine labels at labelInner + 56% of the annulus, not at its middle.
function labelPoint(index, count) {
    var radius = GEOMETRY.labelInner + (GEOMETRY.outer - GEOMETRY.labelInner) * 0.56
    return pointAt(radius, angleFor(index, count))
}

// The label in the hub: the wheel's name until a petal is chosen, then that petal's own.
function centerText(index) {
    return index >= 0 && index < ACTIONS.length ? ACTIONS[index].label : "Actions"
}

// One route per petal, all through the pane's existing actions. Returning the route lets
// tests/js/wheel.js hold the table to the same calls without a Pane.
function run(index, pane) {
    var action = ACTIONS[index]
    if (!action || !pane) return ""
    if (action.id === "shelf") {
        // Shelf is client-side, but the snapshot route resolves the whole selection (including rows
        // outside the held window) before PaneMenuActions.shelve sees it.
        pane.sheetMenuAction("addToShelf")
        return "addToShelf"
    }
    var route = action.id === "move" ? "moveTo"
              : action.id === "path" ? "copyPath"
              : action.id === "compress" ? "compress:zip"
              : action.id === "terminal" ? "openTerminal"
              : action.id
    pane.act(route)
    return route
}
