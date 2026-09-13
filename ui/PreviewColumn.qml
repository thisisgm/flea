import QtQuick
import qs.Commons
import "." as Flea
import "js/Facts.js" as Facts
import "js/Format.js" as Format
import "js/Icons.js" as Icons
import "js/PreviewKeys.js" as PreviewKeys

// The columns view's last pane when a file is picked. One anatomy for all twelve states the canvas
// draws: a frame, an optional transport, the name, and a caption-type table of facts under it.
Item {
    id: root

    property var row: null
    // The meta reply's own fields, whatever ui/Backend.qml's meta signal carries; null until it arrives.
    property var meta: null
    property string kindName: ""
    property string thumb: ""
    // True once no thumbnail is coming: the backend answered with none, or never offered one at all.
    property bool noThumbComing: false
    property int selectionCount: 0
    property var selectedRows: []
    // The row's own absolute path, which the PDF page needs and nothing else here does.
    property string path: ""
    property int pdfControlIndex: -1
    property real pdfZoom: 1
    readonly property real pdfScrollY: pdfFlick.contentY
    readonly property var pdfFrameItem: frame
    readonly property var pdfToolbarItem: pdfToolbar
    readonly property var pdfControls: [pagePrev, pageNext, pdfZoomOut, pdfZoomIn, pdfExpand]
    signal expandRequested()
    // No player exists until the operator presses play. The strip draws from the probe's own
    // duration, so browsing a folder of clips builds no MediaPlayer at all: it costs nothing, makes
    // no sound, and stops QtMultimedia logging a teardown warning on every cursor move.
    property bool wantsPlayback: false
    // The shared Space preview, handed in by ui/ColumnsArea.qml: one file plays in one place.
    property bool overlayOpen: false
    // Where a player may exist at all: this column on screen, no overlay over it, and a strip to drive it; losing any of them ends the play intent.
    readonly property bool playerAllowed: root.visible && !root.overlayOpen && mediaLoader.active
    onPlayerAllowedChanged: if (!root.playerAllowed) root.wantsPlayback = false

    // A new row is a new subject, so whatever was playing stops being this column's business, and
    // the player it needed is torn down with it.
    onPathChanged: { root.wantsPlayback = false; root.pdfZoom = 1; root.pdfControlIndex = -1 }
    onActiveFocusChanged: if (root.activeFocus && root.pdfPages > 0 && root.pdfControlIndex < 0)
        PreviewKeys.pdfAction("focusNext", root)

    // What the row itself is, before Loading or Error can override it. The readers below are built
    // from this and never from previewState: an error that switched its own reader off would clear
    // the error, re-enter the read, and flap between the two states forever.
    readonly property string rowState: root.row !== null && root.row.d !== true
        ? Facts.state(root.row, 1, false, "", root.kindName) : ""

    // Loading and Error are states the column reaches on its own: loading while the facts are still
    // in flight, and error when the thing it was going to draw could not be read at all.
    readonly property bool busy: root.row !== null && root.row.d !== true && root.meta === null
    readonly property string failure: lines.readFailed || root.pdfFailed
        ? "This file could not be read."
        : (root.meta && root.meta.archiveFailed ? "This archive could not be read." : "")

    // The PDF reader exists only for a PDF row, the same rule the media transport follows: browsing
    // a folder of anything else builds no PdfDocument at all.
    readonly property int pdfPages: pdfLoader.item ? pdfLoader.item.pageCount : 0
    readonly property bool pdfFailed: pdfLoader.item ? pdfLoader.item.failed : false

    readonly property string previewState: Facts.state(root.row, root.selectionCount, root.busy, root.failure, root.kindName)
    readonly property var factRows: root.previewState === Facts.MULTI
        ? Facts.multiFacts(root.selectedRows, Date.now(), root.selectionCount)
        : (root.row ? Facts.facts(root.previewState, root.row, root.meta, root.kindName, Date.now(),
                                  { pages: root.pdfPages > 0 ? String(root.pdfPages) : "",
                                    owner: root.meta && root.meta.owner ? root.meta.owner : "" }) : [])

    // The canvas's frame is 16 by 10, which is the one proportion every state shares.
    readonly property real frameRatio: 10 / 16

    // The states that draw a picture of the file itself. A PDF draws its own page, loading draws the
    // crawl, and a multi-selection is a summary and never the cursor row's own picture.
    readonly property bool wantsThumb: root.previewState !== Facts.LOADING
        && root.previewState !== Facts.PDF && root.previewState !== Facts.MULTI
    // A thumbnail path is not a thumbnail: the cache file can be evicted between the pane's answer
    // and the decode, and an Image that failed to load draws nothing at all. The fallback waits for
    // Ready instead, because a file off a slow mount would leave the frame blank for seconds.
    readonly property bool thumbDrawn: root.thumb.length > 0 ? frameThumb.status !== Image.Error
                                                             : frameThumb.status === Image.Ready
    readonly property bool thumbShown: root.wantsThumb && root.thumbDrawn
    // For ui/Ipc.qml's columnFrameRect: the box a playing video's pixels must change inside.
    readonly property Item frameItem: frame
    readonly property Item linesItem: lines
    readonly property Item archiveItem: archivePane
    // Ready is the decoded picture on screen; thumbShown is already true while it loads.
    readonly property alias frameStatus: frameThumb.status
    // The two states whose picture is the thumbnail; audio's mark is what that state draws when it works.
    readonly property bool picturesFromThumb: root.previewState === Facts.IMAGE || root.previewState === Facts.VIDEO

    // One mark per kind in the selection, front-most first; ui/KindStack.qml stacks them.
    readonly property var multiMarks: root.previewState === Facts.MULTI
        ? Facts.multiMarks(root.selectedRows) : []

    Column {
        anchors.fill: parent
        anchors.margins: Theme.spacing.rowPaddingX
        spacing: Theme.spacing.gap

        Rectangle {
            id: frame
            width: parent.width
            height: Math.round(width * root.frameRatio)
            color: Theme.color.background
            border.width: Theme.spacing.hairline
            border.color: Theme.color.muted
            // Mirrors hyprland decoration:rounding, the same as every other floating chrome surface.
            radius: Style.cornerRadius
            clip: true

            // The thumbnail subsystem's own surface: a real thumbnail when there is one, the kind's
            // mark when there is not, which is what the canvas means by "the glyph stands in here".
            Image {
                id: frameThumb
                anchors.fill: parent
                anchors.margins: Theme.spacing.hairline
                visible: root.thumbShown && !playerLoader.visible
                source: root.frameSource()
                fillMode: Image.PreserveAspectFit
                asynchronous: true
                cache: false
                // Zero is unbounded to Qt, which is what the small cache PNG wants; only the fallback,
                // which can be the whole camera file, takes the ceiling ui/PreviewImage.qml sets.
                sourceSize.width: root.thumb.length > 0 ? 0 : Math.max(1, Math.round(width))
                sourceSize.height: root.thumb.length > 0 ? 0 : Math.max(1, Math.round(height))
            }

            // The player, in the frame it paints into; built by the first press of play and not before, and by source rather than type, because QtMultimedia costs 20 MB on import alone.
            Loader {
                id: playerLoader
                anchors.fill: parent
                anchors.margins: Theme.spacing.hairline
                active: root.wantsPlayback && root.playerAllowed
                visible: active && root.previewState === Facts.VIDEO
                source: "PreviewMedia.qml"
                onLoaded: {
                    item.path = Qt.binding(function () { return root.path })
                    item.kind = Qt.binding(function () {
                        return root.previewState === Facts.VIDEO ? "video" : "audio"
                    })
                    // It exists because play was pressed, so it starts.
                    item.autoStart = true
                }
            }

            // The kind's mark, and the one line saying why it is standing in. A frame with neither is
            // the blank-frame defect: an evicted cache file drew a bordered empty box and said nothing.
            Column {
                anchors.centerIn: parent
                spacing: Theme.spacing.gap
                visible: !root.thumbShown && root.glyphState()

                Flea.Glyph {
                    anchors.horizontalCenter: parent.horizontalCenter
                    // A kind standing in for a missing thumbnail is a pane state, not a row mark: PreviewColumn.dc.html draws 40 on its Video tile.
                    maxSize: Theme.stateMarkSize
                    width: Theme.stateMarkSize
                    height: Theme.stateMarkSize
                    name: root.frameGlyph()
                    // A symlink takes the cyan role the row list already gives it; everything else stays muted.
                    color: root.previewState === Facts.SYMLINK ? Theme.color.symlink : Theme.color.muted
                    opacity: 0.7
                }

                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    visible: root.frameNote().length > 0
                    text: root.frameNote()
                    color: Theme.color.muted
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    textFormat: Text.PlainText
                }
            }

            // A multi-selection is a count, so the frame stacks the kinds it holds rather than
            // picking one of them; the front mark is the kind the Kinds row below names first.
            Flea.KindStack {
                anchors.centerIn: parent
                visible: root.previewState === Facts.MULTI && root.multiMarks.length > 0
                marks: root.multiMarks
            }

            // The file's own first lines, which is the frame's whole content for text and code.
            Flea.PreviewLines {
                id: lines
                anchors.fill: parent
                anchors.margins: Theme.spacing.hairline
                visible: root.previewState === Facts.TEXT || root.previewState === Facts.CODE
                active: root.visible && (root.rowState === Facts.TEXT || root.rowState === Facts.CODE)
                path: root.path
                size: root.row ? root.row.s : 0
                numbered: root.previewState === Facts.CODE
            }

            // The PDF's own page, which is the frame's whole content for that state. QtPdf is
            // reached only through this Loader, so a folder with no PDF in it never opens one.
            Flickable {
                id: pdfFlick
                anchors.fill: parent
                anchors.margins: Theme.spacing.hairline
                visible: root.previewState === Facts.PDF
                clip: true
                contentWidth: width * root.pdfZoom
                contentHeight: height * root.pdfZoom
                boundsBehavior: Flickable.StopAtBounds
                Flea.FastScrollHandler { flickable: pdfFlick }
                Loader {
                    id: pdfLoader
                    width: pdfFlick.contentWidth
                    height: pdfFlick.contentHeight
                    active: root.visible && root.rowState === Facts.PDF
                    source: "PreviewPdf.qml"
                    onLoaded: { item.path = Qt.binding(function () { return root.path }); item.active = true }
                }
            }

            // The canvas's Archive tile: the first entries by name, then the count it could not show.
            Flea.PreviewArchive {
                id: archivePane
                anchors.fill: parent
                anchors.margins: Theme.spacing.gap
                visible: root.previewState === Facts.ARCHIVE && root.meta !== null
                meta: root.meta
            }

            Flea.LoadingState {
                anchors.fill: parent
                visible: root.previewState === Facts.LOADING
            }

            // The one sentence an error is, in the theme's error role; the facts below still show.
            Column {
                anchors.centerIn: parent
                visible: root.previewState === Facts.ERROR || root.previewState === Facts.UNSUPPORTED
                spacing: Theme.spacing.gap

                Flea.Glyph {
                    anchors.horizontalCenter: parent.horizontalCenter
                    // A failure mark stands alone, so it takes the pane-state ceiling; the board draws 32 here and 36 for unsupported, and 40 is the one token for both.
                    maxSize: Theme.stateMarkSize
                    width: Theme.stateMarkSize
                    height: Theme.stateMarkSize
                    name: root.previewState === Facts.ERROR ? "alert" : "file"
                    color: root.previewState === Facts.ERROR ? Theme.color.error : Theme.color.muted
                }

                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: root.previewState === Facts.ERROR ? root.failure : "no preview"
                    color: root.previewState === Facts.ERROR ? Theme.color.foreground : Theme.color.muted
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    textFormat: Text.PlainText
                }
            }
        }

        // PDF controls stay outside the page frame, including when a narrow column wraps them.
        Flow {
            id: pdfToolbar
            width: Math.min(pagePrev.width * root.pdfControls.length + pageLabel.width + spacing * root.pdfControls.length,
                parent.width)
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: Theme.spacing.gap
            visible: root.previewState === Facts.PDF && root.pdfPages > 0

            Flea.ChromeButton {
                id: pagePrev
                glyph: "chevron-left"
                keyboardFocused: root.activeFocus && root.pdfControlIndex === 0
                enabled: root.pdfPage() > 0
                onActivated: root.turnPage(-1)
            }

            Text {
                id: pageLabel
                height: pagePrev.height
                verticalAlignment: Text.AlignVCenter
                text: (root.pdfPage() + 1) + " / " + root.pdfPages
                color: Theme.color.muted
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                textFormat: Text.PlainText
            }

            Flea.ChromeButton {
                id: pageNext
                glyph: "chevron-right"
                keyboardFocused: root.activeFocus && root.pdfControlIndex === 1
                enabled: root.pdfPage() + 1 < root.pdfPages
                onActivated: root.turnPage(1)
            }
            Flea.ChromeButton {
                id: pdfZoomOut
                glyph: "minus"
                enabled: root.pdfZoom > 1
                keyboardFocused: root.activeFocus && root.pdfControlIndex === 2
                onActivated: root.zoomBy(-1)
            }
            Flea.ChromeButton {
                id: pdfZoomIn
                glyph: "plus"
                enabled: root.pdfZoom < 4
                keyboardFocused: root.activeFocus && root.pdfControlIndex === 3
                onActivated: root.zoomBy(1)
            }
            Flea.ChromeButton {
                id: pdfExpand
                glyph: "maximize"
                keyboardFocused: root.activeFocus && root.pdfControlIndex === 4
                onActivated: root.toggleExpand()
            }
        }

        // A media file adds the transport strip under the frame, which is what the canvas draws on
        // its Video and Audio tiles. QtMultimedia is reached only through this Loader, on demand.
        Loader {
            id: mediaLoader
            width: parent.width
            height: active ? Theme.chromeHeight : 0
            active: root.visible && (root.previewState === Facts.VIDEO || root.previewState === Facts.AUDIO)
            sourceComponent: mediaTransport
        }

        // corner: a filename is arbitrary text, so PlainText, the same rule every name on this surface follows.
        Text {
            width: parent.width
            text: root.nameText()
            color: Theme.color.foreground
            font.family: Theme.font.family
            font.pixelSize: Theme.font.body
            textFormat: Text.PlainText
            elide: Text.ElideRight
        }

        Flea.FactsTable {
            width: parent.width
            rows: root.factRows
        }
    }

    Component {
        id: mediaTransport

        Item {
            id: transport
            readonly property bool playing: playerLoader.item ? playerLoader.item.status === "playing" : false
            readonly property real position: playerLoader.item ? playerLoader.item.position : 0
            readonly property var strip: strip

            // The player is playerLoader in the frame above; the probe's duration gives the strip a scale until it exists.
            Flea.MediaStrip {
                id: strip
                anchors.fill: parent
                playing: transport.playing
                position: transport.position
                duration: transport.position > 0 && playerLoader.item && playerLoader.item.duration > 0
                          ? playerLoader.item.duration
                          : (root.meta && root.meta.durationMs ? root.meta.durationMs : 0)
                onToggled: {
                    if (!root.wantsPlayback)
                        root.wantsPlayback = true
                    else if (playerLoader.item)
                        playerLoader.item.togglePlay()
                }
                onSeeked: function (ms) { if (playerLoader.item) playerLoader.item.seekTo(ms) }
            }
        }
    }

    // Which page the frame is on, and the turn the chevrons make; both read back over IPC too.
    function pdfPage() { return pdfLoader.item ? pdfLoader.item.page : 0 }
    function turnPage(delta) { if (pdfLoader.item) pdfLoader.item.turn(delta) }
    function zoomBy(steps) { root.pdfZoom = Math.max(1, Math.min(4, root.pdfZoom + steps * 0.25)) }
    function toggleExpand() { root.expandRequested() }
    function scrollPage(delta) {
        pdfFlick.contentY = Math.max(0, Math.min(pdfFlick.contentHeight - pdfFlick.height,
            pdfFlick.contentY + delta * Theme.rowHeight))
    }
    function pdfChevron(dir) { return dir === "left" ? pagePrev : pageNext }
    // Whether a PdfDocument exists at all, which is what makes the Loader worth having.
    function pdfLoaded() { return pdfLoader.item !== null }

    // Read back through shell.qml's IPC so a test can prove the transport plays and seeks.
    function mediaPlaying() { return mediaLoader.item ? mediaLoader.item.playing : false }
    function mediaPosition() { return mediaLoader.item ? mediaLoader.item.position : 0 }
    function mediaStripItem() { return mediaLoader.item ? mediaLoader.item.strip : null }
    // Whether a player object exists at all, for the teardown check; playing false alone would mask one that survived.
    function playerLoaded() { return playerLoader.item !== null }
    // What the text, archive and failure surfaces actually draw, for ui/Ipc.qml: the lines, the member names, the sentence.
    function textLines() { return lines.tooLarge ? "too large" : lines.lines.join("|") }
    function archiveNames() { return root.meta && root.meta.names ? root.meta.names.map(function (e) { return e.n }).join("|") : "" }
    function failureText() { return root.failure }

    // A multi-selection describes a count, not a file, so it names the count instead of a name.
    function nameText() {
        if (root.previewState === Facts.MULTI)
            return root.selectionCount + (root.selectionCount === 1 ? " item selected" : " items selected")
        return root.row ? root.row.n : ""
    }

    // One rule for the whole frame: a state that draws its own content takes the mark away, and
    // gives it back the moment its own reader has nothing on screen. Error, unsupported and loading
    // draw a mark of their own, so they never come back here.
    function glyphState() {
        switch (root.previewState) {
        case Facts.ERROR:
        case Facts.UNSUPPORTED:
        case Facts.LOADING:
            return false
        case Facts.PDF:
            return root.pdfPages <= 0
        case Facts.TEXT:
        case Facts.CODE:
            return lines.blank
        case Facts.ARCHIVE:
            return root.meta === null
        case Facts.MULTI:
            return root.multiMarks.length === 0
        }
        return true
    }

    // The cache file while there is one, then the image itself once the backend says none is coming.
    function frameSource() {
        if (!root.visible) return ""
        if (root.thumb.length > 0)
            return Format.fileUri(root.thumb)
        if (root.noThumbComing && root.previewState === Facts.IMAGE && root.path.length > 0)
            return Format.fileUri(root.path)
        return ""
    }

    // Why the frame is showing a mark instead of the thing it meant to draw. Empty for every state
    // whose mark is simply what it draws, so the line only ever appears when something went wrong.
    function frameNote() {
        if (root.wantsThumb && root.thumb.length > 0 && !root.thumbDrawn) {
            return "thumbnail unavailable"
        }
        // A mark with no line reads as a finished preview, which is how a whole network mount looked
        // like it held no pictures while its audio, whose mark is its finished state, looked fine.
        if (root.wantsThumb && root.noThumbComing && root.picturesFromThumb && !root.thumbDrawn) {
            return "no preview could be made"
        }
        if ((root.previewState === Facts.TEXT || root.previewState === Facts.CODE) && lines.tooLarge) {
            return "too large to preview"
        }
        return ""
    }

    function frameGlyph() {
        if (root.previewState === Facts.SYMLINK)
            return "symlink"
        if (root.previewState === Facts.MULTI)
            return "file"
        return root.row ? Icons.glyphForRow(root.row.i, root.row.p) : "file"
    }
}
