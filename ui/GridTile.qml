import QtQuick
import qs.Commons
import "." as Flea
import "js/Density.js" as Density
import "js/Drag.js" as DragOps
import "js/Format.js" as Format
import "js/GridNames.js" as GridNames
import "js/Icons.js" as Icons
import "js/LangMarks.js" as LangMarks

// One grid cell: the same mark the list row draws, in a larger slot, with the name under it.
Item {
    id: root

    property var row: null
    property bool cursor: false
    property bool hovered: false
    // False while the selection band runs, so the wash keeps the highlight off while the tip still reads hovered.
    property bool hoverShown: true
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
    readonly property int clipPx: Theme.clipMarkSize
    // The query List marks: a search's answered query, else the "/" filter's, "" for none; ui/GridArea.qml hands it down.
    property string markQuery: ""
    // True while the strip is up, so the wash builds with the strip and no keystroke pays for it, as List's rows arm.
    property bool markArmed: false
    readonly property string editorText: renameLoader.item ? renameLoader.item.current : ""
    readonly property Item editorField: renameLoader.item as Item
    // Keep an uninitialized Loader visible; a begun editor's captured index survives retirement.
    visible: !(root.editing || root.pooled) || !renameLoader.item
        || renameLoader.item.editIndex < 0 || !renameLoader.item.viewport
        || renameLoader.item.viewport.renameEditor === renameLoader.item
    // The editor's one line box, centred on the caption's first line on whole pixels: it stands a little over the line's top.
    readonly property real renameLineBox: renameLoader.item ? renameLoader.item.lineBox : Theme.rowHeight - 2 * Theme.spacing.rowPaddingY
    readonly property real renameY: Math.round(nameLabel.y + (Theme.grid.captionLineHeight - root.renameLineBox) / 2)
    readonly property real editorExtraHeight: root.editing && renameLoader.item ? Math.max(0, root.renameY - nameLabel.y + renameLoader.item.implicitHeight - Math.max(Theme.grid.captionHeight, Math.ceil(nameLabel.contentHeight)) - Density.gridPadY(Theme.spacing.rowPaddingX, ViewState.density)) : 0
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
    readonly property bool lifted: root.cursor || (root.hovered && root.hoverShown) || root.selected
    // A thumbnail path is not a thumbnail: the cache file can be evicted between the pane's answer
    // and the decode, and a tile whose Image failed to load has to be marked by its kind instead.
    readonly property bool thumbDrawn: root.thumb.length > 0 && tileThumb.status !== Image.Error
    // One filename lookup; only a mapped row without a thumbnail builds the mark's Loader.
    readonly property string langSlug: ViewState.langMarks ? LangMarks.slugForRow(root.row) : ""
    // A search warms the complete mark hidden, so its first answer only changes the slug.
    readonly property bool langWarm: ViewState.langMarks && root.renamePane !== null && root.renamePane.viewMode === "grid" && root.renamePane.searchMode.length > 0 && root.renamePane.swap.searchWarm === true
    property var langLoader: null
    function syncLangMark() { LangMarks.syncMark(root, markGlyph, Qt.resolvedUrl("LangMarkLoader.qml"), Component.Ready) }
    onLangWarmChanged: root.syncLangMark()
    onLangSlugChanged: root.syncLangMark()
    onThumbDrawnChanged: root.syncLangMark()
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
             : (root.hovered && root.hoverShown) ? Style.hoverFill
             : "transparent"
        // Only the cursor gets a frame; marked tiles retain their separate fill.
        border.width: root.cursor ? Theme.spacing.hairline : 0
        border.color: Theme.color.accent
    }

    // The drop wash, and its label further down, build only on the tile under a drag, the way ui/Row.qml builds its drop frame.
    Loader {
        anchors.fill: parent
        active: root.dropTarget
        sourceComponent: Rectangle {
            color: Util.alpha(Theme.color.accent, Style.hoverFillAlpha)
            border.width: Theme.spacing.hairline
            border.color: Theme.color.accent
        }
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
            id: markGlyph
            anchors.fill: parent
            visible: !root.thumbDrawn && root.langSlug === ""
            // A folder mark never grows past 128, centred in the larger slots, so only thumbnails grow.
            maxSize: Density.glyphCap(ViewState.thumbnailPixels)
            name: root.row ? Icons.glyphForRow(root.row.i, root.row.p) : "file"
            // ThemeRoles.dc.html gives accent the selection fill and edge and foreground the label
            // and the mark inside it, so the border carries the emphasis and the ink stays readable.
            color: root.cursor || root.selected ? Theme.color.foreground : Theme.color.mutedFrame
        }
    }

    // One drawn line's piece of the match wash, at the box the mark measured for it.
    component MatchWash: Rectangle {
        id: wash
        property var box: null
        visible: wash.box !== null
        x: wash.box ? wash.box.x : 0
        y: wash.box ? wash.box.y : 0
        width: wash.box ? wash.box.width : 0
    }

    // List's match mark, ui/MatchText.qml's wash behind the matched letters under the unchanged caption, built with the strip.
    Loader {
        id: markLoader
        active: root.markArmed || root.markQuery.length > 0
        visible: !root.editing
        opacity: root.dimOpacity
        x: nameLabel.x
        y: nameLabel.y
        width: nameLabel.width
        height: nameLabel.height
        sourceComponent: Item {
            id: washes
            FontMetrics { id: captionFace; font: nameLabel.font }
            // Only lines Qt drew as Flea broke them are measured; a caption Qt wrapped or elided again would place a box off its letters.
            readonly property bool laidAsBroken: nameLabel.lineCount === nameLabel.text.split("\n").length && !nameLabel.truncated
            readonly property var boxes: washes.laidAsBroken ? washes.measure(GridNames.captionMarks(nameLabel.text, root.markQuery), captionFace.font, washes.width) : []
            // One box per line the run touches; face is read only so a font change re-measures, as RenameField's edgeX does.
            function measure(spans, face, width) {
                return spans.map(function (span) {
                    return { x: (width - captionFace.advanceWidth(span.centred)) / 2 + captionFace.advanceWidth(span.before),
                             y: span.line * Theme.grid.captionLineHeight, width: captionFace.advanceWidth(span.run) }
                })
            }
            readonly property color wash: Qt.rgba(Theme.color.accent.r, Theme.color.accent.g, Theme.color.accent.b, Theme.color.accent.a * Theme.washActive)
            MatchWash { box: washes.boxes.length > 0 ? washes.boxes[0] : null; height: Math.ceil(captionFace.height); color: washes.wash }
            MatchWash { box: washes.boxes.length > 1 ? washes.boxes[1] : null; height: Math.ceil(captionFace.height); color: washes.wash }
        }
    }

    // corner: a filename is arbitrary text, so PlainText; the breaks are Flea's own, WrapAnywhere only catches wide glyphs.
    readonly property int captionLines: root.dropTarget ? 1 : 2
    // Cells of one caption line at the bodySmall advance the label draws at; a wide glyph counts two.
    readonly property int captionPerLine: Theme.bodySmallAdvance > 0 && nameLabel.width > 0 ? Math.floor(nameLabel.width / Theme.bodySmallAdvance) : -1
    Text {
        id: nameLabel
        visible: !root.editing
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

    // A tagged tile mounts its swatches (ui/TagSlot.qml) under the caption on first need; an untagged tile builds nothing.
    property var tagItem: null
    onRowChanged: root.syncTags()
    onRenamingChanged: root.syncTags()
    function syncTags() {
        var names = root.row && !root.renaming ? root.row.tg : undefined
        if (root.tagItem !== null) root.tagItem.tags = names || []
        else if (names !== undefined) root.tagItem = Qt.createComponent("TagSlot.qml").createObject(root, { tags: names, below: nameLabel })
    }

    // Built only on a clipboard tile: a Glyph per tile costs a Shape, and the mark follows the last line it sits beside.
    Loader {
        id: clipLoader
        active: root.clipMark.length > 0 && !root.editing
        width: root.clipPx
        height: root.clipPx
        // One gap past the centred last line's own right end, capped at the strip the margin reserves; a tile with no mark measures nothing.
        x: clipLoader.active ? Math.min(nameLabel.x + (nameLabel.width + GridNames.lastLineCells(nameLabel.text) * Theme.bodySmallAdvance) / 2 + Theme.spacing.gap, nameLabel.x + nameLabel.width + Theme.spacing.gap) : 0
        y: clipLoader.active ? nameLabel.y + Math.min(nameLabel.text.split("\n").length, root.captionLines) * Theme.grid.captionLineHeight - (Theme.grid.captionLineHeight + root.clipPx) / 2 - 1 : 0
        sourceComponent: Flea.Glyph {
            width: root.clipPx
            height: root.clipPx
            name: root.clipMark
            color: Theme.color.mutedFrame
        }
    }

    // Built only while renaming, the way ui/Row.qml builds its own editor.
    Loader {
        id: renameLoader
        active: root.pooled ? root.pooledEditorRow >= 0 : root.renaming
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

    Loader {
        anchors.top: nameLabel.bottom
        anchors.left: nameLabel.left
        anchors.right: nameLabel.right
        height: Theme.grid.captionLineHeight
        active: root.dropTarget
        sourceComponent: Text {
            horizontalAlignment: Text.AlignHCenter
            text: DragOps.label(root.dropCopying, root.dropLinking)
            color: Theme.color.accent
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            textFormat: Text.PlainText
            elide: Text.ElideRight
        }
    }

    // Flea elides the caption itself, so Qt reports truncated only when it had to cut a caption Flea handed over whole.
    function captionElided() {
        return nameLabel.truncated || (root.row ? nameLabel.text.split("\n").join("") !== String(root.row.n) : false)
    }

    // Built only while the host's hover rests on a truncated caption: its size reads the wrapped name's layout, which every tile paid at rest.
    Loader {
        objectName: "nameTip"
        active: !root.editing && !root.dropTarget && root.hovered && root.captionElided()
        z: 1
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: nameLabel.bottom
        anchors.topMargin: Theme.spacing.hairline
        sourceComponent: Rectangle {
            width: Math.min(tip.implicitWidth + 2 * Theme.spacing.gap, Theme.grid.minCellWidth * 2)
            height: tip.implicitHeight + Theme.spacing.gap
            color: Theme.color.surface
            border.width: Theme.spacing.hairline
            border.color: Theme.color.mutedFrame

            Text {
                id: tip
                anchors.centerIn: parent
                width: parent.width - 2 * Theme.spacing.gap
                text: root.row ? root.row.n.replace(/\n/g, "\u21b5").replace(/\t/g, "\u21e5") : ""
                color: Theme.color.foreground
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
            }
        }
    }

    // Pooled by its view, which parks the tile's listing reads on live (ui/GridArea.qml); a pooled tile builds no editor.
    property bool pooled: false
    readonly property bool live: !root.pooled
    // The row whose editor the tile held when pooled, -1 for none; it keeps that editor while the edit stays on that row.
    property int pooledEditorRow: -1
    // The pane's edit row as a pooled tile sees it; once the edit leaves the recorded row the record clears for good.
    readonly property int pooledRename: root.pooled && root.renamePane !== null ? root.renamePane.renamingIndex : -1
    onPooledRenameChanged: if (root.pooledEditorRow >= 0 && root.pooledRename !== root.pooledEditorRow) root.pooledEditorRow = -1
    // Drawn as the editing tile: renaming, or pooled with the editor it held.
    readonly property bool editing: root.renaming || root.pooled && root.pooledEditorRow >= 0
    GridView.onPooled: { root.pooledEditorRow = root.renaming && root.renamePane !== null ? root.renamePane.renamingIndex : -1; root.pooled = true }
    // The kept editor drops first, while every listing read is still parked, so its retirement always owns the edit.
    GridView.onReused: { root.pooledEditorRow = -1; root.pooled = false }
    // What the grid last drew here (GridGeometry.heldRow), a plain object no binding tracks.
    readonly property var rowMemo: ({ row: null, index: -1, listing: 0 })
}
