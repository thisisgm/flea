.import "../../ui/js/ActionWheel.js" as Wheel

// Tangerine's drag wheel is one annulus split into equal sectors. These checks hold the ported
// geometry, its gap, and the six routes to the same shape the landing page uses, without a Pane.
function near(a, b) {
    return Math.abs(a - b) < 0.0001
}

function pane() {
    return {
        acted: [],
        sheets: [],
        act: function (action) { this.acted.push(action) },
        sheetMenuAction: function (action) { this.sheets.push(action) }
    }
}

function run(check) {
    check("the wheel has Tangerine's six file actions", Wheel.ACTIONS.length, 6)
    check("sector 0 is at the top", near(Wheel.angleFor(0, 6), Math.PI / 2), true)
    check("sectors run clockwise", near(Wheel.angleFor(1, 6), Math.PI / 2 - Math.PI / 3), true)
    check("a three-way wheel starts at the left", near(Wheel.angleFor(0, 3), Math.PI), true)
    var zero = Wheel.pointAt(10, 0), top = Wheel.pointAt(10, Math.PI / 2)
    check("a point at angle zero is the right spoke", near(zero.x, 10) && near(zero.y, 0), true)
    check("a point at a right angle is the top spoke", near(top.x, 0) && near(top.y, -10), true)

    var radius = (Wheel.GEOMETRY.inner + Wheel.GEOMETRY.outer) / 2
    var misses = []
    for (var i = 0; i < Wheel.ACTIONS.length; i++) {
        var at = Wheel.pointAt(radius, Wheel.angleFor(i, Wheel.ACTIONS.length))
        if (Wheel.indexAt(at.x, at.y, Wheel.ACTIONS.length) !== i)
            misses.push(i)
    }
    check("every sector answers its own middle", misses.join(","), "")
    check("the hub is not a sector", Wheel.indexAt(0, 0, 6), -1)
    check("the inside edge is not a sector", Wheel.indexAt(Wheel.GEOMETRY.inner - 1, 0, 6), -1)
    check("the outside edge is not a sector", Wheel.indexAt(Wheel.GEOMETRY.outer + 1, 0, 6), -1)
    var boundary = Wheel.angleFor(0, 6) - Math.PI / 6
    var gap = Wheel.pointAt(radius, boundary)
    check("the gap between two petals belongs to neither", Wheel.indexAt(gap.x, gap.y, 6), -1)

    var paths = []
    var broken = []
    for (var p = 0; p < Wheel.ACTIONS.length; p++) {
        var path = Wheel.sectorPath(p, Wheel.ACTIONS.length, 100, 100)
        paths.push(path)
        if (path.indexOf("M ") !== 0 || path.indexOf("A ") < 0 || path.slice(-2) !== " Z" || /NaN|undefined/.test(path))
            broken.push(p)
    }
    check("every petal is a closed SVG path around the wheel's centre", broken.join(","), "")
    check("no two petals draw the same path", paths.filter(function (path, index) { return paths.indexOf(path) !== index }).length, 0)
    check("labels sit between the two rims, above their sector's middle",
          Wheel.labelPoint(0, 6).y < 0 && Wheel.labelPoint(0, 6).y > -Wheel.GEOMETRY.outer, true)
    check("the hub names the wheel at rest", Wheel.centerText(-1), "Actions")
    check("and names the lit action", Wheel.centerText(0), "Copy")

    var sink = pane()
    var routes = []
    for (var r = 0; r < Wheel.ACTIONS.length; r++)
        routes.push(Wheel.run(r, sink))
    check("the six routes are the pane's existing actions", routes.join(","),
          "copy,moveTo,copyPath,addToShelf,compress:zip,openTerminal")
    check("copy and the other four go through act()", sink.acted.join(","),
          "copy,moveTo,copyPath,compress:zip,openTerminal")
    check("Shelf goes through the menu's selection snapshot", sink.sheets.join(","), "addToShelf")
    check("a missing sector runs nothing", Wheel.run(-1, sink), "")
    check("and a missing pane runs nothing", Wheel.run(0, null), "")
}
