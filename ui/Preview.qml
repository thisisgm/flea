import QtQuick
import qs.Commons
import "." as Flea
import "js/Facts.js" as Facts
import "js/Kinds.js" as Kinds
import "js/Motion.js" as Motion

// The overlay lives inside the Flea window (Finder's Quick Look shape); a second window breaks
// omarchy-drive focus flea and every test that narrows on it.
Item {
    id: root
    anchors.fill: parent
    // active flips instantly (open()/close() below), so previewOpen()'s IPC read never races the
    // close fade; visible only stays true a little longer, until surface's own opacity finishes it.
    visible: root.active || surface.opacity > 0
    z: 1

    property bool active: false
    // shell.qml wires the pane in: the archive pane asks the backend about the cursor row and nothing else here reads it.
    property var pane: null
    property string path: ""
    property string iconName: ""
    property int size: 0
    property string kind: ""
    readonly property bool isMedia: root.kind === "audio" || root.kind === "video"
    readonly property bool isPdf: root.kind === "pdf"
    readonly property bool isImage: root.kind === "image"
    readonly property bool isArchive: root.kind === "archive"
    // The backend's meta answer for the open archive, null until it lands; archiveRow is the row it was asked for.
    property var archiveMeta: null
    property int archiveRow: -1
    readonly property bool archiveFailed: root.isArchive && root.archiveMeta !== null && root.archiveMeta.archiveFailed === true
    readonly property bool pdfExpanded: root.isPdf && pdfLoader.item !== null && pdfLoader.item.expanded
    // The PDF surface itself, null when no document is loaded: ui/Ipc.qml's zoom and expand
    // readers answer "" for that, so an unmeasured state can never read as a real value.
    readonly property var pdfItem: pdfLoader.item
    // What the strip actually draws, the same "not just the lookup" idiom Row.qml's iconUrl uses;
    // shell.qml's IPC reads this instead of re-deriving the visible: expression a second time.
    readonly property alias stripVisible: mediaStrip.visible
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
    property int pendingSize: 0
    // The settle idiom Pane's own thumbnail request reuses: a held j/k costs zero reloads until the cursor rests.
    readonly property int followSettleMs: 120

    // Read through to PreviewMedia so this file never has to import QtMultimedia itself; 0 before
    // the loader has produced an item, same shape root.status already uses.
    readonly property int position: (root.isMedia && mediaLoader.item) ? mediaLoader.item.position : 0
    readonly property int duration: (root.isMedia && mediaLoader.item) ? mediaLoader.item.duration : 0

    // Task 22: the strip is shown on open, hidden stripHideMs after the last reveal, video only
    // (audio has nothing else to look at, so its strip never hides; see the strip's own visible:).
    property bool stripShown: true
    // Matches StatusBar.messageMs, the OEM's own transient interval; Sidebar's unmount arm reuses
    // the same number for the same reason, see AGENTS.md "Right click arms, it does not fire".
    readonly property int stripHideMs: 4000

    function revealStrip() {
        root.stripShown = true
        stripHideTimer.restart()
    }

    function togglePlay() {
        if (root.isMedia && mediaLoader.item)
            mediaLoader.item.togglePlay()
    }

    // Absolute seek in ms, clamped by PreviewMedia's own seekTo; the slider's onReleased calls this directly.
    function seekTo(ms) {
        if (root.isMedia && mediaLoader.item)
            mediaLoader.item.seekTo(ms)
    }

    // Relative seek in ms, Left/Right's own shape; seekTo does the clamping.
    function seek(deltaMs) {
        root.seekTo(root.position + deltaMs)
    }

    // The PDF viewer's own three actions, reached the way the media transport's already are: through
    // this file, so ui/js/Focus.js never has to know a Loader item is what answers.
    function turnPage(delta) {
        if (root.isPdf && pdfLoader.item)
            pdfLoader.item.turn(delta)
    }

    function zoomBy(steps) {
        if (root.isPdf && pdfLoader.item)
            pdfLoader.item.zoomBy(steps)
    }

    function toggleExpand() {
        if (root.isPdf && pdfLoader.item)
            pdfLoader.item.toggleExpand()
    }

    // Space opens on the cursor row; this is immediate, follow() below is the held-key j/k path.
    function open(newPath, newIcon, newSize) {
        followSettle.stop()
        root.load(newPath, newIcon, newSize)
    }

    function follow(newPath, newIcon, newSize) {
        root.pendingPath = newPath
        root.pendingIcon = newIcon
        root.pendingSize = newSize
        followSettle.restart()
    }

    // Dropping the loader's source is what stops playback: media dies with the loader.
    function close() {
        followSettle.stop()
        stripHideTimer.stop()
        root.active = false
        root.kind = ""
        mediaLoader.source = ""
        pdfLoader.source = ""
        imageLoader.source = ""
        root.archiveMeta = null
        root.archiveRow = -1
    }

    function load(newPath, newIcon, newSize) {
        root.path = newPath
        root.iconName = newIcon
        root.size = newSize
        root.kind = Kinds.quickLookKind(newIcon, newPath)
        root.active = true
        mediaLoader.source = root.isMedia ? "PreviewMedia.qml" : ""
        pdfLoader.source = root.isPdf ? "PdfViewer.qml" : ""
        imageLoader.source = root.isImage ? "PreviewImage.qml" : ""
        root.askArchive()
        root.revealStrip()
    }

    // One row, only while an archive is the thing open: the same no-sweep rule the column follows.
    function askArchive() {
        root.archiveMeta = null
        root.archiveRow = root.isArchive && root.pane ? root.pane.cursorIndex : -1
        if (root.archiveRow >= 0)
            root.pane.backend.askMeta(root.archiveRow, false, false, true)
    }

    Connections {
        target: root.pane ? root.pane.backend : null
        function onMeta(row, w, h, durationMs, sampleRate, entries, unpacked, archiveFailed, names, lines, partial, linesFailed, target, targetDir, owner) {
            if (root.isArchive && row === root.archiveRow)
                root.archiveMeta = { entries: entries, unpacked: unpacked, archiveFailed: archiveFailed, names: names }
        }
    }

    // A meta asked across a listing change is answered with silence, so the rows landing re-asks it, the way ui/ColumnsArea.qml does.
    Connections {
        target: root.pane
        function onRowsChanged() { if (root.active && root.isArchive && root.archiveMeta === null) root.askArchive() }
    }

    Timer {
        id: followSettle
        interval: root.followSettleMs
        repeat: false
        onTriggered: root.load(root.pendingPath, root.pendingIcon, root.pendingSize)
    }

    Timer {
        id: stripHideTimer
        interval: root.stripHideMs
        repeat: false
        onTriggered: root.stripShown = false
    }

    MouseArea {
        anchors.fill: parent
        // root.visible outlives root.active by the whole close fade, and a shield that outlives the
        // overlay swallows the first click after it and freezes row hover for that window too.
        enabled: root.active
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        hoverEnabled: true
        // A click behind the overlay would otherwise silently move the cursor, or open the menu on it.
        onClicked: {}
        onPositionChanged: root.revealStrip()
    }

    Rectangle {
        id: surface
        anchors.centerIn: parent
        // Open rises into place; close does not translate (enabled: root.active), only fades,
        // faster than the open animation. root.active itself already flipped above, synchronously.
        anchors.verticalCenterOffset: root.active ? 0 : Motion.translateUpPx
        opacity: root.active ? 1 : 0
        // Expand drops the Quick Look inset, which is the whole of the canvas's "expand fills the window".
        readonly property real inset: root.pdfExpanded ? 1 : Theme.preview.fraction
        width: parent.width * surface.inset
        height: parent.height * surface.inset
        color: Theme.color.surface
        // Mirrors hyprland decoration:rounding; media fills the surface and keeps square corners, a visible corner only shows on text and audio panes.
        radius: Style.cornerRadius

        Behavior on anchors.verticalCenterOffset {
            enabled: root.active && !Motion.reduced
            NumberAnimation { duration: Motion.durMs.open; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.bezierCurve }
        }

        Behavior on opacity {
            enabled: !Motion.reduced
            NumberAnimation {
                duration: root.active ? Motion.durMs.open : Motion.durMs.close
                easing.type: Easing.BezierSpline
                easing.bezierCurve: Motion.bezierCurve
            }
        }

        Flea.PreviewText {
            id: textPane
            anchors.fill: parent
            anchors.margins: Theme.spacing.gap
            active: root.kind === "text"
            path: root.path
            size: root.size
        }

        Loader {
            id: mediaLoader
            anchors.fill: parent
            onLoaded: {
                item.path = Qt.binding(function () { return root.path })
                item.kind = Qt.binding(function () { return root.kind })
                item.size = Qt.binding(function () { return root.size })
            }
        }

        // The image pane, source rather than sourceComponent like the two beside it, so the file is
        // decoded only while an image is open and its texture goes with the item on close.
        Loader {
            id: imageLoader
            anchors.fill: parent
            onLoaded: item.path = Qt.binding(function () { return root.path })
        }

        // The canvas's PdfViewer, which draws its own chrome. source, not sourceComponent, so
        // QtQuick.Pdf loads on the first PDF opened and never for a folder without one.
        Loader {
            id: pdfLoader
            anchors.fill: parent
            onLoaded: {
                item.path = Qt.binding(function () { return root.path })
                item.active = true
            }
        }

        Connections {
            target: pdfLoader.item
            function onClosed() { root.close() }
        }

        // The canvas's Archive tile at Quick Look size: the name, the count the index gave, then the entries.
        Column {
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
                font.pixelSize: Theme.font.bodySmall
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
                font.pixelSize: Theme.font.bodySmall
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }
        }

        // Media still buffering or an image still decoding shows the crawl; LoadingState's hold-off keeps a fast local open from flashing it.
        Flea.LoadingState {
            anchors.fill: parent
            visible: (root.isMedia || root.isImage || root.isArchive) && root.status === "loading"
        }

        // Task 22's transport strip, MediaStrip unframed: quiet over the video and permanent on
        // audio (nothing else there to look at). The column draws the framed form of the same file.
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
}
