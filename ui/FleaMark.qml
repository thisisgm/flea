import QtQuick
import QtQuick.Shapes

// Flea's own mark: the Omarchy spiral redrawn as a 24-grid stroke glyph, per BrandMoments.dc.html's
// "The mark" cell. It paints itself in from blank and then holds. The caller drives the repeat, so
// the empty state's mark and its caption share one beat; the loading crawl (ui/Spinner.qml) stays a
// separate gesture on purpose, continuous where this one arrives and rests.
Item {
    id: root

    property color color: Theme.color.muted
    // The mark draws on the same 24 unit grid as every Glyph, scaled to the slot.
    readonly property real grid: 24
    readonly property real markScale: Math.min(root.width, root.height) / root.grid
    // The path below sums to this along its centreline: 18+18+18+14+14+10+10+6+6.
    readonly property real markUnits: 114
    // A brand mark, not a cut glyph: the spiral keeps the brand's 2 and is exempt from Theme.strokeWidth on purpose.
    readonly property real brandStroke: 2
    // QML dash units are strokeWidth multiples, so the whole mark is this many dashes long.
    readonly property real markDashes: root.markUnits / root.brandStroke
    readonly property bool settled: !draw.running && stroke.dashOffset === 0 && root.opacity === 1

    // Repaints the mark from blank. EmptyState calls this off the caption's own timer so the two run
    // on one beat; a second timer in here would drift against that one and read as sloppy.
    function replay() {
        if (Theme.reducedMotion) {
            draw.stop()
            stroke.dashOffset = 0
            root.opacity = 1
        } else draw.restart()
    }

    Shape {
        width: root.grid
        height: root.grid
        x: (root.width - root.grid * root.markScale) / 2
        y: (root.height - root.grid * root.markScale) / 2
        preferredRendererType: Shape.CurveRenderer
        transform: Scale { xScale: root.markScale; yScale: root.markScale }

        ShapePath {
            id: stroke
            strokeColor: root.color
            fillColor: "transparent"
            strokeWidth: root.brandStroke
            capStyle: ShapePath.SquareCap
            joinStyle: ShapePath.MiterJoin
            strokeStyle: ShapePath.DashLine
            // One dash and one gap, each the whole mark, so only ever one dash can sit on the path.
            dashPattern: [root.markDashes, root.markDashes]
            // A whole mark of offset draws nothing; the animation walks that back to zero.
            dashOffset: root.markDashes
            PathSvg { path: "M21 21H3V3h18v14H7V7h10v6h-6" }
        }
    }

    // BrandMoments.dc.html's draw keyframe, 0.2s delay then 1.8s on cubic-bezier(0.4, 0, 0.2, 1);
    // QML's BezierSpline wants the implicit (1,1) end point appended, the way Motion.js notes.
    SequentialAnimation {
        id: draw
        // The caption's own fade-out, so a repeat dissolves the finished mark rather than cutting it.
        NumberAnimation { target: root; property: "opacity"; to: 0.0; duration: 180; easing.type: Easing.OutQuad }
        // Blank before opacity comes back, never after: an offset of a whole mark is what hides the seam.
        PropertyAction { target: stroke; property: "dashOffset"; value: root.markDashes }
        PropertyAction { target: root; property: "opacity"; value: 1.0 }
        PauseAnimation { duration: 200 }
        NumberAnimation {
            target: stroke
            property: "dashOffset"
            from: root.markDashes
            to: 0
            duration: 1800
            easing.type: Easing.BezierSpline
            easing.bezierCurve: [0.4, 0, 0.2, 1, 1, 1]
        }
    }

    // The draw is the entrance as well as the loop, so it runs on every appearance; Item.visible reads effective visibility, so a hidden ancestor holds it back too.
    onVisibleChanged: if (root.visible) root.replay(); else draw.stop()
    // visible can already be true at creation, and onVisibleChanged never fires for that.
    Component.onCompleted: if (root.visible) root.replay()
    Connections {
        target: Theme
        function onReducedMotionChanged() { if (Theme.reducedMotion) root.replay() }
    }
}
