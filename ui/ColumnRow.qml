import QtQuick
import qs.Commons
import "." as Flea
import "js/Drag.js" as DragOps
import "js/Icons.js" as Icons
import "js/LangMarks.js" as LangMarks
import "js/Format.js" as Format
import "js/Match.js" as Match

// One row of a Miller column: a mark, a name, and the chevron a chosen directory carries. Simpler
// than a list row on purpose, because a column has no size, date, mode or kind to draw.
Item {
    id: root

    // {n, d, i} as a peek answers them, or a listing row, which carries the same three fields.
    property var row: null
    // The pane's thumbnail path for this row, empty for a peek's row: only the listing's own column asks for any.
    property string thumb: ""
    // A path is not a thumbnail: a cache file evicted between the answer and the decode leaves the mark to the glyph.
    readonly property bool thumbDrawn: root.thumb.length > 0 && thumbImage.status !== Image.Error
    // One filename lookup; only a mapped row without a thumbnail builds the mark's Loader.
    readonly property string langSlug: ViewState.langMarks ? LangMarks.slugForRow(root.row) : ""
    // A search warms the complete mark hidden, so its first answer only changes the slug.
    readonly property bool langWarm: ViewState.langMarks && root.shown && root.pane !== null && root.pane.searchMode.length > 0 && root.pane.swap.searchWarm === true
    property var langLoader: null
    function syncLangMark() { LangMarks.syncMark(root, markGlyph, Qt.resolvedUrl("LangMarkLoader.qml"), Component.Ready) }
    onLangWarmChanged: root.syncLangMark()
    onLangSlugChanged: root.syncLangMark()
    onThumbDrawnChanged: root.syncLangMark()
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
    // False while ui/ColumnPane.qml's view holds the row pooled, which parks its listing reads and turns its hover off.
    property bool live: true
    ListView.onPooled: root.live = false
    ListView.onReused: root.live = true
    property bool dropTarget: false
    property bool dropCopying: false
    property bool dropLinking: false
    // The area derives it per visible row, so an empty clipboard costs nothing.
    property string clipMark: ""
    // A cut row dims to the ClipMarks board's own opacity, content only.
    readonly property bool clipCut: root.clipMark === "scissors"
    // One dim for the content below, so one ternary serves the mark, the name and the size.
    readonly property real dimOpacity: root.clipCut ? Theme.disabledOpacity : 1
    // One helper, so each dimmed colour multiplies the cut without a second binding.
    function dimmed(base) { return Util.alpha(base, root.dimOpacity) }
    // The mark's own size: 12 px in the muted role, 9 px after the name, per the board.
    readonly property int clipPx: Theme.clipMarkSize
    // The pane's own column hands it over; a peek has none and never marks.
    property var pane: null
    // The column is drawn: a hidden Columns view keeps its rows still and builds no mark on them.
    readonly property bool shown: root.ListView.view === null || root.ListView.view.visible
    // The query List marks on this row, "" for none: a search's answered query, else the "/" filter's.
    readonly property string markQuery: root.pane === null || !root.shown ? "" : root.pane.searchMode.length > 0 ? root.pane.swap.answeredQuery : root.pane.filterQuery
    readonly property bool marking: root.markQuery.length > 0 && root.row !== null
    // List's markArmed: built with the filter strip, or after a search strip's first frame on pooled rows too, so no key or answer builds it.
    readonly property bool markArmed: root.shown && root.pane !== null && (root.pane.searchMode.length > 0 ? root.pane.swap.searchWarm === true : root.row !== null && (root.pane.filterTyping || root.pane.filterQuery.length > 0))
    // The column hands one budget for every row, with ElideMiddle as the backstop.
    property int nameBudget: -1
    property real paintWidth: 0 // Caller view width; fills paint to it under the lane, content keeps rowWidth.

    // Truthiness, like the two readers below: ui/ColumnPane.qml hands this rows[index] raw, so a
    // listing that shrank leaves a surviving delegate holding undefined, which is not null.
    readonly property bool isDir: !!root.row && root.row.d === true
    // The cut dim lives in the ink, so no child carries its own opacity binding.
    readonly property color ink: root.dimmed(root.cursor ? Theme.color.accent : root.dim ? Theme.color.muted : Theme.color.foreground)

    // ui/ColumnPane.qml sets it on the renaming (cursor) row: the error line pushes the rows below down as ui/Row.qml's does.
    property real errorGrowth: 0
    // Set by ui/ColumnPane.qml while its editor stands over this row, so the name under it is not drawn twice.
    property bool renaming: false
    // The editor is drawn over the row by ui/ColumnPane.qml, so its y is plain arithmetic on this row height.
    implicitHeight: Theme.fileRowHeight + root.errorGrowth

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
        width: Theme.spacing.hairline * 2
        height: parent.height
        color: root.cursor ? Theme.color.accent : "transparent"
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

    // The row's first line: its content centres here, so an error line that grows the row never moves the name.
    Item {
        id: line
        width: parent.width
        height: Theme.fileRowHeight
    }

    Item {
        id: markSlot
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: line.verticalCenter
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
            visible: !root.thumbDrawn && root.langSlug === ""
            name: root.row ? Icons.glyphForRow(root.row.i, root.row.p) : "file"
            color: root.dimmed(root.cursor ? Theme.color.accent : Theme.color.mutedFrame)
        }
    }

    // corner: arbitrary filename text draws PlainText and elides in the middle per Names040.
    Text {
        id: nameText
        anchors.left: markSlot.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.right: sizeCell.left
        anchors.rightMargin: (root.row && root.row.shown ? Theme.tagSlotWidth + Theme.spacing.gap : 0) + (root.showSize ? Theme.spacing.gap : 0) + (root.clipMark.length > 0 ? Theme.spacing.gap + root.clipPx : 0)
        anchors.verticalCenter: line.verticalCenter
        visible: !root.renaming && !root.marking
        text: root.row && root.nameBudget >= 0 ? Format.middleElide(root.row.n.replace(/\n/g, "\u21b5").replace(/\t/g, "\u21e5"), Math.max(0, root.nameBudget - (root.clipMark.length > 0 ? Math.ceil((Theme.spacing.gap + root.clipPx) / Theme.bodyAdvance) : 0))) : (root.row ? root.row.n.replace(/\n/g, "\u21b5").replace(/\t/g, "\u21e5") : "")
        color: root.ink
        font.family: Theme.font.family
        font.pixelSize: Theme.font.body
        textFormat: Text.PlainText
        elide: Text.ElideMiddle
    }

    // A tagged row mounts its swatches (ui/TagSlot.qml) on first need and keeps them; an untagged row builds nothing.
    property var tagItem: null
    onRowChanged: {
        var names = root.row ? root.row.tg : undefined
        if (root.tagItem !== null) root.tagItem.tags = names || []
        else if (names !== undefined) root.tagItem = Qt.createComponent("TagSlot.qml").createObject(root, { tags: names, after: nameText, hug: true })
    }

    // Only the active column carries a number (ColumnsTabs board rule 3); the size holds its right edge off the parent by the chevron width.
    Text {
        id: sizeCell
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX + chevronSlot.width + (root.showSize ? Theme.spacing.gap : 0)
        anchors.verticalCenter: line.verticalCenter
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

    // The drop label, the clip mark and the match mark build above the row content, so a row at rest builds none.
    Loader {
        id: dropClipLoader
        active: root.dropTarget || root.clipMark.length > 0 || root.marking || root.markArmed
        anchors.fill: line
        sourceComponent: Item {
            readonly property Item label: dropParts.item ? dropParts.item.label : null
            readonly property Item mark: dropParts.item ? dropParts.item.mark : null
            property alias names: markContent
            anchors.fill: parent
            // List's match mark and search split, built while a query marks or the strip is up: a row at rest builds none.
            Loader {
                id: markContent
                active: root.marking || root.markArmed
                x: root.markX()
                width: root.markWidth()
                height: parent.height
                visible: root.marking && !root.renaming
                sourceComponent: Item {
                    id: markParts
                    property alias label: matchName
                    readonly property Item folder: folderLoader.item as Item
                    // A search row draws its folder beside the name, as List's locating rows do; a "/" filter row keeps the plain name.
                    readonly property bool locating: root.pane !== null && root.pane.searchMode.length > 0
                    readonly property string path: root.row ? root.row.n : ""
                    readonly property string shownName: markParts.locating ? Match.base(path) : path
                    readonly property var run: Match.run(shownName, root.markQuery)
                    Flea.MatchText {
                        id: matchName
                        anchors.left: parent.left
                        anchors.verticalCenter: parent.verticalCenter
                        width: markParts.locating ? Math.min(implicitWidth, parent.width * Theme.searchNameShare) : parent.width
                        text: parent.shownName
                        matchStart: parent.run.start
                        matchLength: parent.run.length
                        color: root.ink
                        accent: root.dimmed(Theme.color.accent)
                        armed: root.markArmed
                        elideMiddle: true
                    }
                    // Only a search row draws its folder; a "/" filter row builds none.
                    Loader {
                        id: folderLoader
                        active: markParts.locating
                        anchors.left: matchName.right
                        anchors.leftMargin: Theme.spacing.gap + (root.clipMark.length > 0 ? Theme.spacing.gap + root.clipPx : 0)
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        sourceComponent: Text {
                            text: Match.location(markParts.path)
                            // List's metadata ink (ui/Row.qml cellInk): foreground on a lifted row, a darker foreground at rest.
                            color: root.dimmed(root.cursor || root.lifted || root.hovered || root.selected || root.dropTarget ? Theme.color.foreground : Theme.color.metadata)
                            font.family: Theme.font.family
                            font.pixelSize: Theme.font.caption
                            elide: Text.ElideLeft
                            textFormat: Text.PlainText
                        }
                    }
                }
            }
            // Built only on a drop target or a clipped row, so a marked row builds neither.
            Loader {
                id: dropParts
                active: root.dropTarget || root.clipMark.length > 0
                anchors.fill: parent
                sourceComponent: Item {
                    property alias label: dropLabel
                    property alias mark: clipMarkGlyph
                    // The label the glyph follows: the match label while a query marks, else the plain name.
                    readonly property Item clipLabel: root.marking && markContent.item ? markContent.item.label : nameText
                    Text {
                        id: dropLabel
                        visible: root.dropTarget
                        anchors.right: parent.right
                        anchors.rightMargin: Theme.spacing.rowPaddingX
                        anchors.verticalCenter: parent.verticalCenter
                        text: DragOps.label(root.dropCopying, root.dropLinking)
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
                        x: (root.marking && markContent.item ? markContent.x : 0) + parent.clipLabel.x + Math.min(parent.clipLabel.implicitWidth, parent.clipLabel.width) + Theme.spacing.gap
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.verticalCenterOffset: -1
                        name: root.clipMark
                        color: Theme.color.mutedFrame
                    }
                }
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
        anchors.verticalCenter: line.verticalCenter
        width: root.dropTarget && dropClipLoader.item && dropClipLoader.item.label ? dropClipLoader.item.label.implicitWidth : root.showChevron ? Theme.font.caption : 0
        height: root.dropTarget && dropClipLoader.item && dropClipLoader.item.label ? dropClipLoader.item.label.implicitHeight : Theme.font.caption

        // Built only on the row that draws it, so a row at rest holds no Shape for it, and kept once built so a cursor that moves on hides it without a destroy and rebuild in the frame an answer lands.
        Loader {
            id: chevronLoader
            property bool built: false
            readonly property bool drawn: root.showChevron && !root.dropTarget
            anchors.fill: parent
            visible: drawn
            active: built || drawn
            onDrawnChanged: if (drawn) built = true
            Component.onCompleted: if (drawn) built = true
            sourceComponent: Flea.Glyph {
                name: "chevron-right"
                color: root.cursor ? Theme.color.accent : Theme.color.mutedFrame
            }
        }
    }

    readonly property bool showChevron: root.isDir && (root.cursor || root.lifted)

    HoverHandler {
        id: hover
        enabled: root.live
        onHoveredChanged: root.hovered = hovered
    }

    // Test seam as functions, so no row at rest binds to name geometry (tests/columnscost.qml).
    function clipGlyph() { var m = dropClipLoader.item ? dropClipLoader.item.mark : null; return m && m.visible && m.width > 0 ? m : null }
    function dropClipBuilt() { return dropClipLoader.item !== null && dropClipLoader.item.mark !== null }
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
    function clipExpectedX() { var n = root.nameItem(); return n.mapToItem(root, 0, 0).x + Math.min(n.implicitWidth, n.width) + Theme.spacing.gap }
    // Where a marked label starts and how wide it may run: after the mark slot, short of the size cell and the clip mark.
    function markX() { return markSlot.x + markSlot.width + Theme.spacing.gap }
    function markWidth() { return Math.max(0, sizeCell.x - root.markX() - (root.showSize ? Theme.spacing.gap : 0) - (root.clipMark.length > 0 ? Theme.spacing.gap + root.clipPx : 0)) }
    function displayText() { return nameText.text }
    // The drawn label: the match mark while a query marks, so a slow click still lands on the visible name.
    function markPart() { var c = root.marking ? dropClipLoader.item : null; return c ? c.names.item : null }
    function nameItem() { var p = root.markPart(); return p ? p.label : nameText }
    function folderItem() { var p = root.markPart(); return p ? p.folder : null }
    function markItem() { return markGlyph }
    function sizeItem() { return sizeCell }
    // Row-relative boxes for the geometry suite, so the probe reads the drawn layout and not the source.
    function markRight() { return markSlot.x + markSlot.width }
    function nameGeom() { return [nameText.x, nameText.width] }
    function sizeGeom() { return [sizeCell.x, sizeCell.width] }
    function chevronGeom() { return [chevronSlot.x, chevronSlot.width] }
}
