import QtQuick
import qs.Commons
import "." as Flea
import "js/Facts.js" as Facts
import "js/Icons.js" as Icons
import "js/Kinds.js" as Kinds
import "js/Search.js" as Search
import "js/Thumbs.js" as Thumbs
import "js/ExtThumbs.js" as ExtThumbs
import "js/MarkdownPrepared.js" as Prepared
import "js/Motion.js" as Motion
import "js/PreviewSettle.js" as PreviewSettle
import "js/PreviewSwap.js" as PreviewSwap
import "js/Swap.js" as Swap
import "js/QuickLookRect.js" as QuickLookRect

// The overlay lives inside the Flea window, Finder's Quick Look shape: a second window breaks omarchy-drive focus flea and every test that narrows on it.
Item {
    id: root
    anchors.fill: parent
    // active flips instantly, so the IPC read never races the close fade; visible outlives it until surface's own opacity finishes.
    visible: root.active || surface.opacity > 0
    z: 1

    property bool active: false
    // shell.qml wires the pane in: the archive pane asks the backend about the cursor row and nothing else here reads it.
    property var pane: null
    property string path: ""
    property string iconName: ""
    property string kindName: ""
    property int size: 0
    property string kind: ""
    readonly property bool isMedia: root.kind === "audio" || root.kind === "video"
    readonly property bool isPdf: root.kind === "pdf"
    readonly property bool isImage: root.kind === "image"
    readonly property bool isArchive: root.kind === "archive"
    readonly property bool isFont: root.kind === "font"
    readonly property bool fontFailed: root.isFont && fontLoader.item !== null && fontLoader.item.failed
    // RenderedPreviews: a Markdown file keeps the text kind and draws its own bar and pane.
    readonly property bool isMarkdown: root.kind === "text" && Kinds.isMarkdown(root.path)
    readonly property bool isHtml: root.kind === "text" && Kinds.isHtml(root.path)
    readonly property bool isDocument: root.isMarkdown || root.isHtml
    // Code and plain text share ui/PreviewText.qml; every text kind draws the Quick Look bar.
    readonly property bool isCodeText: root.kind === "text" && !root.isDocument
    readonly property bool hasBar: root.kind === "text"
    // Compare shows two files in this overlay: path is the first and comparePath the second, and one diff is asked per open.
    readonly property bool isCompare: root.kind === "compare"
    property string comparePath: ""
    // r flips the open document Quick Look to Source; close() forgets it, and a move to another file keeps it.
    property bool markdownSource: false
    // The file whose first Markdown read may block the key, decided from its listing row before the path moves; "" when none may.
    property string inlineMarkdownPath: ""
    // Tab put the keyboard on the bar's close mark, Markdown or code; a move off the text kinds or a close lets it go.
    property bool markdownCloseFocus: false
    readonly property bool markdownCloseFocused: root.active && root.hasBar && root.markdownCloseFocus
    onHasBarChanged: if (!root.hasBar) root.markdownCloseFocus = false
    // The backend's meta answer for the open archive, null until it lands; archiveRow is the row it was asked for.
    property var archiveMeta: null
    property int archiveRow: -1
    // MediaPdf rule 6's fourth fact: Qt carries no sample-rate key at all (QMediaMetaData::Key, Qt 6.11), so the number is the backend probe's, asked the way an archive's is.
    property int mediaRate: 0
    property int mediaRow: -1
    readonly property bool archiveFailed: root.isArchive && root.archiveMeta !== null && root.archiveMeta.archiveFailed === true
    // For ui/Ipc.qml: the item drawing this kind's content, whether a player exists, and what the text and archive panes hold.
    function surfaceItem() {
        if (root.isImage) return imageLoader.item
        if (root.isMedia) return mediaLoader.item
        if (root.isPdf) return pdfLoader.item
        if (root.isArchive) return archivePane
        if (root.isCompare) return compareLoader.item
        if (root.kind === "text") return root.isDocument
            ? (markdownLoader.item ? markdownLoader.item.bodyItem : null) : textPane.bodyItem
        return null
    }
    function mediaLoaded() { return mediaLoader.item !== null }
    function textShown() { return root.isDocument
        ? (markdownLoader.item ? markdownLoader.item.rawText : "") : textPane.shownText() }
    function markdownView() { return root.markdownShown ? root.markdownShown.shownView : "" }
    function markdownCloseState() { return JSON.stringify(root.markdownShown ? root.markdownShown.closeState() : {}) }
    function markdownEndGap() { return root.markdownShown ? root.markdownShown.endGap() : -1 }
    function markdownScrollY() { return root.markdownShown ? Math.round(root.markdownShown.scrollY) : -1 }
    function archiveNames() { return root.archiveMeta && root.archiveMeta.names ? root.archiveMeta.names.map(function (e) { return e.n }).join("|") : "" }
    readonly property bool pdfExpanded: root.isPdf && pdfLoader.item !== null && pdfLoader.item.expanded
    // The PDF surface, null with no document loaded: ui/Ipc.qml answers "" for that, so an unmeasured state never reads as a value.
    readonly property var pdfItem: pdfLoader.item
    // What the strip actually draws: shell.qml's IPC reads this rather than re-deriving the visible: expression.
    readonly property alias stripVisible: mediaStrip.visible
    // The strip's own mute mark and the flag it draws from, so a test reads and clicks what is there.
    readonly property var muteMark: mediaStrip.muteItem
    readonly property bool muted: Flea.MediaSound.muted
    // fleaWindow.itemRect needs the real Item, the same seam rowCentre already reads through pane.
    readonly property var seekSlider: mediaStrip.seekItem
    readonly property var volumeSlider: mediaStrip.volumeItem
    readonly property string status: {
        if (!root.active) return ""
        // A fresh media item still carries its empty path until onLoaded binds it; its
        // "stopped" there is not a whole preview, so the swap never releases on it.
        if (root.isMedia) return (mediaLoader.item && mediaLoader.item.path === root.path) ? mediaLoader.item.status : "loading"
        if (root.isPdf) return (pdfLoader.item && pdfLoader.item.failed) ? "This file could not be read." : "pdf"
        if (root.isImage) return imageLoader.item ? imageLoader.item.status : "loading"
        if (root.isArchive) return root.archiveMeta === null ? "loading" : (root.archiveFailed ? "This archive could not be read." : "archive")
        if (root.isCompare) return compareLoader.item ? compareLoader.item.status : "loading"
        if (root.kind === "text") return root.isDocument
            ? (markdownLoader.item ? markdownLoader.item.status : "loading") : textPane.status
        if (root.isFont) return fontLoader.item ? fontLoader.item.status : "loading"
        return "This file cannot be previewed."
    }
    // The swap's answer: nothing loading, and a PDF with a page on screen or refused.
    // An interim cache thumbnail shown counts as whole, never as a half-built frame.
    readonly property bool lookReady: !root.active || PreviewSwap.lookReady(root.status, root.isPdf,
        root.pdfItem !== null && root.pdfItem.shownPage >= 0 && root.pdfItem.opened === root.path, root.pdfItem !== null && root.pdfItem.failed,
        root.interimShown, (root.isDocument && markdownLoader.item !== null && markdownLoader.item.firstScreen)
        || (root.isCodeText && textPane.firstScreen)) && !root.sizeWait
    // The cached thumbnail held at open, drawn under the full decode at the final rect; the stamp retires a stale one.
    property string interimThumb: ""
    property string interimStamp: ""
    // The interim's rect off the upright original, null while its pixels are unknown: no
    // interim until then. A cache file is already upright, so no box swap applies here.
    readonly property var interimBox: PreviewSwap.interimRect(panes.width, panes.height - (root.isRawFile ? Theme.chromeHeight : 0),
        root.imageW, root.imageH, root.imageOrient)
    // True once the interim is on screen at the final's rect; lookReady releases on this.
    readonly property bool interimVisible: root.isImage && root.interimThumb.length > 0
        && root.path === root.interimStamp && root.interimBox !== null
    readonly property bool interimShown: root.interimVisible && imageLoader.item !== null
        && imageLoader.item.interimReady === true
    // The original's pixels, off the shown row's meta reply; 0 until it lands.
    property int imageW: 0
    property int imageH: 0
    property int imageOrient: 1
    property int imageFrames: 0
    property int imageRow: -1
    // One meta ask per show; onRowsChanged never re-asks behind it.
    property bool imageAsked: false
    // The row whose thumbnail may stand in for an image that cannot decode and had none at open, -1 when none is owed; the sentence waits while it is set.
    property int thumbRow: -1
    // One thumbnail ask per show, sent only once the final decode has failed.
    property bool thumbAsked: false
    // A listing started while the thumbnail is owed, so its map and row indices are the old listing's.
    property bool thumbRelist: false
    readonly property bool imageUndecodable: root.isImage && imageLoader.item !== null && imageLoader.item.pictureFailed
    onImageUndecodableChanged: root.settleThumb()
    // A camera RAW is drawn through the copy of its embedded JPEG the meta reply names; null until it lands or when the RAW holds none.
    readonly property bool isRawFile: root.isImage && Kinds.isRaw(root.path)
    property var rawCopy: null
    property bool rawAnswered: false
    // See ui/js/QuickLookRect.js: shapeNext follows the file shown and its size reply, shape is what the card draws, taken once no swap holds a picture.
    property var shapeNext: ({ kind: "", known: false, w: 0, h: 0, orient: 1 })
    property var shape: root.shapeNext
    readonly property var rectTokens: ({ fraction: Theme.preview.fraction, edge: Theme.preview.edge, bar: Theme.chromeHeight })
    readonly property var rect: QuickLookRect.surface(root.shape, root.width, root.height, root.pdfExpanded, root.rectTokens)
    readonly property var nextRect: QuickLookRect.surface(root.shapeNext, root.width, root.height, root.pdfExpanded, root.rectTokens)
    readonly property bool held: root.swap !== null && (root.swap.holding || root.swap.capturing)
    // Plain, as a binding on its own releases loops: set by show(), cleared by the reply, an early reveal or the swap's release a turn later.
    property bool sizeWait: false
    onHeldChanged: {
        root.commitShape()
        if (!root.held) Qt.callLater(function () { root.sizeWait = false })
    }
    function commitShape() { if (!root.held) root.shape = root.shapeNext }
    // A size reply counts until the card shows or the swap lets go of its picture; a later one leaves the card alone.
    function sizeArrived(w, h, orient, known) {
        if (!root.revealed || root.held) {
            root.shapeNext = { kind: root.kind, known: known, w: w, h: h, orient: orient }
            root.commitShape()
        }
        root.sizeWait = false
    }
    function swapState() {
        if (root.swap) return root.swap.describe()
        return { holding: false, capturing: false, fellBack: false, holds: 0, fallbacks: 0,
                 bursts: 0, heldFrames: 0, midFrames: 0, loadingFrames: 0, last: { ms: 0, end: "" } }
    }
    // The 0.3.6 swap additions, null until the first open builds them below; every reader guards it.
    readonly property var swap: swapLoader.item
    readonly property bool swapBuilt: swapLoader.active
    // The memory suite asserts both Markdown loader items are null without a Markdown file.
    readonly property var markdownItem: markdownLoader.item
    // The open Markdown document, null unless Quick Look shows one; the guarded readers share this null.
    readonly property var markdownShown: root.active && root.isDocument ? markdownLoader.item : null
    // Exposes the eager panes for the live swap gate, so it can pin the wiring.
    readonly property var panesItem: panes
    // Synchronous: a local source: URL answers item on the same call that sets active.
    function ensureSwap() {
        if (!swapLoader.active)
            swapLoader.active = true
        return swapLoader.item
    }

    property string pendingPath: ""
    property string pendingIcon: ""
    property string pendingKind: ""
    property int pendingSize: 0
    property string pendingThumb: ""
    // The row open/follow captured, carried through load/show like the thumb.
    property int pendingImageRow: -1
    // Last distinct follow target, so a held key cannot reload mid-burst: only idleness loads at once.
    property double lastMoveAt: 0
    property string lastMoveKey: ""
    // The settle idiom Pane's own thumbnail request reuses: a held j/k costs zero reloads until the cursor rests.
    readonly property int followSettleMs: 120
    // The same dim ui/SettingsPanel.qml lays over the listing.
    readonly property real groundOpacity: 0.5
    // The card and dim open once the content can draw, or early over the loading state, so no frame shows an empty card.
    readonly property bool revealed: root.landed || root.revealedEarly
    // Set by the cap, or by a press or the wheel during the hold.
    property bool revealedEarly: false
    // When the early loading state first drew, 0 while none stands; content replaces it only after Swap.MIN_SHOW_MS.
    property double loadingSince: 0
    // True once this open's content first drew; later moves are the swap's to count.
    property bool landed: false
    readonly property bool revealedOpen: root.active && root.revealed
    // The hold: the card and dim draw nothing, a reopen inside the close fade included, and a move takes no picture.
    readonly property bool unrevealed: root.active && !root.revealed
    // An early reveal shows the loading state at once, for every kind, until this open's content lands.
    readonly property bool loadingEarly: root.revealedEarly && !root.landed
    // Frames drawn with the card showing during the hold; tests/ui.sh lookopen and tests/quicklook-hold.sh read them.
    property int lookEmptyFrames: 0
    property int revealCaps: 0
    // The cap's start stamp and the sequence number of show()'s return, ordered by openSeq so tests/quicklook-firstkind.qml reads arm-before-show or after with no clock.
    property var revealCapStart: null
    property int openSeq: 0
    property int shownSeq: 0
    // The kinds whose Loader item this window has built once, so a repeat of one is a warm open.
    property var builtKinds: ({})
    function openState() {
        return { revealed: root.revealed, revealedEarly: root.revealedEarly, landed: root.landed, lookEmptyFrames: root.lookEmptyFrames, revealCaps: root.revealCaps, revealCapStart: root.revealCapStart, shownSeq: root.shownSeq }
    }
    // A move still settling lands nothing, and followSettle reruns this once it stops, so what reveals is the cursor's own file.
    // Past the cap, content that is ready waits out the loading state's minimum, so it never blinks for under 100 ms.
    function revealIfReady() {
        if (!root.active || !root.lookReady || followSettle.running)
            return
        var left = root.loadingEarly ? Swap.showLeft(root.loadingSince, Date.now()) : 0
        if (left > 0) {
            minShow.interval = left
            minShow.restart()
        } else {
            root.landed = true
        }
    }
    function showEarly() {
        root.loadingSince = Date.now()
        root.revealedEarly = true
        root.sizeWait = false
    }
    function revealNow() {
        root.revealIfReady()
        if (root.unrevealed)
            root.showEarly()
    }
    // The ready test runs after the open's own turn, because lookReady still reads the closed overlay inside this handler.
    onActiveChanged: {
        root.revealedEarly = false
        root.loadingSince = 0
        root.landed = false
        minShow.stop()
        if (!root.active)
            revealCap.stop()
        else
            Qt.callLater(root.revealIfReady)
    }
    onLookReadyChanged: root.revealIfReady()
    // Armed by load(): before show() for a warm kind, after it for a kind's first build, beside the swap's own cap.
    Timer {
        id: revealCap
        objectName: "revealCap"
        repeat: false
        onTriggered: {
            if (!root.unrevealed)
                return
            root.revealCaps += 1
            root.showEarly()
        }
    }
    // The early loading state has stood its minimum, so ready content lands now.
    Timer {
        id: minShow
        repeat: false
        onTriggered: root.revealIfReady()
    }
    // afterAnimating runs just before each frame is synchronised, so it sees a frame that draws the card or dim with neither content nor the loading state.
    Connections {
        target: root.active && !root.landed && !root.loadingEarly && (surface.opacity > 0 || lookDim.opacity > 0) ? root.Window.window : null
        function onAfterAnimating() { root.lookEmptyFrames += 1 }
    }
    // One fade for the card and dim, so the hold can cut a close fade still running.
    property real fade: root.revealedOpen ? 1 : 0
    Behavior on fade {
        enabled: !Theme.reducedMotion
        NumberAnimation { duration: root.active ? Motion.durMs.open : Motion.durMs.close; easing.type: Easing.OutCubic }
    }

    // Read through to PreviewMedia so this file never imports QtMultimedia, and 0 before the loader has an item.
    readonly property int position: (root.isMedia && mediaLoader.item) ? mediaLoader.item.position : 0
    readonly property int duration: (root.isMedia && mediaLoader.item) ? mediaLoader.item.duration : 0

    // Shown on open, hidden stripHideMs after the last reveal, video only: audio has nothing else to look at.
    property bool stripShown: true
    // StatusBar.messageMs, the OEM's own transient interval, which Sidebar's unmount arm reuses for the same reason.
    property int stripHideMs: 4000

    function revealStrip() {
        root.stripShown = true
        stripHideTimer.restart()
    }

    function togglePlay() { if (root.isMedia && mediaLoader.item) mediaLoader.item.togglePlay() }

    // MediaMute rule 3: one session flag, so the column's strip and this one always agree.
    function toggleMute() { if (root.isMedia) Flea.MediaSound.toggle() }

    // Absolute seek in ms, clamped by PreviewMedia's own seekTo; the slider's onReleased calls this directly.
    function seekTo(ms) { if (root.isMedia && mediaLoader.item) mediaLoader.item.seekTo(ms) }

    // Relative seek in ms, Left/Right's own shape; seekTo does the clamping.
    function seek(deltaMs) {
        root.seekTo(root.position + deltaMs)
    }

    // The PDF viewer's three actions come through this file, so ui/js/Focus.js never learns a Loader item answers them.
    function turnPage(delta) { if (root.isPdf && pdfLoader.item) pdfLoader.item.turn(delta) }

    function zoomBy(steps) { if (root.isPdf && pdfLoader.item) pdfLoader.item.zoomBy(steps); else if (root.isImage && imageLoader.item) imageLoader.item.zoomBy(steps); else if (root.isHtml && root.markdownShown) root.markdownShown.zoomBy(steps) }
    function zoomFit() { if (root.isImage && imageLoader.item) imageLoader.item.zoomFit(); else if (root.isHtml && root.markdownShown) root.markdownShown.zoomFit() }

    function toggleExpand() { if (root.isPdf && pdfLoader.item) pdfLoader.item.toggleExpand() }

    // Space opens on the cursor row; this is immediate, follow() below is the held-key j/k path.
    function open(newPath, newIcon, newSize, newKind, newThumb) {
        followSettle.stop()
        root.lastMoveKey = newPath + "\n" + newIcon + "\n" + newSize + "\n" + newKind
        root.lastMoveAt = Date.now()
        root.load(newPath, newIcon, newSize, newKind, newThumb, root.pane ? root.pane.cursorIndex : -1)
    }

    function scrollDocument(direction, isPage) {
        var document = root.isMarkdown ? markdownLoader.item : root.kind === "text" ? textPane : null
        if (document) document.scrollBy(direction * (isPage ? document.viewportHeight / 2 : Theme.font.body))
    }

    // The Compare overlay for two regular files; iconName is the first file's, which picks the bar's mark.
    function openCompare(pathA, pathB, newIcon) {
        followSettle.stop()
        root.comparePath = pathB
        root.iconName = newIcon || ""
        root.path = pathA
        root.size = 0
        root.kindName = ""
        root.kind = "compare"
        root.active = true
    }

    // y: the patch goes to the clipboard; any other kind has none.
    function copyPatch() { if (root.isCompare && compareLoader.item) compareLoader.item.copyPatch() }

    // Document Quick Looks open rendered; r flips only the open one.
    function toggleMarkdownView() {
        if (root.isDocument) root.markdownSource = !root.markdownSource
    }

    function toggleMarkdownClose() {
        if (root.hasBar) root.markdownCloseFocus = !root.markdownCloseFocus
    }

    // w wraps the open code or text Quick Look, and a Markdown Source; ui.json keeps the choice for the next one.
    function toggleWrap() {
        if (root.isCodeText || (root.isDocument && root.markdownShown && root.markdownShown.shownView === "source")) ViewState.changeLeaf("preview", { wrap: !ViewState.previewWrap })
    }

    // The picture is taken now, so the settled load below changes the panes under it.
    function follow(newPath, newIcon, newSize, newKind, newThumb) {
        var key = newPath + "\n" + newIcon + "\n" + newSize + "\n" + newKind
        // Pending covers a repeat; a settled revisit of the shown target stays put, anything else reloads.
        var now = Date.now()
        var decision = PreviewSettle.plan(now, root.lastMoveAt, root.followSettleMs, key, root.lastMoveKey, followSettle.running)
        if (decision === "same") return
        if (key === root.lastMoveKey && root.isShown(newPath, newIcon, newSize, newKind)) return
        root.lastMoveKey = key
        root.lastMoveAt = now
        root.pendingPath = newPath
        root.pendingIcon = newIcon
        root.pendingSize = newSize
        root.pendingKind = newKind
        root.pendingThumb = newThumb || ""
        root.pendingImageRow = root.pane ? root.pane.cursorIndex : -1
        // During the hold a move takes no picture, and a burst still trails so it asks nothing per row.
        if (root.revealedOpen) {
            var held = root.ensureSwap()
            if (held)
                held.hold(null, newPath)
        }
        if (decision === "now") {
            followSettle.stop()
            root.load(newPath, newIcon, newSize, newKind, newThumb, root.pendingImageRow)
        } else {
            followSettle.restart()
        }
    }

    // Whether the overlay already shows this target: the duplicate test the key clock cannot make.
    function isShown(newPath, newIcon, newSize, newKind) {
        return root.active && newPath === root.path && newIcon === root.iconName
            && newSize === root.size && newKind === root.kindName
    }

    // Dropping the loader's source is what stops playback: media dies with the loader.
    function close() {
        if (root.swap) root.swap.cancel()
        root.revealCapStart = null
        followSettle.stop()
        root.lastMoveKey = ""
        root.pendingThumb = ""
        root.pendingImageRow = -1
        root.interimThumb = ""
        root.interimStamp = ""
        root.imageW = 0
        root.imageH = 0
        root.imageOrient = 1
        root.imageFrames = 0
        root.imageRow = -1
        root.imageAsked = false
        root.thumbRow = -1
        root.thumbAsked = false
        root.thumbRelist = false
        root.rawCopy = null
        root.rawAnswered = false
        stripHideTimer.stop()
        root.active = false
        root.kind = ""
        root.markdownSource = false
        root.markdownCloseFocus = false
        root.inlineMarkdownPath = ""
        root.comparePath = ""
        mediaLoader.source = ""
        pdfLoader.source = ""
        imageLoader.source = ""
        root.archiveMeta = null
        root.archiveRow = -1
        root.mediaRate = 0
        root.mediaRow = -1
        if (root.pane) root.pane.listArea.forceActiveFocus()
    }

    // Under the held picture when a move took one; Space's own open has none and reveals once its content can draw.
    // The swap starts after its mutation: show runs at once or waits for the capture, start runs with it.
    function load(newPath, newIcon, newSize, newKind, newThumb, newRow) {
        var isPdf = Kinds.quickLookKind(newIcon, newPath) === Kinds.PDF
        var swapItem = root.ensureSwap()
        var show = function () {
            // A warm kind counts from the key, before show() builds the Loader items; a kind's first build (a PDF's compile above all) is outside the cap, which starts once show() returns; a move during the hold keeps the open's deadline.
            var kindKey = root.kindFor(newPath, newIcon, newThumb, newRow)
            var arm = (!root.active || !root.revealed) && !revealCap.running
            var warm = root.builtKinds[kindKey] === true
            if (arm && warm) root.armRevealCap(isPdf)
            root.show(newPath, newIcon, newSize, newKind, newThumb, newRow)
            if (swapItem) swapItem.start(isPdf)
            root.shownSeq = ++root.openSeq
            root.builtKinds[kindKey] = true
            if (arm && !warm) root.armRevealCap(isPdf)
            else if (!arm && !revealCap.running) root.revealCapStart = null
        }
        if (swapItem && (swapItem.holding || swapItem.capturing))
            swapItem.hold(show, newPath, true)
        else
            show()
    }

    // The row at index while its name under the listing's base is path, else null: how this file identifies a row.
    function namedRow(index, path) {
        if (!root.pane || index < 0) return null
        var row = root.pane.rowFor(index)
        return row !== null && root.pane.join(Search.baseOf(root.pane), row.n) === path ? row : null
    }

    function kindFor(newPath, newIcon, newThumb, newRow) {
        var row = root.namedRow(newRow, newPath)
        return Kinds.quickLookKind(newIcon, newPath, (row !== null && row.t === true) || (newThumb || "").length > 0)
    }

    function armRevealCap(isPdf) {
        revealCap.interval = PreviewSwap.capMs(isPdf)
        revealCap.start()
        root.revealCapStart = { seq: ++root.openSeq }
    }

    function show(newPath, newIcon, newSize, newKind, newThumb, newRow) {
        var row = root.namedRow(newRow, newPath)
        root.inlineMarkdownPath = row !== null && Prepared.readsInline(row, root.pane.storageClass, root.pane.storageKnown) ? newPath : ""
        var kind = root.kindFor(newPath, newIcon, newThumb, newRow)
        // Gate the live image reader before its path moves, so a follow cannot decode at the previous card size while its dimensions are owed.
        root.sizeWait = kind === "image" && root.pane !== null && newRow >= 0
        root.kind = kind
        // A pane of another kind goes before the path moves, or it tries to open a file it cannot draw: an image
        // pane handed a video logged "Unsupported image format" on every move from a picture to a clip.
        if (!root.isMedia) mediaLoader.source = ""
        if (!root.isPdf) pdfLoader.source = ""
        if (!root.isImage) imageLoader.source = ""
        // The interim holds the path it was read for; a stale one never outlives a move.
        root.interimThumb = newThumb || ""
        root.interimStamp = newPath
        // The meta row is the one captured at open/follow, never the cursor now.
        root.imageRow = root.isImage && newRow !== undefined ? newRow : -1
        root.imageW = 0
        root.imageH = 0
        root.imageOrient = 1
        root.imageFrames = 0
        root.imageAsked = false
        root.thumbAsked = false
        root.thumbRelist = false
        root.rawCopy = null
        root.rawAnswered = false
        root.shapeNext = { kind: root.kind, known: false, w: 0, h: 0, orient: 1 }
        root.commitShape()
        root.path = newPath
        root.thumbRow = root.thumbOwed(newRow)
        root.iconName = newIcon
        root.size = newSize
        // MediaPdf rule 6: the overlay is the bigger surface, so it says at least what the column says, and the kind is the caller's because the backend named it for that row.
        root.kindName = newKind || ""
        root.active = true
        mediaLoader.source = root.isMedia ? "PreviewMedia.qml" : ""
        pdfLoader.source = root.isPdf ? "PdfViewer.qml" : ""
        imageLoader.source = root.isImage ? "PreviewImage.qml" : ""
        root.askArchive()
        root.askMedia()
        root.askImage()
        root.sizeWait = root.isImage && root.imageAsked && root.imageRow >= 0
        root.revealStrip()
        root.maybePrefetch()
    }

    // Bounded prefetch: the next row's cache entry only, once per rest, never a decode and never a meta.
    function maybePrefetch() {
        if (!root.active || !root.pane || followSettle.running) return
        if (!ViewState.previewAutomatic) return
        var next = root.pane.cursorIndex + 1
        if (root.pane.thumbState.file[next] !== undefined) return
        var row = root.pane.rowFor(next)
        if (!row || row.d || row.t !== true || !root.thumbClear(row)) return
        var work = { ask: [next], drop: [], cacheOnly: true }
        root.pane.thumbState = Thumbs.applied(root.pane.thumbState, work)
        root.pane.backend.thumb(work.ask, true)
    }

    // Whether a thumbnail ask may go out for this row here: the listing settled, the class known, no manual hold, and the row allowed.
    function thumbClear(row) {
        if (!root.pane || root.pane.listInFlight || !root.pane.storageKnown) return false
        if (ExtThumbs.manualHold(root.pane.storageClass, ViewState.preview)) return false
        return Thumbs.allowed(row, ViewState.thumbnailMode)
    }

    // One row, only while an archive is the thing open: the same no-sweep rule the column follows.
    function askArchive() {
        root.archiveMeta = null
        root.archiveRow = root.isArchive && root.pane ? root.pane.cursorIndex : -1
        if (root.archiveRow >= 0)
            root.pane.backend.askMeta(root.archiveRow, false, false, true)
    }

    // The same one row, for the one fact the transport under the overlay cannot report.
    function askMedia() {
        root.mediaRate = 0
        root.mediaRow = root.isMedia && root.pane ? root.pane.cursorIndex : -1
        if (root.mediaRow >= 0)
            root.pane.backend.askMeta(root.mediaRow, false, true, false)
    }

    // The original's pixels, which size the interim and the card; once per show, never while a held key is still bursting.
    function askImage() {
        if (!root.isImage || !root.pane) return
        if (root.imageRow < 0 || root.imageAsked || followSettle.running) return
        // An insert or a re-sort between capture and ask moves another file to the index,
        // so the row must still name the shown file before anything is asked for it.
        root.resolveImageRow()
        root.imageAsked = true
        if (root.imageRow < 0) return
        root.pane.backend.askMeta(root.imageRow, false, false, false)
    }

    function imageRowShown() {
        return root.namedRow(root.imageRow, root.path) !== null
    }

    // The captured index drifted onto another file: take the cursor when it names the
    // shown file instead, else drop the interim for this show with no ask.
    function resolveImageRow() {
        if (root.imageRowShown()) return
        var at = root.pane ? root.pane.cursorIndex : -1
        root.imageRow = root.namedRow(at, root.path) !== null ? at : -1
    }

    // A re-list clears the owed row's entry: take the cursor when it names the shown file, else drop the owed thumbnail with no ask.
    function resolveThumbRow() {
        if (root.namedRow(root.thumbRow, root.path) !== null) return
        var at = root.pane ? root.pane.cursorIndex : -1
        root.thumbRow = root.namedRow(at, root.path) !== null ? at : -1
    }

    // The row whose thumbnail may stand in if this image cannot decode: only with none held, clear to ask, and not refused.
    function thumbOwed(newRow) {
        if (!root.isImage || root.interimThumb.length > 0) return -1
        var at = newRow !== undefined ? newRow : -1
        if (!root.thumbClear(root.namedRow(at, root.path)) || Thumbs.refused(root.pane.thumbState, at)) return -1
        return at
    }

    // After a failed final decode: ask the one row's thumbnail, then bind its reply to the shown file; a refusal or a forgotten ask ends on the sentence.
    function settleThumb() {
        if (root.thumbRow < 0 || !root.imageUndecodable || root.pane.listInFlight) return
        // A re-list cleared the map under the owed row: find the shown file again and ask once on the new listing, or end on the sentence when it is gone.
        if (root.thumbRelist) {
            root.thumbRelist = false
            root.resolveThumbRow()
            root.thumbAsked = false
            if (root.thumbRow < 0) return
        }
        // An insert or a re-sort since the capture moves another file to the index, so the row must still name the shown file.
        var at = root.namedRow(root.thumbRow, root.path) !== null ? root.thumbRow : -1
        var state = root.pane.thumbState
        var file = at >= 0 ? Thumbs.fileFor(state, at) : ""
        // Nothing more is owed once a name landed, the row was refused, or our own ask was forgotten.
        if (at < 0 || file.length > 0 || Thumbs.refused(state, at) || (root.thumbAsked && state.file[at] === undefined)) {
            if (file.length > 0) root.interimThumb = file
            root.thumbRow = -1
            return
        }
        if (root.thumbAsked || Thumbs.pending(state.file[at])) return
        root.thumbAsked = true
        var work = { ask: [at], drop: [], cacheOnly: false }
        root.pane.thumbState = Thumbs.applied(state, work)
        root.pane.backend.thumb(work.ask, false)
    }

    Connections {
        target: root.pane ? root.pane.backend : null
        function onMeta(row, w, h, orient, durationMs, sampleRate, entries, unpacked, archiveFailed, names, lines, partial, linesFailed, target, targetDir, owner) {
            if (root.isArchive && row === root.archiveRow)
                root.archiveMeta = { entries: entries, unpacked: unpacked, archiveFailed: archiveFailed, names: names }
            if (root.isMedia && row === root.mediaRow)
                root.mediaRate = sampleRate
            if (root.isImage && !root.isRawFile && row === root.imageRow && root.imageRowShown()) {
                root.imageW = w
                root.imageH = h
                root.imageOrient = orient
                root.sizeArrived(w, h, orient, true)
            }
        }
        // The RAW's own pixels are not what is drawn, so its size and turn come from the copy and not from the file's header.
        function onMetaResult(message) {
            if (!root.isRawFile || message.row !== root.imageRow || !root.imageRowShown()) return
            root.rawCopy = message.raw || null
            root.rawAnswered = true
            if (root.rawCopy === null) {
                root.sizeArrived(0, 0, 1, false)
                return
            }
            root.imageW = root.rawCopy.w
            root.imageH = root.rawCopy.h
            root.imageOrient = root.rawCopy.orient
            root.sizeArrived(root.rawCopy.w, root.rawCopy.h, root.rawCopy.orient, true)
        }
    }

    Connections {
        target: root.pane ? root.pane.backend : null
        function onMetaResult(message) {
            if (root.isImage && message.row === root.imageRow && root.imageRowShown()) root.imageFrames = message.frames || 0
        }
    }

    // A meta asked across a listing change is answered with silence, so the rows landing re-asks it, the way ui/ColumnsArea.qml does.
    Connections {
        target: root.pane
        function onRowsChanged() {
            if (root.active && root.isArchive && root.archiveMeta === null) root.askArchive()
            if (root.active && root.isMedia && root.mediaRate === 0) root.askMedia()
            if (root.active && root.isImage && root.imageW === 0 && !root.rawAnswered) root.askImage()
        }
        // The cursor and the rows are final once the listing lands, so an owed row is found again there.
        function onListInFlightChanged() {
            if (!root.pane) return
            if (root.pane.listInFlight) root.thumbRelist = root.thumbRelist || root.thumbRow >= 0
            else root.settleThumb()
        }
        function onThumbStateChanged() { root.settleThumb() }
    }

    Timer {
        id: followSettle
        interval: root.followSettleMs
        repeat: false
        onTriggered: root.load(root.pendingPath, root.pendingIcon, root.pendingSize, root.pendingKind, root.pendingThumb, root.pendingImageRow)
        onRunningChanged: if (!followSettle.running) Qt.callLater(root.revealIfReady)
    }

    Timer {
        id: stripHideTimer
        interval: root.stripHideMs
        repeat: false
        // A held seek or volume press is contact too, so the strip waits for the release before it goes.
        onTriggered: {
            if (mediaStrip.scrubbing || (mediaStrip.volumeItem && mediaStrip.volumeItem.pressing)) stripHideTimer.restart()
            else root.stripShown = false
        }
    }

    MouseArea {
        anchors.fill: parent
        // A shield that outlives the overlay swallows the first click after it and freezes row hover for the window.
        enabled: root.active
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        hoverEnabled: true
        // A click behind the overlay would silently move the cursor or open the listing's menu on it.
        // A left click outside the surface closes, a rule of 2026-09-11, and a right click is only swallowed.
        onClicked: function (mouse) {
            if (mouse.button === Qt.LeftButton && !surface.contains(surface.mapFromItem(root, mouse.x, mouse.y)))
                root.close()
        }
        onPositionChanged: root.revealStrip()
        // The ground takes the wheel, so a scroll over the overlay never reaches the listing behind it.
        onWheel: function (wheel) { wheel.accepted = true }
    }

    // PdfViewer.html and MediaPlayer.html draw a pane with its own edge: on the surface colour alone the inset vanished into the listing behind it.
    Rectangle {
        id: lookDim
        anchors.fill: parent
        color: Theme.color.background
        opacity: root.unrevealed ? 0 : root.fade * root.groundOpacity
    }

    Rectangle {
        id: surface
        x: Theme.cardOrigin(parent.width, width)
        y: Theme.cardOrigin(parent.height, height) + surface.rise
        border.width: Theme.spacing.hairline
        border.color: Theme.color.mutedFrame
        // Open rises into place and close only fades, faster, because the translation is enabled: root.active.
        property real rise: root.revealedOpen ? 0 : Motion.translateUpPx
        opacity: root.unrevealed ? 0 : root.fade
        // Tall kinds take the window's height less the edge, and Expand fills the window, which is the whole of the canvas's "expand fills the window".
        width: Theme.cardSpan(root.rect.w, parent.width)
        height: Theme.cardSpan(root.rect.h, parent.height)
        color: Theme.color.surface
        // Mirrors hyprland decoration:rounding; media fills the surface and keeps square corners, a visible corner only shows on text and audio panes.
        radius: Style.cornerRadius

        Behavior on rise {
            enabled: root.active && !Theme.reducedMotion
            NumberAnimation { duration: Motion.durMs.open; easing.type: Easing.OutCubic }
        }

        // Every kind's pane, eager the way 0.3.5 drew them: the first Space decodes and never
        // builds. Moving to the next file holds this one's picture through the swap below until that one is whole.
        Item {
            id: panes
            // The size the next card has, so a preview built under a held picture is whole at the size it lands on.
            width: Theme.cardSpan(root.nextRect.w, root.width)
            height: Theme.cardSpan(root.nextRect.h, root.height)

            Flea.PreviewText {
                id: textPane
                anchors.fill: parent
                anchors.margins: Theme.spacing.hairline
                active: root.isCodeText
                path: root.path
                size: root.size
                backend: root.pane ? root.pane.backend : null
                markName: Icons.glyphFor(root.iconName)
                // The column's rule: code gets the gutter, plain text none.
                numbered: Kinds.kindState(root.iconName) === Kinds.CODE || Kinds.isCode(root.path)
                wrap: ViewState.previewWrap
                closeFocused: root.markdownCloseFocused
                onCloseRequested: root.close()
                // ExtThumbs: Quick Look reads at most the first 256 KiB on network and phone storage.
                maxBytes: ExtThumbs.textLimit(root.pane ? root.pane.storageClass : "")
                truncate: root.pane ? (root.pane.storageClass === "network" || root.pane.storageClass === "phone") : false
            }

            // A file-path Loader, never an inline Component, so a window that never shows Markdown never compiles it.
            Loader {
                id: markdownLoader
                anchors.fill: parent
                // The pane lies inside the surface's own frame, as the column's does; media fills it on purpose.
                anchors.margins: Theme.spacing.hairline
                active: root.isDocument
                source: root.isHtml ? "HtmlPane.qml" : "MarkdownPane.qml"
                onLoaded: {
                    item.blockPath = Qt.binding(function () { return root.inlineMarkdownPath })
                    item.path = Qt.binding(function () { return root.path })
                    item.size = Qt.binding(function () { return root.size })
                    item.view = Qt.binding(function () { return root.markdownSource ? "source" : "rendered" })
                    item.maxBytes = Qt.binding(function () {
                        return ExtThumbs.textLimit(root.pane ? root.pane.storageClass : "")
                    })
                    item.truncate = Qt.binding(function () {
                        return root.pane ? (root.pane.storageClass === "network"
                            || root.pane.storageClass === "phone") : false
                    })
                    item.active = Qt.binding(function () { return root.isDocument })
                    item.closeFocused = Qt.binding(function () { return root.markdownCloseFocused })
                    item.backend = Qt.binding(function () { return root.pane ? root.pane.backend : null })
                    item.wrap = Qt.binding(function () { return ViewState.previewWrap })
                    item.closeRequested.connect(root.close)
                }
            }

            Loader {
                id: fontLoader
                anchors.fill: parent
                anchors.margins: Theme.spacing.hairline
                active: root.isFont
                source: "FontPane.qml"
                onLoaded: {
                    item.path = Qt.binding(function () { return root.path })
                    item.size = Qt.binding(function () { return root.size })
                    item.backend = Qt.binding(function () { return root.pane ? root.pane.backend : null })
                    item.active = Qt.binding(function () { return root.isFont })
                    item.closeRequested.connect(root.close)
                }
            }

            // A file-path Loader, so a window that never compares compiles none of it; the item asks its one diff on load.
            Loader {
                id: compareLoader
                anchors.fill: parent
                active: root.isCompare
                source: "CompareView.qml"
                onLoaded: {
                    item.backend = Qt.binding(function () { return root.pane ? root.pane.backend : null })
                    item.opener = Qt.binding(function () { return root.pane ? root.pane.opener : null })
                    item.pathA = Qt.binding(function () { return root.path })
                    item.pathB = Qt.binding(function () { return root.comparePath })
                    item.iconName = Qt.binding(function () { return root.iconName })
                    item.active = Qt.binding(function () { return root.active })
                    item.closeRequested.connect(root.close)
                }
            }

            Loader {
                id: mediaLoader
                anchors.fill: parent
                onLoaded: {
                    item.path = Qt.binding(function () { return root.path })
                    item.kind = Qt.binding(function () { return root.kind })
                    item.size = Qt.binding(function () { return root.size })
                    item.kindName = Qt.binding(function () { return root.kindName })
                    item.rate = Qt.binding(function () { return root.mediaRate })
                }
            }

            // source rather than sourceComponent, so a file is decoded only while an image is open and its texture goes with the item.
            Loader {
                id: imageLoader
                anchors.fill: parent
                // With no thumbnail to show, built once the size reply lands or the cap passes, so the picture decodes once at the card's final size.
                active: !root.sizeWait || root.interimThumb.length > 0
                onLoaded: {
                    item.path = Qt.binding(function () { return root.path })
                    item.origW = Qt.binding(function () { return root.imageW })
                    item.origH = Qt.binding(function () { return root.imageH })
                    item.thumbPending = Qt.binding(function () { return root.thumbRow >= 0 })
                    item.frames = Qt.binding(function () { return root.imageFrames })
                    item.origOrient = Qt.binding(function () { return root.imageOrient })
                    item.embedded = Qt.binding(function () { return root.rawCopy !== null ? root.rawCopy.file : "" })
                    item.embeddedAnswered = Qt.binding(function () { return root.rawAnswered })
                    item.interimThumb = Qt.binding(function () { return root.interimThumb })
                    item.interimVisible = Qt.binding(function () { return root.interimVisible })
                    item.interimX = Qt.binding(function () { return root.interimBox !== null ? root.interimBox.x : 0 })
                    item.interimY = Qt.binding(function () { return root.interimBox !== null ? root.interimBox.y : 0 })
                    item.interimWidth = Qt.binding(function () { return root.interimBox !== null ? root.interimBox.w : 0 })
                    item.interimHeight = Qt.binding(function () { return root.interimBox !== null ? root.interimBox.h : 0 })
                }
            }

            // The canvas's PdfViewer, source not sourceComponent, so QtQuick.Pdf loads on the first PDF and never for a folder without one.
            Loader {
                id: pdfLoader
                anchors.fill: parent
                onLoaded: {
                    item.path = Qt.binding(function () { return root.path })
                    item.active = true
                    // A document on a hangable class loads from the backend's fetched copy.
                    item.backend = Qt.binding(function () { return root.pane ? root.pane.backend : null })
                    item.fetchFirst = Qt.binding(function () { return root.pane ? ExtThumbs.present(root.pane.storageClass) : false })
                    item.viewerSlot = "quicklook"
                    item.forceActiveFocus()
                }
            }

            Connections {
                target: pdfLoader.item
                function onClosed() { root.close() }
                function onPageAspectChanged() { root.sizeArrived(pdfLoader.item.pageAspect, 1, 1, pdfLoader.item.pageAspect > 0) }
            }

            // The canvas's Archive tile at Quick Look size: the name, the count the index gave, then the entries.
            Column {
                id: archivePane
                anchors.fill: parent
                anchors.margins: Theme.spacing.rowPaddingX
                spacing: Theme.spacing.gap
                visible: root.isArchive && root.archiveMeta !== null && !root.archiveFailed

                // corner: a filename is arbitrary text, so PlainText, the same rule every name on this surface follows.
                Text {
                    width: parent.width
                    text: root.path.substring(root.path.lastIndexOf("/") + 1)
                    color: Theme.color.foreground
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.body
                    textFormat: Text.PlainText
                    elide: Text.ElideMiddle
                }

                Text {
                    width: parent.width
                    text: Facts.archiveLine(root.archiveMeta)
                    color: Theme.color.muted
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    textFormat: Text.PlainText
                }

                Flea.PreviewArchive {
                    width: parent.width
                    height: parent.height - y
                    meta: root.archiveMeta
                }
            }

            // Declined, or an archive whose index could not be read: a mark over the sentence, never a bare surface.
            Column {
                anchors.centerIn: parent
                width: parent.width - 2 * Theme.spacing.rowPaddingX
                spacing: Theme.spacing.gap
                visible: root.kind === "unsupported" || root.archiveFailed || root.fontFailed

                Flea.Glyph {
                    anchors.horizontalCenter: parent.horizontalCenter
                    // The overlay declining is a pane state standing alone, which States.dc.html draws at 40.
                    maxSize: Theme.stateMarkSize
                    width: Theme.stateMarkSize
                    height: Theme.stateMarkSize
                    name: root.archiveFailed ? "alert" : "file"
                    color: root.archiveFailed ? Theme.color.error : Theme.color.mutedFrame
                }

                Text {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    text: root.status
                    color: root.archiveFailed ? Theme.color.foreground : Theme.color.muted
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.body
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }
            }

            // Media still buffering or an image still decoding shows the crawl; LoadingState's hold-off keeps a fast local open from flashing it.
            Flea.LoadingState {
                anchors.fill: parent
                visible: root.loadingEarly || (root.isMedia || root.isImage || root.isArchive || root.isFont) && root.status === "loading"
                heldOff: root.loadingEarly || root.swap !== null && root.swap.fellBack
            }

            // MediaStrip unframed: quiet over the video, permanent on audio, and the column draws the framed form of the same file.
            Flea.MediaStrip {
                id: mediaStrip
                visible: root.isMedia && (root.kind === "audio" || root.stripShown)
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                framed: false
                playing: root.status === "playing"
                position: root.position
                duration: root.duration
                onToggled: root.togglePlay()
                onSeeked: function (ms) { root.seekTo(ms) }
                onTouched: root.revealStrip()
            }
        }

        // The swap wrapper waits for the first open, so launch pays no Loader, compile or picture.
        Loader {
            id: swapLoader
            anchors.fill: parent
            active: false
            source: "QuickLookSwap.qml"
            onLoaded: {
                // The surface's own fill, inside its hairline, so the live border and rounded corners show round the picture.
                item.panesSource = panes
                item.ready = Qt.binding(function () { return root.lookReady })
                item.ground = Qt.binding(function () { return surface.color })
                item.groundInset = Qt.binding(function () { return surface.border.width })
                item.groundRadius = Qt.binding(function () { return Math.max(0, surface.radius - surface.border.width) })
            }
        }
    }

    // During the hold a press or the wheel reveals at once and reaches nothing unseen, so no click closes a card that never drew.
    MouseArea {
        id: holdShield
        anchors.fill: parent
        // Held through its own press, so the release after the reveal clicks nothing either.
        enabled: root.unrevealed || holdShield.pressed
        acceptedButtons: Qt.AllButtons
        onPressed: root.revealNow()
        onWheel: function (wheel) {
            wheel.accepted = true
            root.revealNow()
        }
    }
}
