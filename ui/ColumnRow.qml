import QtQuick
import qs.Commons
import "." as Flea
import "js/Icons.js" as Icons

// One row of a Miller column: a mark, a name, and the chevron a chosen directory carries. Simpler
// than a list row on purpose, because a column has no size, date, mode or kind to draw.
Item {
    id: root

    // {n, d, i} as a peek answers them, or a listing row, which carries the same three fields.
    property var row: null
    // The row this column's own cursor is on. Only the active column paints it in the accent.
    property bool cursor: false
    // A member of the pane's selection, which only the column drawing the pane's own listing has.
    property bool selected: false
    // The same row in an ancestor column: the cursor trail, lifted like a hover rather than accented.
    property bool lifted: false
    // An ancestor column that is not on the trail reads back, so its text drops to muted.
    property bool dim: false
    property bool hovered: false

    // Truthiness, like the two readers below: ui/ColumnPane.qml hands this rows[index] raw, so a
    // listing that shrank leaves a surviving delegate holding undefined, which is not null.
    readonly property bool isDir: !!root.row && root.row.d === true
    readonly property color ink: root.cursor ? Theme.color.accent
                                : root.dim ? Theme.color.muted
                                : Theme.color.foreground

    implicitHeight: Theme.rowHeight

    Rectangle {
        anchors.fill: parent
        // The ladder ui/Row.qml climbs, and for its reason: selectionFill is the OEM's fifth rung,
        // kept visually distinct from the cursor's own selectedFill so a member reads apart from it.
        color: root.cursor ? Style.selectedFill
             : root.selected ? Style.selectionFill
             : root.lifted ? Style.hoverFill
             : root.hovered ? Style.hoverFill
             : "transparent"
    }

    Item {
        id: markSlot
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        width: Theme.iconSize
        height: Theme.iconSize

        Flea.Glyph {
            anchors.fill: parent
            name: root.row ? Icons.glyphForRow(root.row.i, root.row.p) : "file"
            color: root.cursor ? Theme.color.accent : Theme.color.muted
        }
    }

    // corner: a filename is arbitrary text, so PlainText, the same rule every name on this surface follows.
    Text {
        anchors.left: markSlot.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.right: chevronSlot.left
        anchors.rightMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        text: root.row ? root.row.n : ""
        color: root.ink
        font.family: Theme.font.family
        font.pixelSize: Theme.font.bodySmall
        textFormat: Text.PlainText
        elide: Text.ElideRight
    }

    // Only a chosen directory carries it: it says the column to the right is showing what is inside.
    Item {
        id: chevronSlot
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        width: root.showChevron ? Theme.font.caption : 0
        height: Theme.font.caption

        Flea.Glyph {
            anchors.fill: parent
            visible: root.showChevron
            name: "chevron-right"
            color: root.cursor ? Theme.color.accent : Theme.color.muted
        }
    }

    readonly property bool showChevron: root.isDir && (root.cursor || root.lifted)

    HoverHandler {
        id: hover
        onHoveredChanged: root.hovered = hovered
    }
}
