import QtQuick
import QtQuick.Shapes
import "js/Icons.js" as Icons

// One row or sidebar mark, chosen by name. Lucide's grid is 24 units at stroke 2, scaled to the slot.
Item {
    id: root

    property string name: "file"
    property color color: "transparent"
    // Lucide draws on a 24 unit grid, so every path in Icons.js is in those units.
    readonly property real grid: 24
    // The mark's ceiling. Default is the row and menu mark; a surface drawing a mark alone passes its own.
    property real maxSize: Theme.markSize
    // The min() against the slot is the icon spec's own rule, so a smaller caller slot still wins
    // (the Network "+" at the caption token needs it) and no container can ever enlarge a mark.
    readonly property real markSize: Math.min(root.maxSize, root.width, root.height)
    // Its own name: "scale" would shadow Item.scale, which qmllint flagged.
    readonly property real gridScale: root.markSize / root.grid
    // Grid units, then scaled with the path. A large slot overrides this so screen-space thickness
    // can stay at the designed weight instead of growing with the mark.
    property real strokeWidth: Theme.strokeWidth

    // The Shape is sized in grid units pre-transform; Scale's origin defaults to (0,0), so this
    // offset is what re-centers the scaled-down box in root's slot instead of pinning it top-left.
    Shape {
        width: root.grid
        height: root.grid
        x: (root.width - root.markSize) / 2
        y: (root.height - root.markSize) / 2
        preferredRendererType: Shape.CurveRenderer
        transform: Scale { xScale: root.gridScale; yScale: root.gridScale }

        ShapePath {
            strokeColor: root.color
            fillColor: "transparent"
            strokeWidth: root.strokeWidth
            // The Omarchy cut: square caps and mitered joins, the edge of the brand spiral; Icons.js paths are redrawn sharp to match.
            capStyle: ShapePath.SquareCap
            joinStyle: ShapePath.MiterJoin
            // A mark with more than one stroke is one multi-subpath SVG string; see Icons.js "Lucide path data".
            PathSvg { path: Icons.pathFor(root.name) }
        }
    }
}
