import QtQuick
import qs.Commons
import "." as Flea
import "js/Facts.js" as Facts
import "js/Format.js" as Format
import "js/Icons.js" as Icons
import "js/KeyName.js" as KeyName
import "js/Kinds.js" as Kinds
import "js/Thumbs.js" as Thumbs
import "js/ExtThumbs.js" as ExtThumbs
// The columns view's last pane when a file is picked. One anatomy for all twelve states the canvas
// draws: a frame, an optional transport, the name, and a caption-type table of facts under it.
Item {
    id: root

    property var row: null
    // The meta reply's own fields, whatever ui/Backend.qml's meta signal carries; null until it arrives.
    property var meta: null
    property string kindName: ""
    property string thumb: ""
    // ExtThumbs: a class that is off holds the frame on its facts until Ctrl+Space loads it.
    property bool manualHold: false
    // ExtThumbs: how much of a text file the frame may read; 256 KiB on network and phone storage.
    property int textLimit: 1048576
    // True where the board truncates instead of refusing: an over-limit remote file shows its first bytes.
    property bool truncateText: false
    // True once no thumbnail is coming: the backend answered with none, or never offered one at all.
    property bool noThumbComing: false
    // A move cleared the row and its settle is running, so the column is loading, not showing a file with no preview.
    property bool pending: false
    // Set by ui/PreviewSwap.qml when a hold ran out, which already spent LoadingState's own hold-off.
    property bool loadingHeldOff: false
    property int selectionCount: 0
    property var selectedRows: []
    property string path: ""
    readonly property real pdfScrollY: pdfFlick.contentY
    readonly property var pdfFrameItem: frame
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
    onPathChanged: root.wantsPlayback = false

    // What the row itself is, before Loading or Error can override it. The readers below are built
    // from this and never from previewState: an error that switched its own reader off would clear
    // the error, re-enter the read, and flap between the two states forever.
    readonly property string rowState: root.row !== null && root.row.d !== true
        ? Facts.state(root.row, 1, false, "", root.kindName) : ""

    // Loading and Error are states the column reaches on its own: loading while the facts are still
    // in flight, and error when the thing it was going to draw could not be read at all. A held
    // frame is neither: its facts are the listing's own and there is nothing in flight.
    readonly property bool busy: root.pending || (root.row !== null && root.row.d !== true && root.meta === null && !root.manualHold)
    readonly property string pdfSentence: pdfLoader.item ? pdfLoader.item.failSentence : ""
    readonly property string failure: lines.readFailed || root.pdfFailed || (root.isHtmlRow && root.markdown && root.markdown.failed)
        ? (root.pdfFailed && root.pdfSentence.length > 0 ? root.pdfSentence : "This file could not be read.")
        : (root.meta && root.meta.archiveFailed ? "This archive could not be read." : "")

    // The PDF reader exists only for a PDF row, the same rule the media transport follows: browsing
    // a folder of anything else builds no PdfDocument at all.
    readonly property int pdfPages: pdfLoader.item ? pdfLoader.item.pageCount : 0
    readonly property bool pdfFailed: pdfLoader.item ? pdfLoader.item.failed : false
    readonly property bool pdfDrawn: pdfLoader.item ? pdfLoader.item.shownPage >= 0 : false

    readonly property string previewState: Facts.state(root.row, root.selectionCount, root.busy, root.failure, root.kindName)
    // What the rows and the facts line state that the listing and the meta reply do not carry.
    readonly property var factExtra: ({ pages: root.pdfPages > 0 ? String(root.pdfPages) : "",
                                        owner: root.meta && root.meta.owner ? root.meta.owner : "" })
    readonly property var factRows: root.previewState === Facts.MULTI
        ? Facts.multiFacts(root.selectedRows, root.selectionCount)
        : (root.row ? Facts.facts(root.previewState, root.row, root.meta, root.kindName, root.factExtra) : [])

    // The canvas's frame is 16 by 10, the one proportion every state shares but a tall kind's, which fills the column down to the name.
    readonly property real frameRatio: 10 / 16
    readonly property bool tallRow: root.previewState !== Facts.MULTI && Facts.tall(root.rowState)
    // Document rows always render in the tall column frame, independently of Quick Look.
    readonly property bool isMarkdownRow: root.rowState === Facts.TEXT && root.row !== null
        && Kinds.isMarkdown(root.row.n)
    readonly property bool isHtmlRow: root.rowState === Facts.CODE && root.row !== null && Kinds.isHtml(root.row.n)
    readonly property bool isDocumentRow: root.isMarkdownRow || root.isHtmlRow
    // The document pane stays null without a document row, so every reader guards the loader item.
    readonly property var fontItem: fontLoader.item
    readonly property var markdown: markdownLoader.item
    // The document reader's activation is shared by the Loader and its item so the two cannot drift apart.
    readonly property bool markdownActive: root.visible && !root.manualHold && root.isDocumentRow && (!root.isHtmlRow || root.selectionCount <= 1)
    // Stretch renders a vector to the whole box, so the frame's pictures keep Fit for an SVG alone.
    readonly property bool vectorPath: /\.svgz?$/i.test(root.path)
    // EXIF orientations 5 to 8 swap the sides, and Qt fits its decode before it turns, so the box it is asked for turns too.
    readonly property bool turned: root.meta !== null && root.meta.orient >= 5

    // The states that draw a picture of the file itself. A PDF draws its own page, loading draws the
    // crawl, and a multi-selection is a summary and never the cursor row's own picture.
    readonly property bool wantsThumb: root.previewState !== Facts.LOADING
        && root.previewState !== Facts.FONT && root.previewState !== Facts.PDF && root.previewState !== Facts.MULTI
    // A thumbnail path is not a thumbnail: the cache file can be evicted between the pane's answer
    // and the decode, and an Image that failed to load draws nothing at all. The fallback waits for
    // Ready instead, because a file off a slow mount would leave the frame blank for seconds.
    readonly property bool thumbDrawn: root.thumb.length > 0 ? frameThumb.status !== Image.Error
                                                             : frameThumb.status === Image.Ready
    readonly property bool thumbShown: root.wantsThumb && root.thumbDrawn
    // For ui/Ipc.qml's columnFrameRect: the box a playing video's pixels must change inside.
    readonly property Item frameItem: frame
    // For ui/Ipc.qml's columnPictureRect: the player's content rect while it plays, else the poster;
    // frameThumb keeps its geometry hidden under the player, so reading it then measures a stale poster.
    readonly property Item pictureItem: playerLoader.visible && playerLoader.item ? playerLoader.item.contentItem : frameThumb
    readonly property Item linesItem: lines
    readonly property Item archiveItem: archivePane
    readonly property alias factsLine: facts.line
    // A 3D model's original is never decoded: Qt has no plugin for it, and each attempt logs a warning.
    readonly property bool originalRefused: Kinds.isModel(root.path)
    // Ready is the decoded picture (thumbShown is true while it loads); a model with no thumbnail coming reads Error, which ui/js/PreviewSwap.js treats as settled.
    readonly property int frameStatus: root.originalRefused && root.thumb.length === 0 && root.noThumbComing ? Image.Error : frameThumb.status
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
            height: root.tallRow ? Facts.tallHeight(root.height, Theme.spacing.rowPaddingX, nameText.height, facts.height, Theme.spacing.gap) : Math.round(width * root.frameRatio)
            // The listing's own colour, so on glass the pane's ground under it is the fill; see ui/Glass.qml.
            color: Glass.on ? "transparent" : Theme.color.background
            border.width: Theme.spacing.hairline
            border.color: Theme.color.mutedFrame
            // Mirrors hyprland decoration:rounding, the same as every other floating chrome surface.
            radius: Style.cornerRadius
            clip: true

            // The thumbnail subsystem's own surface: a real thumbnail when there is one, the kind's
            // mark when there is not, which is what the canvas means by "the glyph stands in here".
            // Sized the way ui/PreviewImage.qml sizes its picture, and for its reasons: the exact fit, never enlarged in the decode.
            Image {
                id: frameThumb
                readonly property real boxWidth: parent.width - 2 * Theme.spacing.hairline
                readonly property real boxHeight: parent.height - 2 * Theme.spacing.hairline
                readonly property bool vector: root.thumb.length === 0 && root.vectorPath
                // The fallback is the original, so never enlarged; a cache file up to the original's own size.
                // A video poster is the exception: it draws at the player's size, enlarged when the clip is small.
                readonly property real fit: Thumbs.fitScale(boxWidth, boxHeight, implicitWidth, implicitHeight, root.thumb.length === 0 ? 1
                    : Thumbs.posterLimit(root.previewState === Facts.VIDEO, implicitWidth, implicitHeight, root.meta ? root.meta.w : 0, root.meta ? root.meta.h : 0))
                x: vector ? Theme.spacing.hairline : Math.round((parent.width - width) / 2)
                y: vector ? Theme.spacing.hairline : Math.round((parent.height - height) / 2)
                width: vector ? boxWidth : implicitWidth * fit
                height: vector ? boxHeight : implicitHeight * fit
                visible: root.thumbShown && !playerLoader.visible && !(gifLoader.item && gifLoader.item.ready)
                // Owned by root.applyDecodeTarget, never bound: a bound url and a bound
                // ceiling land in separate passes and each pass reloads the standing file.
                source: ""
                fillMode: vector ? Image.PreserveAspectFit : Image.Stretch
                // The fallback is the camera file itself, whose EXIF turn Qt applies only when asked.
                autoTransform: true
                asynchronous: true
                cache: false
                Component.onCompleted: root.applyDecodeTarget()
            }

            // An animated GIF moves only after its still frame is drawn, and not while the column is hidden or covered; the still stays until the first moving frame is ready.
            Loader {
                id: gifLoader
                x: frameThumb.x
                y: frameThumb.y
                width: frameThumb.width
                height: frameThumb.height
                active: root.previewState === Facts.IMAGE && root.meta !== null && root.meta.frames > 1
                    && /\.gif$/i.test(root.path) && frameThumb.status === Image.Ready
                source: "PreviewAnimation.qml"
                onLoaded: {
                    item.path = Qt.binding(function () { return root.path })
                    item.shown = Qt.binding(function () { return root.visible && !root.overlayOpen })
                }
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
                    item.letterbox = Qt.binding(function () { return Glass.on ? "transparent" : Theme.color.background })
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
                    color: root.previewState === Facts.SYMLINK ? Theme.color.symlink : Theme.color.mutedFrame
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
                visible: (root.previewState === Facts.TEXT || root.previewState === Facts.CODE) && !root.isDocumentRow
                active: root.visible && !root.manualHold && (root.rowState === Facts.TEXT || root.rowState === Facts.CODE) && !root.isDocumentRow
                path: root.path
                size: root.row ? root.row.s : 0
                maxBytes: root.textLimit
                truncate: root.truncateText
                backend: root.pane ? root.pane.backend : null
                numbered: root.previewState === Facts.CODE
            }
            // The document body flows tall with no toggle; a file-path Loader compiles it only when needed.
            Loader {
                id: markdownLoader
                anchors.fill: parent
                anchors.margins: Theme.spacing.hairline
                active: root.markdownActive
                visible: root.isDocumentRow && (!root.isHtmlRow || root.selectionCount <= 1)
                source: root.isHtmlRow ? "PreviewHtml.qml" : "PreviewMarkdown.qml"
                onLoaded: {
                    item.path = Qt.binding(function () { return root.path })
                    item.size = Qt.binding(function () { return root.row ? root.row.s : 0 })
                    item.maxBytes = Qt.binding(function () { return root.textLimit })
                    item.active = Qt.binding(function () { return root.markdownActive })
                    item.truncate = Qt.binding(function () { return root.truncateText })
                    item.compact = true
                    item.externalScrollBars = true
                }
            }
            // The document reader takes the pane's backend once loaded, so Source and Markdown fences colour through the engine.
            Binding {
                target: markdownLoader.item
                property: "backend"
                value: root.pane ? root.pane.backend : null
                when: markdownLoader.status === Loader.Ready
            }
            Loader {
                id: fontLoader
                anchors.fill: parent
                anchors.margins: Theme.spacing.hairline
                active: root.visible && !root.manualHold && !root.overlayOpen && root.previewState === Facts.FONT
                source: "PreviewFont.qml"
                onLoaded: {
                    item.compact = true
                    item.path = Qt.binding(function () { return root.path })
                    item.expandRequested.connect(root.expandRequested)
                }
            }

            // The PDF's pages, one under another at the frame's width; QtPdf is reached only through this Loader, so a folder with no PDF never opens one.
            Flickable {
                id: pdfFlick
                anchors.fill: parent
                anchors.margins: Theme.spacing.hairline
                visible: root.previewState === Facts.PDF
                clip: true
                contentWidth: width
                contentHeight: pdfLoader.height
                flickableDirection: Flickable.VerticalFlick
                boundsBehavior: Flickable.StopAtBounds
                Flea.FastScrollHandler { flickable: pdfFlick }
                Flea.ViewportScrollBars { parent: pdfFlick; flickable: pdfFlick }
                Loader {
                    id: pdfLoader
                    width: pdfFlick.width
                    height: item ? item.stackHeight : 0
                    active: root.visible && !root.manualHold && root.rowState === Facts.PDF
                    source: "PreviewPdf.qml"
                    onLoaded: {
                        item.continuous = true
                        item.path = Qt.binding(function () { return root.path })
                        item.viewport = pdfFlick
                        item.active = true
                        // A document on a hangable class loads from the backend's fetched copy.
                        item.backend = Qt.binding(function () { return root.pane ? root.pane.backend : null })
                        item.fetchFirst = Qt.binding(function () { return root.pane ? ExtThumbs.present(root.pane.storageClass) : false })
                        item.viewerSlot = "column"
                    }
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
                heldOff: root.loadingHeldOff
            }

            // The one sentence an error is, in the theme's error role; the facts below still show. An office file's
            // embedded thumbnail is the frame's own picture, so Unsupported says "no preview" only when there is none.
            Column {
                anchors.centerIn: parent
                visible: root.previewState === Facts.ERROR || (root.previewState === Facts.UNSUPPORTED && !root.thumbShown)
                spacing: Theme.spacing.gap

                Flea.Glyph {
                    anchors.horizontalCenter: parent.horizontalCenter
                    // A failure mark stands alone, so it takes the pane-state ceiling; the board draws 32 here and 36 for unsupported, and 40 is the one token for both.
                    maxSize: Theme.stateMarkSize
                    width: Theme.stateMarkSize
                    height: Theme.stateMarkSize
                    name: root.previewState === Facts.ERROR ? "alert" : "file"
                    color: root.previewState === Facts.ERROR ? Theme.color.error : Theme.color.mutedFrame
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
            id: nameText
            width: parent.width
            text: root.nameText()
            color: Theme.color.foreground
            font.family: Theme.font.family
            font.pixelSize: Theme.font.body
            textFormat: Text.PlainText
            elide: Text.ElideRight
        }

        Flea.PreviewFacts {
            id: facts
            width: parent.width
            compact: root.tallRow && ViewState.previewFacts === "compact"
            line: Facts.compactLine(root.rowState, root.row, root.meta, root.kindName, root.factExtra)
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

    Flea.PreviewScroll {
        id: scrolling
        column: root
    }

    function pageBy(direction) { scrolling.pageBy(direction) }
    // The page under the frame's top, read back over IPC too, and the turn that scrolls to the next or previous one.
    function pdfPage() { return pdfLoader.item ? pdfLoader.item.page : 0 }
    function turnPage(delta) { if (pdfLoader.item) pdfLoader.item.turn(delta) }
    function toggleExpand() { root.expandRequested() }
    function scrollPage(delta) { scrolling.scrollPage(delta, pdfFlick) }
    // Whether a PdfDocument exists at all, which is what makes the Loader worth having.
    function pdfLoaded() { return pdfLoader.item !== null }
    function pdfLanded() { return pdfLoader.item ? pdfLoader.item.landedPages() : [] }

    // For the geometry gates: the box of the page under the frame's top, or of page index, null until the document below built it.
    function pdfPageItem(index) { return pdfLoader.item ? pdfLoader.item.pageSlot(index === undefined ? pdfLoader.item.page : index) : null }
    // Read back through shell.qml's IPC so a test can prove the transport plays and seeks.
    function mediaPlaying() { return mediaLoader.item ? mediaLoader.item.playing : false }
    function mediaPosition() { return mediaLoader.item ? mediaLoader.item.position : 0 }
    function mediaStripItem() { return mediaLoader.item ? mediaLoader.item.strip : null }
    // Whether a player object exists at all, for the teardown check; playing false alone would mask one that survived.
    function playerLoaded() { return playerLoader.item !== null }
    // What the text, archive and failure surfaces actually draw, for ui/Ipc.qml: the lines, the member names, the sentence.
    function textLines() { return lines.tooLarge ? "too large" : lines.lines.join("|") }
    // Sample input: "# Notes\n###\ntext" reads "Notes\n\ntext"; source view returns the bytes.
    function markdownText() {
        if (!root.isMarkdownRow || !markdown) return ""
        var body = markdown.rawText || ""
        if (markdown.shownView === "source") return body
        var out = body.split("\n")
        for (var i = 0; i < out.length; i++) {
            var head = /^#{1,6}\s/.exec(out[i])
            if (head !== null) out[i] = out[i].slice(head[0].length)
            else if (/^#{1,6}$/.test(out[i])) out[i] = ""
        }
        return out.join("\n")
    }
    readonly property bool textLoading: scrolling.loading
    function archiveNames() { return root.meta && root.meta.names ? root.meta.names.map(function (e) { return e.n }).join("|") : "" }
    function failureText() { return root.failure }
    function toggleFacts() { ViewState.changeKey("previewFacts", ViewState.previewFacts === "compact" ? "full" : "compact") }

    // The two whole sentences a multi-selection names its count with, one per count, each filled by Format.fill.
    readonly property var selectedWords: ({ one: "%1 item selected", many: "%1 items selected" })
    // A multi-selection describes a count, not a file, so it names the count instead of a name.
    function nameText() {
        if (root.previewState === Facts.MULTI)
            return Format.fill(root.selectionCount === 1 ? root.selectedWords.one : root.selectedWords.many, root.selectionCount)
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
        case Facts.FONT:
            return false
        case Facts.PDF:
            return !root.pdfDrawn
        case Facts.TEXT:
            return root.isDocumentRow ? (markdown ? markdown.blank : false) : lines.blank
        case Facts.CODE:
            return root.isDocumentRow ? (markdown ? markdown.blank : false) : lines.blank
        case Facts.ARCHIVE:
            return root.meta === null
        case Facts.MULTI:
            return root.multiMarks.length === 0
        }
        return true
    }

    // The cache file while there is one, then the image itself once the backend says none is coming.
    // A held frame never decodes the original: its cache entry still shows, nothing else does.
    function frameSource() {
        if (!root.visible) return ""
        if (root.thumb.length > 0)
            return Format.fileUri(root.thumb)
        if (!root.manualHold && root.noThumbComing && root.previewState === Facts.IMAGE && root.path.length > 0 && !root.originalRefused)
            return Format.fileUri(root.path)
        return ""
    }

    // One binding for the whole decode request: url and ceiling change in one pass, zero is Qt unbounded.
    readonly property var decodeTarget: ({
        url: root.frameSource(),
        w: root.thumb.length > 0 ? 0 : Math.max(1, Math.round(root.turned ? frameThumb.boxHeight : frameThumb.boxWidth)),
        h: root.thumb.length > 0 ? 0 : Math.max(1, Math.round(root.turned ? frameThumb.boxWidth : frameThumb.boxHeight))
    })
    onDecodeTargetChanged: root.applyDecodeTarget()

    // Blank first, then ceiling, then url, so the three never land in separate passes.
    // A request that changed nothing is skipped, so a relayout over a cache file loads nothing.
    function applyDecodeTarget() {
        var t = root.decodeTarget
        if (String(frameThumb.source) === String(t.url)
                && frameThumb.sourceSize.width === t.w && frameThumb.sourceSize.height === t.h)
            return
        frameThumb.source = ""
        frameThumb.sourceSize.width = t.w
        frameThumb.sourceSize.height = t.h
        frameThumb.source = t.url
    }

    // Why the frame is showing a mark instead of the thing it meant to draw. Empty for every state
    // whose mark is simply what it draws, so the line only ever appears when something went wrong.
    function frameNote() {
        if (root.isHtmlRow && root.markdown) {
            if (root.markdown.binary) return "binary file"
            if (root.markdown.tooLarge) return "too large to preview"
        }
        if (lines.note.length > 0) return lines.note
        if (root.manualHold) {
            return KeyName.hinted("%1 loads it", "loadPreview")
        }
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
