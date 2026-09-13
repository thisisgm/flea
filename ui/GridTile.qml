import QtQuick
import qs.Commons
import "." as Flea
import "js/Drag.js" as DragOps
import "js/Format.js" as Format
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
    property string thumb: ""
    property bool renaming: false
    property var renamePane: null
    readonly property string editorText: editor.current
    readonly property Item editorField: editor
    readonly property real renameExtraHeight: root.renaming ? Math.max(0, editor.implicitHeight - Theme.grid.captionHeight - Theme.spacing.rowPaddingX) : 0
    signal renameCommitted(string newName)
    signal renameAbandoned()
    function commitEditor() { return editor.commit() }

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
        anchors.topMargin: Theme.spacing.rowPaddingX
        width: ViewState.thumbnailPixels
        height: ViewState.thumbnailPixels

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
            // The tile is the mark's own slot: without its own ceiling Glyph caps a 46 px tile at the 19 px row mark.
            maxSize: ViewState.thumbnailPixels
            name: root.row ? Icons.glyphForRow(root.row.i, root.row.p) : "file"
            // ThemeRoles.dc.html gives accent the selection fill and edge and foreground the label
            // and the mark inside it, so the border carries the emphasis and the ink stays readable.
            color: root.cursor || root.selected ? Theme.color.foreground : Theme.color.muted
        }
    }

    HoverHandler { id: hover }

    // corner: a filename is arbitrary text, so PlainText, the same rule every name on this surface follows.
    Text {
        id: nameLabel
        visible: !root.renaming
        anchors.top: markSlot.bottom
        anchors.topMargin: Theme.spacing.gap
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.rightMargin: Theme.spacing.gap
        height: root.dropTarget ? Theme.grid.captionLineHeight : Theme.grid.captionHeight
        horizontalAlignment: Text.AlignHCenter
        text: root.row ? root.row.n : ""
        color: Theme.color.foreground
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        maximumLineCount: root.dropTarget ? 1 : 2
        lineHeightMode: Text.FixedHeight
        lineHeight: Theme.grid.captionLineHeight
        elide: Text.ElideRight
    }

    Flea.RenameField {
        id: editor
        visible: root.renaming
        anchors { top: nameLabel.top; left: nameLabel.left; right: nameLabel.right }
        height: implicitHeight
        pane: root.renamePane
        name: root.row ? root.row.n.split("/").pop() : ""
        onCommitted: function(newName) { root.renameCommitted(newName) }
        onAbandoned: root.renameAbandoned()
    }

    Text {
        anchors.top: nameLabel.bottom
        anchors.left: nameLabel.left
        anchors.right: nameLabel.right
        visible: root.dropTarget
        height: Theme.grid.captionLineHeight
        horizontalAlignment: Text.AlignHCenter
        text: DragOps.label(root.dropCopying)
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
