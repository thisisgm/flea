import QtQuick
import qs.Commons
import "." as Flea
import "js/Facts.js" as Facts
import "js/Kinds.js" as Kinds
import "js/Thumbs.js" as Thumbs
import "js/ExtThumbs.js" as ExtThumbs
import "js/MarkdownPrepared.js" as Prepared
import "js/Motion.js" as Motion
import "js/PreviewSettle.js" as PreviewSettle
import "js/PreviewSwap.js" as PreviewSwap

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
    // RenderedPreviews: a Markdown file keeps the text kind and draws its own bar and pane.
    readonly property bool isMarkdown: root.kind === "text" && Kinds.isMarkdown(root.path)
    // r flips the open Markdown Quick Look to Source; close() forgets it, and a move to another file keeps it.
    property bool markdownSource: false
    // The file whose first Markdown read may block the key, decided from its listing row before the path moves; "" when none may.
    property string inlineMarkdownPath: ""
    // Tab put the keyboard on the Markdown bar's close mark; a move off Markdown or a close lets it go.
    property bool markdownCloseFocus: false
    readonly property bool markdownCloseFocused: root.active && root.isMarkdown && root.markdownCloseFocus
    onIsMarkdownChanged: if (!root.isMarkdown) root.markdownCloseFocus = false
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
        if (root.kind === "text") return root.isMarkdown
            ? (markdownLoader.item ? markdownLoader.item.bodyItem : null) : textPane.bodyItem
        return null
    }
    function mediaLoaded() { return mediaLoader.item !== null }
    function textShown() { return root.isMarkdown
        ? (markdownLoader.item ? markdownLoader.item.rawText : "") : textPane.shownText() }
    function markdownView() { return root.active && root.isMarkdown && markdownLoader.item ? markdownLoader.item.shownView : "" }
    function markdownCloseState() { return JSON.stringify(root.active && root.isMarkdown && markdownLoader.item ? markdownLoader.item.closeState() : {}) }
    function markdownEndGap() { return root.active && root.isMarkdown && markdownLoader.item ? markdownLoader.item.endGap() : -1 }
    function markdownScrollY() { return root.active && root.isMarkdown && markdownLoader.item ? Math.round(markdownLoader.item.scrollY) : -1 }
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
    readonly property string status: {
        if (!root.active) return ""
        // A fresh media item still carries its empty path until onLoaded binds it; its
        // "stopped" there is not a whole preview, so the swap never releases on it.
        if (root.isMedia) return (mediaLoader.item && mediaLoader.item.path === root.path) ? mediaLoader.item.status : "loading"
        if (root.isPdf) return (pdfLoader.item && pdfLoader.item.failed) ? "This file could not be read." : "pdf"
        if (root.isImage) return imageLoader.item ? imageLoader.item.status : "loading"
        if (root.isArchive) return root.archiveMeta === null ? "loading" : (root.archiveFailed ? "This archive could not be read." : "archive")
        if (root.kind === "text") return root.isMarkdown
            ? (markdownLoader.item ? markdownLoader.item.status : "loading") : textPane.status
        return "This file cannot be previewed."
    }
    // The swap's answer: nothing loading, and a PDF with a page on screen or refused.
    // An interim cache thumbnail shown counts as whole, never as a half-built frame.
    readonly property bool lookReady: !root.active || PreviewSwap.lookReady(root.status, root.isPdf,
        root.pdfItem !== null && root.pdfItem.shownPage >= 0, root.pdfItem !== null && root.pdfItem.failed,
        root.interimShown, root.isMarkdown && markdownLoader.item !== null && markdownLoader.item.firstScreen)
    // The cached thumbnail held at open, drawn under the full decode at the final rect; the stamp retires a stale one.
    property string interimThumb: ""
    property string interimStamp: ""
    // The interim's rect off the upright original, null while its pixels are unknown: no
    // interim until then. A cache file is already upright, so no box swap applies here.
    readonly property var interimBox: PreviewSwap.interimRect(panes.width, panes.height,
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
    property int imageRow: -1
    // One meta ask per show; onRowsChanged never re-asks behind it.
    property bool imageAsked: false
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

    // Read through to PreviewMedia so this file never imports QtMultimedia, and 0 before the loader has an item.
    readonly property int position: (root.isMedia && mediaLoader.item) ? mediaLoader.item.position : 0
    readonly property int duration: (root.isMedia && mediaLoader.item) ? mediaLoader.item.duration : 0

    // Shown on open, hidden stripHideMs after the last reveal, video only: audio has nothing else to look at.
    property bool stripShown: true
    // StatusBar.messageMs, the OEM's own transient interval, which Sidebar's unmount arm reuses for the same reason.
    readonly property int stripHideMs: 4000

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

    function zoomBy(steps) { if (root.isPdf && pdfLoader.item) pdfLoader.item.zoomBy(steps) }

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
        if (document) document.scrollBy(direction * (isPage ? (root.isMarkdown ? document.viewportHeight : document.height) / 2 : Theme.font.body))
    }

    // GM 2026-10-03: a Markdown Quick Look opens rendered, and r flips only the open one (no stored choice).
    function toggleMarkdownView() {
        if (root.isMarkdown) root.markdownSource = !root.markdownSource
    }

    function toggleMarkdownClose() {
        if (root.isMarkdown) root.markdownCloseFocus = !root.markdownCloseFocus
    }

    // The picture is taken now, so the settled load below changes the panes under it.
    function follow(newPath, newIcon, newSize, newKind, newThumb) {
        var key = newPath + "\n" + newIcon + "\n" + newSize + "\n" + newKind
        // Pending covers a repeat; a settled revisit of the shown target stays put, anything else reloads.
        if (key === root.lastMoveKey && followSettle.running) return
        if (key === root.lastMoveKey && root.isShown(newPath, newIcon, newSize, newKind)) return
        var now = Date.now()
        var idle = PreviewSettle.due(now, root.lastMoveAt, root.followSettleMs)
        root.lastMoveKey = key
        root.lastMoveAt = now
        root.pendingPath = newPath
        root.pendingIcon = newIcon
        root.pendingSize = newSize
        root.pendingKind = newKind
        root.pendingThumb = newThumb || ""
        root.pendingImageRow = root.pane ? root.pane.cursorIndex : -1
        if (root.active) {
            var held = root.ensureSwap()
            if (held)
                held.hold(null, newPath)
        }
        if (idle) {
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
        followSettle.stop()
        root.lastMoveKey = ""
        root.pendingThumb = ""
        root.pendingImageRow = -1
        root.interimThumb = ""
        root.interimStamp = ""
        root.imageW = 0
        root.imageH = 0
        root.imageOrient = 1
        root.imageRow = -1
        root.imageAsked = false
        stripHideTimer.stop()
        root.active = false
        root.kind = ""
        root.markdownSource = false
        root.markdownCloseFocus = false
        root.inlineMarkdownPath = ""
        mediaLoader.source = ""
        pdfLoader.source = ""
        imageLoader.source = ""
        root.archiveMeta = null
        root.archiveRow = -1
        root.mediaRate = 0
        root.mediaRow = -1
        if (root.pane) root.pane.listArea.forceActiveFocus()
    }

    // Under the held picture when a move took one; Space's own open has none and draws as it builds.
    // The swap starts after its mutation: show runs at once or waits for the capture, start runs with it.
    function load(newPath, newIcon, newSize, newKind, newThumb, newRow) {
        var isPdf = Kinds.quickLookKind(newIcon, newPath) === Kinds.PDF
        var swapItem = root.ensureSwap()
        var show = function () {
            root.show(newPath, newIcon, newSize, newKind, newThumb, newRow)
            if (swapItem) swapItem.start(isPdf)
        }
        if (swapItem && (swapItem.holding || swapItem.capturing))
            swapItem.hold(show, newPath, true)
        else
            show()
    }

    function show(newPath, newIcon, newSize, newKind, newThumb, newRow) {
        var row = root.pane ? root.pane.rowFor(newRow) : null
        var named = row && root.pane.join(root.pane.path, row.n) === newPath
        root.inlineMarkdownPath = named && Prepared.readsInline(row, root.pane.storageClass, root.pane.storageKnown) ? newPath : ""
        root.kind = Kinds.quickLookKind(newIcon, newPath)
        // A pane of another kind goes before the path moves, or it tries to open a file it cannot draw: an image
        // pane handed a video logged "Unsupported image format" on every move from a picture to a clip.
        if (!root.isMedia) mediaLoader.source = ""
        if (!root.isPdf) pdfLoader.source = ""
        if (!root.isImage) imageLoader.source = ""
        // The interim holds the path it was read for; a stale one never outlives a move.
        root.interimThumb = newThumb || ""
        root.interimStamp = newPath
        // The meta row is the one captured at open/follow, never the cursor now.
        root.imageRow = root.isImage && (newThumb || "").length > 0 && newRow !== undefined ? newRow : -1
        root.imageW = 0
        root.imageH = 0
        root.imageOrient = 1
        root.imageAsked = false
        root.path = newPath
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
        root.revealStrip()
        root.maybePrefetch()
    }

    // Bounded prefetch: the next row's cache entry only, once per rest, never a decode and never a meta.
    function maybePrefetch() {
        if (!root.active || !root.pane || followSettle.running) return
        if (!ViewState.previewAutomatic || root.pane.listInFlight || !root.pane.storageKnown) return
        if (ExtThumbs.manualHold(root.pane.storageClass, ViewState.preview)) return
        var next = root.pane.cursorIndex + 1
        if (root.pane.thumbState.file[next] !== undefined) return
        var row = root.pane.rowFor(next)
        if (!row || row.d || row.t !== true || !Thumbs.allowed(row, ViewState.thumbnailMode)) return
        var work = { ask: [next], drop: [], cacheOnly: true }
        root.pane.thumbState = Thumbs.applied(root.pane.thumbState, work)
        root.pane.backend.thumb(work.ask, true)
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

    // The original's pixels, which size the interim like the final; only with a thumb to draw,
    // once per show, and never while a held key is still bursting.
    function askImage() {
        if (!root.isImage || root.interimThumb.length === 0 || !root.pane) return
        if (root.imageRow < 0 || root.imageAsked || followSettle.running) return
        // An insert or a re-sort between capture and ask moves another file to the index,
        // so the row must still name the shown file before anything is asked for it.
        root.resolveImageRow()
        root.imageAsked = true
        if (root.imageRow < 0) return
        root.pane.backend.askMeta(root.imageRow, false, false, false)
    }

    // The row at imageRow still names the shown file: its name under the pane's path is
    // the path this show opened, the way the rest of this file identifies a row.
    function imageRowShown() {
        if (!root.pane || root.imageRow < 0) return false
        var row = root.pane.rowFor(root.imageRow)
        return row !== null && root.pane.join(root.pane.path, row.n) === root.path
    }

    // The captured index drifted onto another file: take the cursor when it names the
    // shown file instead, else drop the interim for this show with no ask.
    function resolveImageRow() {
        if (root.imageRowShown()) return
        var at = root.pane ? root.pane.cursorIndex : -1
        var row = root.pane && at >= 0 ? root.pane.rowFor(at) : null
        if (row !== null && root.pane.join(root.pane.path, row.n) === root.path)
            root.imageRow = at
        else
            root.imageRow = -1
    }

    Connections {
        target: root.pane ? root.pane.backend : null
        function onMeta(row, w, h, orient, durationMs, sampleRate, entries, unpacked, archiveFailed, names, lines, partial, linesFailed, target, targetDir, owner) {
            if (root.isArchive && row === root.archiveRow)
                root.archiveMeta = { entries: entries, unpacked: unpacked, archiveFailed: archiveFailed, names: names }
            if (root.isMedia && row === root.mediaRow)
                root.mediaRate = sampleRate
            if (root.isImage && row === root.imageRow && root.imageRowShown()) {
                root.imageW = w
                root.imageH = h
                root.imageOrient = orient
            }
        }
    }

    // A meta asked across a listing change is answered with silence, so the rows landing re-asks it, the way ui/ColumnsArea.qml does.
    Connections {
        target: root.pane
        function onRowsChanged() {
            if (root.active && root.isArchive && root.archiveMeta === null) root.askArchive()
            if (root.active && root.isMedia && root.mediaRate === 0) root.askMedia()
            if (root.active && root.isImage && root.imageW === 0) root.askImage()
        }
    }

    Timer {
        id: followSettle
        interval: root.followSettleMs
        repeat: false
        onTriggered: root.load(root.pendingPath, root.pendingIcon, root.pendingSize, root.pendingKind, root.pendingThumb, root.pendingImageRow)
    }

    Timer {
        id: stripHideTimer
        interval: root.stripHideMs
        repeat: false
        onTriggered: root.stripShown = false
    }

    MouseArea {
        anchors.fill: parent
        // A shield that outlives the overlay swallows the first click after it and freezes row hover for the window.
        enabled: root.active
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        hoverEnabled: true
        // A click behind the overlay would silently move the cursor or open the listing's menu on it.
        // A left click outside the surface closes, GM's rule of 2026-09-11, and a right click is only swallowed.
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
        anchors.fill: parent
        color: Theme.color.background
        opacity: root.active ? root.groundOpacity : 0
        Behavior on opacity {
            enabled: !Theme.reducedMotion
            NumberAnimation { duration: root.active ? Motion.durMs.open : Motion.durMs.close; easing.type: Easing.OutCubic }
        }
    }

    Rectangle {
        id: surface
        x: Theme.cardOrigin(parent.width, width)
        y: Theme.cardOrigin(parent.height, height) + surface.rise
        border.width: Theme.spacing.hairline
        border.color: Theme.color.muted
        // Open rises into place and close only fades, faster, because the translation is enabled: root.active.
        property real rise: root.active ? 0 : Motion.translateUpPx
        opacity: root.active ? 1 : 0
        // Expand drops the Quick Look inset, which is the whole of the canvas's "expand fills the window".
        readonly property real inset: root.pdfExpanded ? 1 : Theme.preview.fraction
        width: Theme.cardSpan(parent.width * surface.inset, parent.width)
        height: Theme.cardSpan(parent.height * surface.inset, parent.height)
        color: Theme.color.surface
        // Mirrors hyprland decoration:rounding; media fills the surface and keeps square corners, a visible corner only shows on text and audio panes.
        radius: Style.cornerRadius

        Behavior on rise {
            enabled: root.active && !Theme.reducedMotion
            NumberAnimation { duration: Motion.durMs.open; easing.type: Easing.OutCubic }
        }

        Behavior on opacity {
            enabled: !Theme.reducedMotion
            NumberAnimation {
                duration: root.active ? Motion.durMs.open : Motion.durMs.close
                easing.type: Easing.OutCubic
            }
        }

        // Every kind's pane, eager the way 0.3.5 drew them: the first Space decodes and never
        // builds. Moving to the next file holds this one's picture through the swap below until that one is whole.
        Item {
            id: panes
            anchors.fill: parent

            Flea.PreviewText {
                id: textPane
                anchors.fill: parent
                anchors.margins: Theme.spacing.gap
                active: root.kind === "text" && !root.isMarkdown
                path: root.path
                size: root.size
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
                active: root.isMarkdown
                source: "MarkdownPane.qml"
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
                    item.active = Qt.binding(function () { return root.isMarkdown })
                    item.closeFocused = Qt.binding(function () { return root.markdownCloseFocused })
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
                onLoaded: {
                    item.path = Qt.binding(function () { return root.path })
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
                visible: root.kind === "unsupported" || root.archiveFailed

                Flea.Glyph {
                    anchors.horizontalCenter: parent.horizontalCenter
                    // The overlay declining is a pane state standing alone, which States.dc.html draws at 40.
                    maxSize: Theme.stateMarkSize
                    width: Theme.stateMarkSize
                    height: Theme.stateMarkSize
                    name: root.archiveFailed ? "alert" : "file"
                    color: root.archiveFailed ? Theme.color.error : Theme.color.muted
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
                visible: (root.isMedia || root.isImage || root.isArchive) && root.status === "loading"
                heldOff: root.swap !== null && root.swap.fellBack
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
}
