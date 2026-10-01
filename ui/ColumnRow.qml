import QtQuick
import qs.Commons
import "." as Flea
import "js/Drag.js" as DragOps
import "js/Icons.js" as Icons
import "js/Format.js" as Format

// One row of a Miller column: a mark, a name, and the chevron a chosen directory carries. Simpler
// than a list row on purpose, because a column has no size, date, mode or kind to draw.
Item {
    id: root

    // {n, d, i} as a peek answers them, or a listing row, which carries the same three fields.
    property var row: null
    // The Stack's rows carry absolute paths. A directory listing's names have no slash, so this is a no-op there.
    property bool leafName: false
    readonly property string shownName: root.leafName && root.row && String(root.row.n).indexOf("/") >= 0 ? String(root.row.n).substring(String(root.row.n).lastIndexOf("/") + 1) : (root.row ? String(root.row.n) : "")
    // The pane's thumbnail path for this row, empty for a peek's row: only the listing's own column asks for any.
    property string thumb: ""
    // A path is not a thumbnail: a cache file evicted between the answer and the decode leaves the mark to the glyph.
    readonly property bool thumbDrawn: root.thumb.length > 0 && thumbImage.status !== Image.Error
    readonly property alias iconStatus: thumbImage.status
    readonly property Item thumbItem: thumbImage
    // The row this column's own cursor is on. Only the active column paints it in the accent.
    property bool cursor: false
    // A member of the pane's selection, which only the column drawing the pane's own listing has.
    property bool selected: false
    // The same row in an ancestor column: the cursor trail, lifted like a hover rather than accented.
    property bool lifted: false
    // The active column draws a size; a peek column cannot ask for one and leaves this false.
    property bool showSize: false
    // The recursive size of a directory row, resolved by index the way the list resolves it.
    property var dirSize: null
    // An ancestor column that is not on the trail reads back, so its text drops to muted.
    property bool dim: false
    property bool hovered: false
    property bool dropTarget: false
    property bool dropCopying: false
    // The area derives it per visible row, so an empty clipboard costs nothing.
    property string clipMark: ""
    // A cut row dims to the ClipMarks board's own opacity, content only.
    readonly property bool clipCut: root.clipMark === "scissors"
    // One dim for the content below, so one ternary serves the mark, the name and the size.
    readonly property real dimOpacity: root.clipCut ? Theme.disabledOpacity : 1
    // One helper, so each dimmed colour multiplies the cut without a second binding.
    function dimmed(base) { return Util.alpha(base, root.dimOpacity) }
    // The mark's own size: 12 px in the muted role, 9 px after the name, per the board.
    readonly property int clipPx: 12
    // The column hands one budget for every row, with ElideMiddle as the backstop.
    property int nameBudget: -1
    property real paintWidth: 0 // Caller view width; fills paint to it under the lane, content keeps rowWidth.

    // Truthiness, like the two readers below: ui/ColumnPane.qml hands this rows[index] raw, so a
    // listing that shrank leaves a surviving delegate holding undefined, which is not null.
    readonly property bool isDir: !!root.row && root.row.d === true
    // The cut dim lives in the ink, so no child carries its own opacity binding.
    readonly property color ink: root.dimmed(root.cursor ? Theme.color.accent : root.dim ? Theme.color.muted : Theme.color.foreground)

    // One height for every row: ui/ColumnPane.qml draws the editor over the row rather than inside
    // it, so no row grows and the overlay's own y is plain arithmetic on this height.
    implicitHeight: Theme.fileRowHeight

    Rectangle {
        width: root.paintWidth > 0 ? root.paintWidth : parent.width
        height: parent.height
        // Match the active listing's cursor and marked-selection roles.
        color: root.cursor ? Style.selectedAccentFill
             : root.selected ? Style.selectionFill
             : root.lifted ? Style.hoverFill
             : root.hovered ? Style.hoverFill
             : "transparent"
    }

    Rectangle {
        visible: root.cursor
        width: Theme.spacing.hairline * 2
        height: parent.height
        color: Theme.color.accent
    }

    // The drop wash builds behind the row content, so the accent never tints the text.
    Loader {
        id: washLoader
        active: root.dropTarget
        width: root.paintWidth > 0 ? root.paintWidth : parent.width
        height: parent.height
        // The board's accent hairline over a faint wash at the hover rung's alpha: the token wins over the mock's own 0.07.
        sourceComponent: Rectangle {
            anchors.fill: parent
            color: Util.alpha(Theme.color.accent, Style.hoverFillAlpha)
            border.width: Theme.spacing.hairline
            border.color: Theme.color.accent
        }
    }

    Item {
        id: markSlot
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        width: Theme.iconSize
        height: Theme.iconSize

        // Sized on purpose, the same decode arm as ui/Row.qml: the cache PNG is capped at the icon's own size.
        Image {
            id: thumbImage
            anchors.fill: parent
            visible: root.thumbDrawn
            opacity: root.dimOpacity
            sourceSize.width: Theme.iconSize
            sourceSize.height: Theme.iconSize
            fillMode: Image.PreserveAspectFit
            asynchronous: true
            // No cache by URL: a regenerated thumbnail keeps its path, and a cached decode would keep the old pixels.
            cache: false
            source: root.thumb.length > 0 ? Format.fileUri(root.thumb) : ""
        }

        Flea.Glyph {
            id: markGlyph
            anchors.fill: parent
            visible: !root.thumbDrawn
            name: root.row ? Icons.glyphForRow(root.row.i, root.row.p) : "file"
            color: root.dimmed(root.cursor ? Theme.color.accent : Theme.color.muted)
        }
    }

    // corner: arbitrary filename text draws PlainText and elides in the middle per Names040.
    Text {
        id: nameText
        anchors.left: markSlot.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.right: sizeCell.left
        anchors.rightMargin: (root.showSize ? Theme.spacing.gap : 0) + (root.clipMark.length > 0 ? Theme.spacing.gap + root.clipPx : 0)
        anchors.verticalCenter: parent.verticalCenter
        text: root.row && root.nameBudget >= 0 ? Format.middleElide(root.shownName, Math.max(0, root.nameBudget - (root.clipMark.length > 0 ? Math.ceil((Theme.spacing.gap + root.clipPx) / Theme.bodyAdvance) : 0))) : root.shownName
        color: root.ink
        font.family: Theme.font.family
        font.pixelSize: Theme.font.body
        textFormat: Text.PlainText
        elide: Text.ElideMiddle
    }

    // Only the active column carries a number (ColumnsTabs board rule 3); the size holds its right edge off the parent by the chevron width.
    Text {
        id: sizeCell
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX + chevronSlot.width + (root.showSize ? Theme.spacing.gap : 0)
        anchors.verticalCenter: parent.verticalCenter
        visible: root.showSize && !root.dropTarget
        width: visible ? Theme.column.size : 0
        text: root.showSize && root.row ? root.sizeText() : ""
        color: root.ink
        horizontalAlignment: Text.AlignRight
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
        elide: Text.ElideRight
    }

    // The drop label and the clip mark build above the row content, so a row at rest builds none.
    Loader {
        id: dropClipLoader
        active: root.dropTarget || root.clipMark.length > 0
        anchors.fill: parent
        sourceComponent: Item {
            property alias label: dropLabel
            property alias mark: clipMarkGlyph
            anchors.fill: parent
            Text {
                id: dropLabel
                visible: root.dropTarget
                anchors.right: parent.right
                anchors.rightMargin: Theme.spacing.rowPaddingX
                anchors.verticalCenter: parent.verticalCenter
                text: DragOps.label(root.dropCopying)
                color: Theme.color.accent
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                textFormat: Text.PlainText
            }
            // The board's own nudge: the mark sits one pixel above the text centre line.
            Flea.Glyph {
                id: clipMarkGlyph
                visible: root.clipMark.length > 0
                width: root.clipPx
                height: root.clipPx
                x: nameText.x + Math.min(nameText.implicitWidth, nameText.width) + Theme.spacing.gap
                anchors.verticalCenter: parent.verticalCenter
                anchors.verticalCenterOffset: -1
                name: root.clipMark
                color: Theme.color.muted
            }
        }
    }

    // The list's own cell text, so a size reads the same wherever it is drawn.
    function sizeText() {
        if (Format.isSymlink(root.row.p)) return "link"
        if (!root.row.d) return Format.size(root.row.s)
        if (!root.dirSize) return "·"
        return (root.dirSize.partial ? ">" : "") + Format.size(root.dirSize.bytes)
    }

    // Only a chosen directory carries it: it says the column to the right is showing what is inside.
    Item {
        id: chevronSlot
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        width: root.dropTarget && dropClipLoader.item && dropClipLoader.item.label ? dropClipLoader.item.label.implicitWidth : root.showChevron ? Theme.font.caption : 0
        height: root.dropTarget && dropClipLoader.item && dropClipLoader.item.label ? dropClipLoader.item.label.implicitHeight : Theme.font.caption

        Flea.Glyph {
            anchors.fill: parent
            visible: root.showChevron && !root.dropTarget
            name: "chevron-right"
            color: root.cursor ? Theme.color.accent : Theme.color.muted
        }
    }

    readonly property bool showChevron: root.isDir && (root.cursor || root.lifted)

    HoverHandler {
        id: hover
        onHoveredChanged: root.hovered = hovered
    }

    // Test seam as functions, so no row at rest binds to name geometry (tests/columnscost.qml).
    function clipGlyph() { var m = dropClipLoader.item ? dropClipLoader.item.mark : null; return m && m.visible && m.width > 0 ? m : null }
    function clipX() { var m = root.clipGlyph(); return m ? m.x : 0 }
    // Paint order as a value: the wash builds behind the content, the label and mark above it.
    function stackingOk() {
        var kids = root.children
        var wash = -1
        var mark = -1
        var top = -1
        for (var i = 0; i < kids.length; i++) {
            if (kids[i] === washLoader) wash = i
            if (kids[i] === markSlot) mark = i
            if (kids[i] === dropClipLoader) top = i
        }
        return wash >= 0 && mark >= 0 && top >= 0 && wash < mark && top > mark
    }
    function clipExpectedX() { return nameText.x + Math.min(nameText.implicitWidth, nameText.width) + Theme.spacing.gap }
    function displayText() { return nameText.text }
    function nameItem() { return nameText }
    function markItem() { return markGlyph }
    function sizeItem() { return sizeCell }
    // Row-relative boxes for the geometry suite, so the probe reads the drawn layout and not the source.
    function markRight() { return markSlot.x + markSlot.width }
    function nameGeom() { return [nameText.x, nameText.width] }
    function sizeGeom() { return [sizeCell.x, sizeCell.width] }
    function chevronGeom() { return [chevronSlot.x, chevronSlot.width] }
}
