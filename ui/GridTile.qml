import QtQuick
import qs.Commons
import "." as Flea
import "js/Density.js" as Density
import "js/Drag.js" as DragOps
import "js/Emblem.js" as Emblem
import "js/Format.js" as Format
import "js/GridNames.js" as GridNames
import "js/Icons.js" as Icons

// One grid cell: the same mark the list row draws, in a larger slot, with the name under it.
Item {
    id: root

    property var row: null
    property bool cursor: false
    property bool hovered: false
    property bool selected: false
    property bool dropTarget: false
    property bool dropCopying: false
    property bool dropLinking: false
    property string thumb: ""
    property bool renaming: false
    property var renamePane: null
    // The grid area derives it per visible tile, so an empty clipboard costs nothing.
    property string clipMark: ""
    // A cut tile dims to the ClipMarks board's own opacity, content only.
    readonly property bool clipCut: root.clipMark === "scissors"
    // One dim for the mark slot and the caption, so one ternary serves both opacities.
    readonly property real dimOpacity: root.clipCut ? Theme.disabledOpacity : 1
    // The mark's own size: 12 px in the muted role, 9 px after the name, per the board.
    readonly property int clipPx: 12
    readonly property string editorText: renameLoader.item ? renameLoader.item.current : ""
    readonly property Item editorField: renameLoader.item as Item
    // Keep an uninitialized Loader visible; a begun editor's captured index survives retirement.
    visible: !root.renaming || !renameLoader.item
        || renameLoader.item.editIndex < 0 || !renameLoader.item.viewport
        || renameLoader.item.viewport.renameEditor === renameLoader.item
    // The editor's one line box, centred on the caption's first line on whole pixels: it stands a little over the line's top.
    readonly property real renameLineBox: renameLoader.item ? renameLoader.item.lineBox : Theme.rowHeight - 2 * Theme.spacing.rowPaddingY
    readonly property real renameY: Math.round(nameLabel.y + (Theme.grid.captionLineHeight - root.renameLineBox) / 2)
    readonly property real editorExtraHeight: root.renaming && renameLoader.item ? Math.max(0, root.renameY - nameLabel.y + renameLoader.item.implicitHeight - Math.max(Theme.grid.captionHeight, Math.ceil(nameLabel.contentHeight)) - Density.gridPadY(Theme.spacing.rowPaddingX, ViewState.density)) : 0
    property real renameExtraHeight: 0
    // Grid layout can pool this Loader while reading its implicit height. Defer and coalesce per tile.
    onEditorExtraHeightChanged: Qt.callLater(root.applyRenameHeight)
    function applyRenameHeight() {
        if (renameLoader.item) {
            renameLoader.item.queueContainment()
            renameLoader.item.extraHeight = root.editorExtraHeight
        }
        root.renameExtraHeight = root.editorExtraHeight
    }
    signal renameCommitted(string newName)
    signal renameAbandoned()
    function commitEditor() { return renameLoader.item ? renameLoader.item.commit() : false }

    // A lifted tile is the cursor, the pointer or a selection member, the same ladder Row.qml climbs.
    readonly property bool lifted: root.cursor || root.hovered || root.selected
    // A thumbnail path is not a thumbnail: the cache file can be evicted between the pane's answer
    // and the decode, and a tile whose Image failed to load has to be marked by its kind instead.
    readonly property bool thumbDrawn: root.thumb.length > 0 && tileThumb.status !== Image.Error
    // The same alias ui/Row.qml carries, so ui/Ipc.qml's rowThumbReady answers for a tile too.
    readonly property alias iconStatus: tileThumb.status
    readonly property Item thumbItem: tileThumb
    readonly property Item captionItem: nameLabel

    Accessible.role: Accessible.ListItem
    Accessible.name: root.row ? root.row.n : ""

    Rectangle {
        anchors.fill: parent
        anchors.margins: Theme.spacing.hairline
        color: root.cursor ? Style.selectedAccentFill
             : root.selected ? Style.selectionFill
             : root.hovered ? Style.hoverFill
             : "transparent"
        // Only the cursor gets a frame; marked tiles retain their separate fill.
        border.width: root.cursor ? Theme.spacing.hairline : 0
        border.color: Theme.color.accent
    }

    Rectangle {
        anchors.fill: parent
        visible: root.dropTarget
        color: Util.alpha(Theme.color.accent, Style.hoverFillAlpha)
        border.width: Theme.spacing.hairline
        border.color: Theme.color.accent
    }

    Item {
        id: markSlot
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: parent.top
        anchors.topMargin: Density.gridPadY(Theme.spacing.rowPaddingX, ViewState.density)
        width: ViewState.thumbnailPixels
        height: ViewState.thumbnailPixels
        opacity: root.dimOpacity

        // A thumbnail is a decoded image and stays one; the glyph beside it is a native mark, and
        // exactly one is visible, chosen the same way ui/Row.qml chooses.
        Image {
            id: tileThumb
            anchors.fill: parent
            visible: root.thumbDrawn
            // Format.fileUri, not a concatenation: a cache path can carry a # or a ? and either one
            // silently truncates a plain file:// URL, which is what the hashcache fixture proves.
            source: root.thumb.length > 0 ? Format.fileUri(root.thumb) : ""
            fillMode: Image.PreserveAspectFit
            sourceSize.width: ViewState.thumbnailPixels
            sourceSize.height: ViewState.thumbnailPixels
            asynchronous: true
            cache: false
        }

        Flea.Glyph {
            anchors.fill: parent
            visible: !root.thumbDrawn
            // A folder mark never grows past 128, centred in the larger slots, so only thumbnails grow.
            maxSize: Density.glyphCap(ViewState.thumbnailPixels)
            name: root.row ? Icons.glyphForRow(root.row.i, root.row.p) : "file"
            // ThemeRoles.dc.html gives accent the selection fill and edge and foreground the label
            // and the mark inside it, so the border carries the emphasis and the ink stays readable.
            color: root.cursor || root.selected ? Theme.color.foreground : Theme.color.muted
        }
    }

    // The sync badge in the icon's corner, made only on a tile a sync tool tagged, see js/Emblem.js.
    property Item emblemItem: null
    function syncEmblem() { root.emblemItem = Emblem.sync(root.emblemItem, Qt.resolvedUrl("EmblemBadge.qml"), markSlot, { targetIcon: markSlot, isGrid: true, status: root.row && root.row.e ? root.row.e : "" }) }
    onRowChanged: root.syncEmblem()
    Component.onCompleted: root.syncEmblem()

    HoverHandler { id: hover }

    // corner: a filename is arbitrary text, so PlainText; the breaks are Flea's own, WrapAnywhere only catches wide glyphs.
    readonly property int captionLines: root.dropTarget ? 1 : 2
    // Cells of one caption line at the bodySmall advance the label draws at; a wide glyph counts two.
    readonly property int captionPerLine: Theme.bodySmallAdvance > 0 && nameLabel.width > 0 ? Math.floor(nameLabel.width / Theme.bodySmallAdvance) : -1
    Text {
        id: nameLabel
        visible: !root.renaming
        opacity: root.dimOpacity
        anchors.top: markSlot.bottom
        anchors.topMargin: Theme.spacing.gap
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.rightMargin: Theme.spacing.gap + (root.clipMark.length > 0 ? Theme.spacing.gap + root.clipPx : 0)
        // The slot fits the laid-out text, keeping the token reserve as its minimum.
        height: Math.max(root.dropTarget ? Theme.grid.captionLineHeight : Theme.grid.captionHeight, Math.ceil(contentHeight))
        horizontalAlignment: Text.AlignHCenter
        text: root.row ? GridNames.gridCaption(root.row.n, root.captionPerLine, root.captionLines) : ""
        color: Theme.color.foreground
        font.family: Theme.font.family
        font.pixelSize: Theme.font.bodySmall
        textFormat: Text.PlainText
        wrapMode: Text.WrapAnywhere
        maximumLineCount: root.captionLines
        lineHeightMode: Text.FixedHeight
        lineHeight: Theme.grid.captionLineHeight
        elide: Text.ElideRight
    }

    // Built only on a clipboard tile: a Glyph per tile costs a Shape, and the mark follows the last line it sits beside.
    Loader {
        id: clipLoader
        active: root.clipMark.length > 0 && !root.renaming
        width: root.clipPx
        height: root.clipPx
        // One gap past the centred last line's own right end, capped at the strip the margin reserves.
        x: Math.min(nameLabel.x + (nameLabel.width + GridNames.lastLineCells(nameLabel.text) * Theme.bodySmallAdvance) / 2 + Theme.spacing.gap, nameLabel.x + nameLabel.width + Theme.spacing.gap)
        y: nameLabel.y + Math.min(nameLabel.text.split("\n").length, root.captionLines) * Theme.grid.captionLineHeight - (Theme.grid.captionLineHeight + root.clipPx) / 2 - 1
        sourceComponent: Flea.Glyph {
            width: root.clipPx
            height: root.clipPx
            name: root.clipMark
            color: Theme.color.muted
        }
    }

    // Built only while renaming, the way ui/Row.qml builds its own editor.
    Loader {
        id: renameLoader
        active: root.renaming
        anchors { left: nameLabel.left; right: nameLabel.right }
        y: root.renameY
        // A normal editor's expansion starts and stays zero, so no height-change signal measures it.
        onLoaded: Qt.callLater(root.applyRenameHeight)
        sourceComponent: Flea.RenameField {
            height: implicitHeight
            editorHost: root
            pane: root.renamePane
            viewport: root.GridView.view
            containOnBegin: true
            name: root.row ? root.row.n.split("/").pop() : ""
            onCommitted: function(newName) { root.renameCommitted(newName) }
            onAbandoned: root.renameAbandoned()
        }
    }

    Text {
        anchors.top: nameLabel.bottom
        anchors.left: nameLabel.left
        anchors.right: nameLabel.right
        visible: root.dropTarget
        height: Theme.grid.captionLineHeight
        horizontalAlignment: Text.AlignHCenter
        text: DragOps.label(root.dropCopying, root.dropLinking)
        color: Theme.color.accent
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
        elide: Text.ElideRight
    }

    Rectangle {
        visible: !root.renaming && !root.dropTarget && hover.hovered && nameLabel.truncated
        z: 1
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: nameLabel.bottom
        anchors.topMargin: Theme.spacing.hairline
        width: Math.min(tip.implicitWidth + 2 * Theme.spacing.gap, Theme.grid.minCellWidth * 2)
        height: tip.implicitHeight + Theme.spacing.gap
        color: Theme.color.surface
        border.width: Theme.spacing.hairline
        border.color: Theme.color.muted

        Text {
            id: tip
            anchors.centerIn: parent
            width: parent.width - 2 * Theme.spacing.gap
            text: root.row ? root.row.n : ""
            color: Theme.color.foreground
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
        }
    }
}
