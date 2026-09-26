import QtQuick
import Quickshell
import qs.Commons
import "." as Flea
import "js/Facts.js" as Facts
import "js/Format.js" as Format
import "js/Icons.js" as Icons
import "js/Kinds.js" as Kinds
import "js/Motion.js" as Motion
import "js/PreviewKeys.js" as PreviewKeys
import "js/PreviewPaths.js" as PreviewPaths

// The overlay lives inside the Flea window, Finder's Quick Look shape: a second window breaks omarchy-drive focus flea and every test that narrows on it.
Item {
    id: root
    anchors.fill: parent
    // active flips instantly, so the IPC read never races the close fade; visible outlives it until surface's own opacity finishes.
    visible: root.active || surface.opacity > 0
    z: 1

    property bool active: false
    // The normal preview is an inset panel. The custom build places it in a separate 80% window.
    property bool floating: false
    property var windowHost: null
    focus: root.active
    property string heldNavigationAction: ""
    property int previewIndex: -1
    property string pendingFolderPath: ""
    property var folderItems: []
    property real contentZoom: 1
    property int savedPaneCursor: -1
    // shell.qml wires the pane in: the archive pane asks the backend about the cursor row and nothing else here reads it.
    property var pane: null
    function isImagePath(path) {
        return /\.(avif|bmp|gif|heic|heif|jpe?g|png|svg|tif?f|webp|arw|cr2|cr3|dng|nef|orf|raf|rw2)$/i.test(path)
    }
    function isPreviewableRow(row, path) {
        return row && !row.d && Kinds.quickLookKind(row.i, path) !== Kinds.UNSUPPORTED
    }
    function rowPath(row) {
        return PreviewPaths.rowPath(root.pane, row)
    }
    readonly property var previewItems: {
        var items = []
        if (root.pendingFolderPath.length > 0) return root.folderItems
        if (!root.pane || root.pane.rows.length === 0) return items
        for (var i = 0; i < root.pane.rows.length; i++) {
            var row = root.pane.rows[i]
            var itemPath = root.rowPath(row)
            if (!root.isPreviewableRow(row, itemPath))
                continue
            items.push({
                index: root.pane.held + i,
                path: itemPath,
                icon: row.i,
                size: row.s,
                thumb: root.pane.thumbFor(i)
            })
        }
        return items
    }
    readonly property var imageItems: root.previewItems.filter(function (item) { return root.isImagePath(item.path) })
    readonly property var filmstripItems: root.previewItems
    readonly property string currentThumb: {
        for (var i = 0; i < root.filmstripItems.length; i++)
            if (root.filmstripItems[i].path === root.path) return root.filmstripItems[i].thumb || ""
        return ""
    }
    readonly property string previousThumb: root.previewIndex > 0
        ? root.filmstripItems[root.previewIndex - 1].thumb || "" : ""
    readonly property string nextThumb: root.previewIndex >= 0 && root.previewIndex + 1 < root.filmstripItems.length
        ? root.filmstripItems[root.previewIndex + 1].thumb || "" : ""
    readonly property int imageFilmstripIndex: {
        for (var i = 0; i < root.filmstripItems.length; i++)
            if (root.filmstripItems[i].path === root.path) return i
        return -1
    }
    property string path: ""
    property string iconName: ""
    property string kindName: ""
    property int size: 0
    property string kind: ""
    readonly property bool isMedia: root.kind === "audio" || root.kind === "video"
    readonly property bool isPdf: root.kind === "pdf"
    readonly property bool isImage: root.kind === "image"
    readonly property bool isArchive: root.kind === "archive"
    // The backend's meta answer for the open archive, null until it lands; archiveRow is the row it was asked for.
    property var archiveMeta: null
    property int archiveRow: -1
    // MediaPdf rule 6's fourth fact: Qt carries no sample-rate key at all (QMediaMetaData::Key, Qt 6.11), so the number is the backend probe's, asked the way an archive's is.
    property int mediaRate: 0
    property int mediaRow: -1
    property var infoMeta: null
    property int infoRow: -1
    property string infoRequestPath: ""
    property string archiveRequestPath: ""
    readonly property bool archiveFailed: root.isArchive && root.archiveMeta !== null && root.archiveMeta.archiveFailed === true
    // For ui/Ipc.qml: the item drawing this kind's content, whether a player exists, and what the text and archive panes hold.
    function surfaceItem() {
        if (root.isImage) return imageLoader.item
        if (root.isMedia) return mediaLoader.item
        if (root.isPdf) return pdfLoader.item
        if (root.isArchive) return archivePane
        if (root.kind === "text") return textPane.bodyItem
        return null
    }
    function mediaLoaded() { return mediaLoader.item !== null }
    function textShown() { return textPane.shownText() }
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
        if (root.isMedia) return mediaLoader.item ? mediaLoader.item.status : "loading"
        if (root.isPdf) return (pdfLoader.item && pdfLoader.item.failed) ? "This file could not be read." : "pdf"
        if (root.isImage) return imageLoader.item ? imageLoader.item.status : "loading"
        if (root.isArchive) return root.archiveMeta === null ? "loading" : (root.archiveFailed ? "This archive could not be read." : "archive")
        if (root.kind === "text") return textPane.status
        return "This file cannot be previewed."
    }

    property string pendingPath: ""
    property string pendingIcon: ""
    property string pendingKind: ""
    property int pendingSize: 0
    // The settle idiom Pane's own thumbnail request reuses: a held j/k costs zero reloads until the cursor rests.
    readonly property int followSettleMs: 120
    // The same dim ui/SettingsPanel.qml lays over the listing.
    readonly property real groundOpacity: root.floating ? 1 : 0.5

    // Read through to PreviewMedia so this file never imports QtMultimedia, and 0 before the loader has an item.
    readonly property int position: (root.isMedia && mediaLoader.item) ? mediaLoader.item.position : 0
    readonly property int duration: {
        var playerDuration = (root.isMedia && mediaLoader.item) ? mediaLoader.item.duration : 0
        if (playerDuration > 0)
            return playerDuration
        return root.infoMeta && root.infoMeta.ms > 0 ? root.infoMeta.ms : 0
    }

    // Shown on open, hidden stripHideMs after the last reveal, video only: audio has nothing else to look at.
    property bool stripShown: true
    property bool filmstripShown: true
    property bool navbarShown: true
    property bool infoOpen: false
    property bool infoPanelSessionSet: false
    property real lastPointerX: -1
    property real lastPointerY: -1
    readonly property real infoInset: root.infoOpen ? infoPanel.width : 0
    readonly property bool previewFullscreen: root.windowHost !== null && root.windowHost.fullscreen
    // StatusBar.messageMs, the OEM's own transient interval, which Sidebar's unmount arm reuses for the same reason.
    readonly property int stripHideMs: 4000

    function revealNavbar() {
        root.navbarShown = true
        if (ViewState.previewHideNavbarFullscreen && root.previewFullscreen)
            navbarHideTimer.restart()
        else
            navbarHideTimer.stop()
    }

    function revealStrip() {
        root.stripShown = true
        stripHideTimer.restart()
    }

    function revealFilmstrip() {
        root.filmstripShown = true
        if (ViewState.previewHideFilmstrip && (root.status === "playing" || root.previewFullscreen))
            filmstripHideTimer.restart()
        else
            filmstripHideTimer.stop()
    }

    onStatusChanged: {
        if (root.filmstripShown)
            root.revealFilmstrip()
        else if (ViewState.previewHideFilmstrip && (root.status === "playing" || root.previewFullscreen))
            filmstripHideTimer.restart()
    }
    onPreviewFullscreenChanged: {
        root.revealNavbar()
        if (root.previewFullscreen) {
            if (root.filmstripShown)
                filmstripHideTimer.restart()
        } else {
            root.revealFilmstrip()
        }
    }

    function togglePlay() { if (root.isMedia && mediaLoader.item) mediaLoader.item.togglePlay() }

    // MediaMute rule 3: one session flag, so the column's strip and this one always agree.
    function toggleMute() { if (root.isMedia) Flea.MediaSound.toggle() }

    function toggleInfo() { root.infoOpen = !root.infoOpen }

    function toggleFilmstrip() {
        root.filmstripShown = !root.filmstripShown
        filmstripHideTimer.stop()
    }

    function resetZoom() {
        if (root.isImage) root.contentZoom = 1
        else if (root.isPdf && pdfLoader.item) pdfLoader.item.zoom = pdfLoader.item.minZoom
    }

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
    function open(newPath, newIcon, newSize, newKind) {
        followSettle.stop()
        if (Kinds.quickLookKind(newIcon, newPath) === Kinds.UNSUPPORTED && root.pane) {
            var cursor = root.pane.cursorIndex
            for (var next = 0; next < root.previewItems.length; next++) {
                if (root.previewItems[next].index > cursor) {
                    var item = root.previewItems[next]
                    root.open(item.path, item.icon, item.size)
                    return
                }
            }
            return
        }
        for (var i = 0; i < root.previewItems.length; i++) {
            if (root.previewItems[i].path === newPath) {
                root.previewIndex = i
                break
            }
        }
        root.load(newPath, newIcon, newSize, newKind)
        Qt.callLater(root.forceActiveFocus)
    }

    function movePreview(delta) {
        root.revealFilmstrip()
        var next = root.previewIndex + delta
        if (root.previewItems.length === 0) return
        next = Math.max(0, Math.min(root.previewItems.length - 1, next))
        if (next === root.previewIndex) return
        var item = root.previewItems[next]
        root.previewIndex = next
        root.open(item.path, item.icon, item.size)
    }

    function openDefault() {
        if (root.pane && root.path.length > 0) {
            root.pane.openFile(root.path)
            root.close()
        }
    }

    function toggleFullscreen() {
        if (!root.windowHost) return
        root.savedPaneCursor = root.pane ? root.pane.cursorIndex : -1
        Quickshell.execDetached(["/home/adam/Code/flea-custom/tools/flea-preview-cursor", "save"])
        root.windowHost.fullscreen = !root.windowHost.fullscreen
        if (root.pane && root.savedPaneCursor >= 0)
            root.pane.cursorIndex = root.savedPaneCursor
    }

    function openFolder(folderPath) {
        root.pendingFolderPath = folderPath
        root.folderItems = []
        root.previewIndex = -1
        root.active = false
        root.infoOpen = false
        root.pane.backend.peek(folderPath, 1000, root.pane.showHidden)
        folderPreviewPoll.start()
        Qt.callLater(root.forceActiveFocus)
    }

    function previewFirstFolderItem() {
        if (root.pendingFolderPath.length === 0)
            return
        for (var i = 0; i < root.folderItems.length; i++) {
            var row = root.folderItems[i]
            if (row) {
                root.pendingFolderPath = ""
                root.open(row.path, row.icon || "", row.size || 0)
                return
            }
        }
    }

    function openNextCurrentItem() {
        var cursor = root.pane ? root.pane.cursorIndex : -1
        root.pendingFolderPath = ""
        for (var i = 0; i < root.previewItems.length; i++) {
            if (root.previewItems[i].index > cursor) {
                var item = root.previewItems[i]
                root.open(item.path, item.icon, item.size)
                return
            }
        }
    }

    function follow(newPath, newIcon, newSize, newKind) {
        root.pendingPath = newPath
        root.pendingIcon = newIcon
        root.pendingSize = newSize
        root.pendingKind = newKind
        followSettle.restart()
    }

    // Dropping the loader's source is what stops playback: media dies with the loader.
    function close() {
        followSettle.stop()
        stripHideTimer.stop()
        filmstripHideTimer.stop()
        navbarHideTimer.stop()
        if (mediaLoader.item) mediaLoader.item.stop()
        root.active = false
        root.kind = ""
        root.filmstripShown = true
        mediaLoader.source = ""
        pdfLoader.source = ""
        imageLoader.source = ""
        root.archiveMeta = null
        root.infoMeta = null
        root.archiveRow = -1
        root.infoRow = -1
        root.infoRequestPath = ""
        root.mediaRate = 0
        root.mediaRow = -1
        root.archiveRequestPath = ""
        root.pendingFolderPath = ""
        root.folderItems = []
        if (root.pane) root.pane.listArea.forceActiveFocus()
    }

    function load(newPath, newIcon, newSize, newKind) {
        if (mediaLoader.item) mediaLoader.item.stop()
        root.path = newPath
        root.iconName = newIcon
        root.size = newSize
        // MediaPdf rule 6: the overlay is the bigger surface, so it says at least what the column says, and the kind is the caller's because the backend named it for that row.
        root.kindName = newKind || ""
        root.kind = Kinds.quickLookKind(newIcon, newPath)
        root.contentZoom = 1
        root.navbarShown = true
        root.lastPointerX = -1
        root.lastPointerY = -1
        navbarHideTimer.stop()
        if (!root.infoPanelSessionSet) {
            root.infoOpen = ViewState.previewInfoPanel
            root.infoPanelSessionSet = true
        }
        root.active = true
        mediaLoader.source = root.isMedia ? "PreviewMedia.qml" : ""
        pdfLoader.source = root.isPdf ? "PdfViewer.qml" : ""
        imageLoader.source = root.isImage ? "PreviewImage.qml" : ""
        root.askInfo()
        root.revealStrip()
    }

    // The preview is a separate window, so the browser's Pane no longer receives these keys.
    // Keep the same navigation rules here, including media seek and PDF page movement.
    Keys.onPressed: function (event) {
        if (!root.active) return
        root.revealNavbar()
        if (event.isAutoRepeat && root.heldNavigationAction.length > 0) {
            event.accepted = true
            return
        }
        var action = ""
        if (event.key === Qt.Key_Space) {
            if (root.isMedia) root.togglePlay()
            else root.close()
            event.accepted = true
            return
        }
        if (event.key === Qt.Key_Escape) {
            root.close()
            event.accepted = true
            return
        }
        if (event.key === Qt.Key_I && event.modifiers === Qt.NoModifier) {
            root.toggleInfo()
            event.accepted = true
            return
        }
        if (event.key === Qt.Key_T && event.modifiers === Qt.NoModifier) {
            root.toggleFilmstrip()
            event.accepted = true
            return
        }
        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
            root.openDefault()
            event.accepted = true
            return
        }
        if (event.key === Qt.Key_Up) action = "cursorUp"
        else if (event.key === Qt.Key_Down) action = "cursorDown"
        else if (event.key === Qt.Key_Left) action = root.isPdf ? "seekBack" : "cursorUp"
        else if (event.key === Qt.Key_Right) action = root.isPdf ? "seekForward" : "cursorDown"
        else if (event.key === Qt.Key_F11 && root.windowHost) {
            root.toggleFullscreen()
            event.accepted = true
            return
        }
        if (action.length > 0 && root.pane) {
            PreviewKeys.act(action, root.pane)
            root.heldNavigationAction = action
            navigationRepeat.start()
            event.accepted = true
        }
    }

    Keys.onReleased: function (event) {
        if (event.key === Qt.Key_Up || event.key === Qt.Key_Down
                || event.key === Qt.Key_Left || event.key === Qt.Key_Right) {
            root.heldNavigationAction = ""
            navigationRepeat.stop()
            event.accepted = true
        }
    }

    Timer {
        id: navigationRepeat
        interval: 700
        repeat: true
        onTriggered: {
            if (root.active && root.pane && root.heldNavigationAction.length > 0)
                PreviewKeys.act(root.heldNavigationAction, root.pane)
            else
                stop()
        }
    }

    // One row, only while an archive is the thing open: the same no-sweep rule the column follows.
    function askArchive() {
        root.archiveMeta = null
        root.archiveRow = root.isArchive ? root.metadataRow() : -1
        root.archiveRequestPath = ""
        if (root.archiveRow >= 0 && root.pane) {
            var row = root.pane.rowFor(root.archiveRow)
            if (row) root.archiveRequestPath = root.pane.join(root.pane.path, row.n)
        }
        if (root.archiveRow >= 0)
            root.pane.backend.askMeta(root.archiveRow, false, false, true)
    }

    // The same one row, for the one fact the transport under the overlay cannot report.
    function askMedia() {
        root.mediaRate = 0
        root.mediaRow = root.isMedia ? root.metadataRow() : -1
        if (root.mediaRow >= 0)
            root.pane.backend.askMeta(root.mediaRow, false, true, false)
    }

    function metadataRow() {
        if (!root.pane)
            return -1
        if (root.previewIndex >= 0 && root.previewIndex < root.filmstripItems.length) {
            var item = root.filmstripItems[root.previewIndex]
            if (item && typeof item.index === "number" && item.index >= 0)
                return item.index
        }
        return root.pendingFolderPath.length > 0 ? -1 : root.pane.cursorIndex
    }

    function askInfo() {
        root.infoMeta = null
        root.infoRow = root.metadataRow()
        root.infoRequestPath = root.path
        root.archiveMeta = null
        root.archiveRow = root.isArchive ? root.infoRow : -1
        root.archiveRequestPath = root.isArchive ? root.path : ""
        root.mediaRate = 0
        root.mediaRow = root.isMedia ? root.infoRow : -1
        if (root.infoRow >= 0)
            root.pane.backend.askMeta(root.infoRow, root.kind === "text", root.isMedia, root.isArchive)
    }

    Connections {
        target: root.pane ? root.pane.backend : null
        function onMeta(row, w, h, durationMs, sampleRate, frameRate, bitrate, entries, unpacked, archiveFailed, names, lines, partial, linesFailed, target, targetDir, owner) {
            if (row === root.infoRow && root.path === root.infoRequestPath)
                root.infoMeta = {w: w, h: h, ms: durationMs, rate: sampleRate, fps: frameRate, bitrate: bitrate,
                    entries: entries, unpacked: unpacked, archiveFailed: archiveFailed, names: names,
                    lines: lines, partial: partial, linesFailed: linesFailed, target: target, targetDir: targetDir, owner: owner}
            if (root.isArchive && row === root.archiveRow && root.path === root.archiveRequestPath)
                root.archiveMeta = { entries: entries, unpacked: unpacked, archiveFailed: archiveFailed, names: names }
            if (root.isMedia && row === root.mediaRow)
                root.mediaRate = sampleRate
        }
    }

    // A meta asked across a listing change is answered with silence, so the rows landing re-asks it, the way ui/ColumnsArea.qml does.
    Connections {
        target: root.pane ? root.pane.backend : null
        function onPeeked(path, hidden, total, rows, readFailed, mode) {
            if (root.pendingFolderPath.length === 0 || path !== root.pendingFolderPath)
                return
            var items = []
            for (var i = 0; i < rows.length; i++) {
                var row = rows[i]
                var itemPath = root.pane.join(path, row.n)
                if (root.isPreviewableRow(row, itemPath))
                    items.push({ path: itemPath, icon: row.i || "", size: row.s || 0, thumb: "" })
            }
            root.folderItems = items
            if (items.length === 0) {
                root.openNextCurrentItem()
                return
            }
            root.active = true
            root.previewFirstFolderItem()
        }
    }

    Connections {
        target: root.pane
        function onRowsChanged() {
            if (root.active && root.infoMeta === null) root.askInfo()
        }
    }

    Timer {
        id: folderPreviewPoll
        interval: 100
        repeat: true
        onTriggered: {
            root.previewFirstFolderItem()
            if (root.pendingFolderPath.length === 0) {
                stop()
            } else if (root.folderItems.length === 0 && root.pane) {
                root.pane.backend.peek(root.pendingFolderPath, 1000, root.pane.showHidden)
            }
        }
    }

    Timer {
        id: followSettle
        interval: root.followSettleMs
        repeat: false
        onTriggered: root.load(root.pendingPath, root.pendingIcon, root.pendingSize, root.pendingKind)
    }

    Timer {
        id: stripHideTimer
        interval: root.stripHideMs
        repeat: false
        onTriggered: root.stripShown = false
    }

    Timer {
        id: filmstripHideTimer
        interval: root.stripHideMs
        repeat: false
        onTriggered: {
            if (ViewState.previewHideFilmstrip && (root.status === "playing" || root.previewFullscreen))
                root.filmstripShown = false
        }
    }

    Timer {
        id: navbarHideTimer
        interval: ViewState.previewNavbarHideMs
        repeat: false
        onTriggered: if (ViewState.previewHideNavbarFullscreen && root.previewFullscreen) root.navbarShown = false
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
        onPositionChanged: function (mouse) {
            if (mouse.x === root.lastPointerX && mouse.y === root.lastPointerY)
                return
            root.lastPointerX = mouse.x
            root.lastPointerY = mouse.y
            root.revealStrip()
            root.revealNavbar()
        }
    }

    // PdfViewer.html and MediaPlayer.html draw a pane with its own edge: on the surface colour alone the inset vanished into the listing behind it.
    Rectangle {
        anchors.fill: parent
        color: Theme.color.background
        opacity: root.active ? root.groundOpacity : 0
        Behavior on opacity {
            enabled: !Theme.reducedMotion
            NumberAnimation { duration: root.active ? Motion.durMs.open : Motion.durMs.close }
        }
    }

    Rectangle {
        id: surface
        anchors.centerIn: parent
        border.width: Theme.spacing.hairline
        border.color: Theme.color.muted
        // Open rises into place and close only fades, faster, because the translation is enabled: root.active.
        anchors.verticalCenterOffset: root.active ? 0 : Motion.translateUpPx
        opacity: root.active ? 1 : 0
        // Expand drops the Quick Look inset, which is the whole of the canvas's "expand fills the window".
        readonly property real inset: root.floating || root.pdfExpanded ? 1 : Theme.preview.fraction
        width: parent.width * surface.inset
        height: parent.height * surface.inset
        color: Theme.color.surface
        clip: true
        // Mirrors hyprland decoration:rounding; media fills the surface and keeps square corners, a visible corner only shows on text and audio panes.
        radius: Style.cornerRadius

        Behavior on anchors.verticalCenterOffset {
            enabled: root.active && !Theme.reducedMotion
            NumberAnimation { duration: Motion.durMs.open; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.bezierCurve }
        }

        Behavior on opacity {
            enabled: !Theme.reducedMotion
            NumberAnimation {
                duration: root.active ? Motion.durMs.open : Motion.durMs.close
                easing.type: Easing.BezierSpline
                easing.bezierCurve: Motion.bezierCurve
            }
        }

        Rectangle {
            id: fileNameBar
            visible: root.active
            enabled: root.navbarShown
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            height: Theme.chromeHeight
            color: Theme.color.surface
            z: 4
            opacity: root.navbarShown ? 1 : 0

            Text {
                anchors.left: parent.left
                anchors.leftMargin: Theme.spacing.rowPaddingX
                anchors.right: tools.left
                anchors.rightMargin: Theme.spacing.gap
                anchors.verticalCenter: parent.verticalCenter
                text: root.path.substring(root.path.lastIndexOf("/") + 1)
                color: Theme.color.foreground
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                textFormat: Text.PlainText
                elide: Text.ElideMiddle
            }

            Row {
                id: tools
                anchors.right: parent.right
                anchors.rightMargin: Theme.spacing.rowPaddingX
                anchors.verticalCenter: parent.verticalCenter
                spacing: Theme.spacing.gap

                Flea.ChromeButton {
                    glyph: "info"
                    accessName: "File info"
                    restingColor: root.infoOpen ? Theme.color.accent : Theme.color.foreground
                    onActivated: root.toggleInfo()
                }

                Flea.ChromeButton {
                    glyph: "minus"
                    accessName: "Zoom out"
                    restingColor: Theme.color.foreground
                    disabledOpacity: 0.55
                    enabled: root.isImage && root.contentZoom > 0.5
                    onActivated: root.contentZoom = Math.max(0.5, root.contentZoom - 0.25)
                }

                Flea.ChromeButton {
                    glyph: "plus"
                    accessName: "Zoom in"
                    restingColor: Theme.color.foreground
                    disabledOpacity: 0.55
                    enabled: root.isImage && root.contentZoom < 4
                    onActivated: root.contentZoom = Math.min(4, root.contentZoom + 0.25)
                }

                Flea.ChromeButton {
                    glyph: "undo"
                    accessName: "Reset zoom"
                    restingColor: Theme.color.foreground
                    enabled: (root.isImage && root.contentZoom !== 1) || (root.isPdf && pdfLoader.item && pdfLoader.item.zoom !== pdfLoader.item.minZoom)
                    onActivated: root.resetZoom()
                }

                Flea.ChromeButton {
                    glyph: "maximize"
                    accessName: "Expand"
                    restingColor: Theme.color.foreground
                    onActivated: root.toggleFullscreen()
                }

                Flea.ChromeButton {
                    glyph: "x"
                    accessName: "Close"
                    restingColor: Theme.color.foreground
                    onActivated: root.close()
                }
            }
        }

        Flea.PreviewText {
            id: textPane
            anchors.top: fileNameBar.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            scale: root.contentZoom
            transformOrigin: Item.TopLeft
            clip: true
            anchors.margins: Theme.spacing.gap
            anchors.rightMargin: Theme.spacing.gap + root.infoInset
            active: root.kind === "text"
            path: root.path
            size: root.size
        }

        Loader {
            id: mediaLoader
            anchors.top: fileNameBar.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.rightMargin: root.infoInset
            scale: root.contentZoom
            transformOrigin: Item.TopLeft
            clip: true
            onLoaded: {
                item.path = Qt.binding(function () { return root.path })
                item.kind = Qt.binding(function () { return root.kind })
                item.size = Qt.binding(function () { return root.size })
                item.kindName = Qt.binding(function () { return root.kindName })
                item.rate = Qt.binding(function () { return root.mediaRate })
                item.probedDuration = Qt.binding(function () {
                    return root.infoMeta && root.infoMeta.ms > 0 ? root.infoMeta.ms : 0
                })
                item.thumb = Qt.binding(function () { return root.currentThumb })
            }
        }

        // Warm the Qt image cache for the two files navigation can reach next.
        Image {
            source: ViewState.previewPreloadAdjacent && root.previousThumb.length > 0 ? Format.fileUri(root.previousThumb) : ""
            asynchronous: true
            cache: true
            visible: false
        }

        Image {
            source: ViewState.previewPreloadAdjacent && root.nextThumb.length > 0 ? Format.fileUri(root.nextThumb) : ""
            asynchronous: true
            cache: true
            visible: false
        }

        // source rather than sourceComponent, so a file is decoded only while an image is open and its texture goes with the item.
        Loader {
            id: imageLoader
            anchors.top: fileNameBar.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.rightMargin: root.infoInset
            clip: true
            onLoaded: {
                item.path = Qt.binding(function () { return root.path })
                item.zoom = Qt.binding(function () {
                    return root.contentZoom * imagePinch.persistentScale
                })
            }
        }

        PinchHandler {
            id: imagePinch
            enabled: root.isImage && imageLoader.item !== null
            target: null
            minimumScale: 0.5
            maximumScale: 4
            minimumRotation: 0
            maximumRotation: 0
            persistentScale: 1
            onActiveChanged: if (!active) {
                var factor = persistentScale
                root.contentZoom = Math.max(0.5, Math.min(4, root.contentZoom * factor))
                persistentScale = 1
            }
        }

        // The canvas's PdfViewer, source not sourceComponent, so QtQuick.Pdf loads on the first PDF and never for a folder without one.
        Loader {
            id: pdfLoader
            anchors.top: fileNameBar.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.rightMargin: root.infoInset
            clip: true
            onLoaded: {
                item.path = Qt.binding(function () { return root.path })
                item.active = true
                item.forceActiveFocus()
            }
        }

    Connections {
        target: pdfLoader.item
        function onClosed() { root.close() }
        function onNextFileRequested() { root.movePreview(1) }
    }

        // The canvas's Archive tile at Quick Look size: the name, the count the index gave, then the entries.
        Column {
            id: archivePane
            anchors.top: fileNameBar.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.margins: Theme.spacing.rowPaddingX
            anchors.rightMargin: Theme.spacing.rowPaddingX + root.infoInset
            scale: root.contentZoom
            transformOrigin: Item.TopLeft
            clip: true
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
            width: parent.width - 2 * Theme.spacing.rowPaddingX - root.infoInset
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
            anchors.top: fileNameBar.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.rightMargin: root.infoInset
            visible: (root.isMedia || root.isImage || root.isArchive) && root.status === "loading"
        }

        // MediaStrip stays below the filmstrip, so video controls and next/previous file arrows remain separate.
        Flea.MediaStrip {
            id: mediaStrip
            visible: ViewState.previewMediaControls && root.isMedia && (root.kind === "audio" || root.stripShown)
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.rightMargin: root.infoInset
            anchors.bottom: parent.bottom
            framed: false
            playing: root.status === "playing"
            position: root.position
            duration: root.duration
            onToggled: root.togglePlay()
            onSeeked: function (ms) { root.seekTo(ms) }
            onTouched: root.revealStrip()
        }

        Flea.PreviewInfoPanel {
            id: infoPanel
            anchors.top: fileNameBar.bottom
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            width: Math.min(320, Math.max(240, parent.width * 0.36))
            z: 2
            opened: root.infoOpen
            path: root.path
            kind: root.kindName.length > 0 ? root.kindName : root.kind
            size: root.size
            meta: root.infoMeta
            mediaError: root.isMedia && mediaLoader.item ? mediaLoader.item.errorDetail : ""
            onCopyRequested: function (label, value) {
                if (root.pane && root.pane.opener) {
                    root.pane.opener.copyText(value)
                    root.pane.message("Copied to clipboard.", false)
                }
            }
        }

        // Windows-style filmstrip for media previews. It uses cached thumbnails when available
        // and falls back to a bounded image decode from the source file.
        Rectangle {
            id: imageFilmstrip
            visible: root.filmstripShown && root.filmstripItems.length > 1
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            // Keep the transport lane reserved for every file, so moving between media and non-media
            // items does not move the filmstrip.
            anchors.bottomMargin: mediaStrip.implicitHeight
            height: 92
            color: Theme.color.background
            opacity: 0.94
            z: 3

            ListView {
                id: imageFilmstripList
                anchors.verticalCenter: parent.verticalCenter
                anchors.left: imageFilmstripLeft.right
                anchors.right: imageFilmstripRight.left
                height: 64
                orientation: ListView.Horizontal
                spacing: Theme.spacing.gap
                model: root.filmstripItems
                currentIndex: root.imageFilmstripIndex
                highlightRangeMode: ListView.StrictlyEnforceRange
                preferredHighlightBegin: width / 2 - 38
                preferredHighlightEnd: width / 2 + 38
                highlightMoveDuration: 70
                highlightMoveVelocity: 1600
                clip: true
                delegate: Rectangle {
                        width: 76
                        height: 64
                        color: Theme.color.surface
                        border.width: modelData.path === root.path ? 2 : Theme.spacing.hairline
                        border.color: modelData.path === root.path ? Theme.color.accent : Theme.color.muted
                        radius: Style.cornerRadius
                        clip: true

                        Image {
                            anchors.fill: parent
                            anchors.margins: 2
                    visible: root.isImagePath(modelData.path) || modelData.thumb.length > 0
                    source: modelData.thumb.length > 0 ? Format.fileUri(modelData.thumb) : Format.fileUri(modelData.path)
                            sourceSize.width: 72
                            sourceSize.height: 60
                            fillMode: Image.PreserveAspectFit
                            asynchronous: true
                            cache: true
                    }

                    Flea.Glyph {
                        anchors.centerIn: parent
                        visible: !root.isImagePath(modelData.path) && modelData.thumb.length === 0
                        width: 32
                        height: 32
                        name: Kinds.quickLookKind(modelData.icon, modelData.path) === Kinds.AUDIO
                              ? "music"
                              : Kinds.quickLookKind(modelData.icon, modelData.path) === Kinds.VIDEO
                                ? "film"
                                : Kinds.quickLookKind(modelData.icon, modelData.path) === Kinds.PDF
                                  ? "file-pdf" : Icons.glyphFor(modelData.icon)
                        color: Theme.color.muted
                    }

                        MouseArea {
                            anchors.fill: parent
                            onClicked: root.open(modelData.path, modelData.icon, modelData.size)
                        }
                }
            }

            Rectangle {
                id: imageFilmstripLeft
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                width: 68
                color: leftArrowMouse.containsMouse ? Theme.color.surface : Theme.color.background
                z: 5
                opacity: root.imageFilmstripIndex > 0 ? 1 : 0.35

                Text {
                    anchors.centerIn: parent
                    text: "‹"
                    color: Theme.color.foreground
                    font.pixelSize: Theme.font.body * 2
                }

                MouseArea {
                    id: leftArrowMouse
                    anchors.fill: parent
                    enabled: root.imageFilmstripIndex > 0
                    hoverEnabled: true
                    onClicked: root.movePreview(-1)
                }
            }

            Rectangle {
                id: imageFilmstripRight
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                width: 68
                color: rightArrowMouse.containsMouse ? Theme.color.surface : Theme.color.background
                z: 5
                opacity: root.imageFilmstripIndex >= 0 && root.imageFilmstripIndex < root.filmstripItems.length - 1 ? 1 : 0.35

                Text {
                    anchors.centerIn: parent
                    text: "›"
                    color: Theme.color.foreground
                    font.pixelSize: Theme.font.body * 2
                }

                MouseArea {
                    id: rightArrowMouse
                    anchors.fill: parent
                    enabled: root.imageFilmstripIndex >= 0 && root.imageFilmstripIndex < root.filmstripItems.length - 1
                    hoverEnabled: true
                    onClicked: root.movePreview(1)
                }
            }
        }
    }
}
