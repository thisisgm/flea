import QtQuick
import QtMultimedia
import "." as Flea
import "js/Format.js" as Format

// The only file in the tree importing QtMultimedia: reached only through Preview.qml's Loader, on demand.
Item {
    id: root

    property string path: ""
    property string kind: "audio"
    property int size: 0
    // The Quick Look starts playing on open, which is its whole job. The preview column does not:
    // arrowing down a folder of clips must not start any of them.
    property bool autoStart: true
    property alias muted: audio.muted

    // The same name PreviewPdf gives its own unreadable state, so both readers test one property.
    readonly property bool failed: player.error !== MediaPlayer.NoError

    readonly property string status: {
        if (root.failed) return "This file could not be played."
        if (player.playbackState === MediaPlayer.PlayingState) return "playing"
        if (player.playbackState === MediaPlayer.PausedState) return "paused"
        if (player.mediaStatus === MediaPlayer.LoadingMedia || player.mediaStatus === MediaPlayer.BufferingMedia) return "loading"
        return "stopped"
    }

    // The canvas draws a stopped video behind its own play mark. Gated on mediaStatus rather than
    // playbackState, because playbackState reads Stopped from construction onward and the mark
    // would flash over every clip in the frames before its first one decodes.
    readonly property bool poster: root.kind === "video" && !root.failed
        && player.playbackState !== MediaPlayer.PlayingState
        && (player.mediaStatus === MediaPlayer.LoadedMedia
            || player.mediaStatus === MediaPlayer.BufferedMedia
            || player.mediaStatus === MediaPlayer.EndOfMedia)

    // The canvas's second line under an audio name is "31 MB · flac · 44.1 kHz". Qt carries no
    // sample-rate key at all (QMediaMetaData::Key, Qt 6.11) and the backend probe's own rate is not
    // routed to this overlay, so this states the size and the format and stops at two parts.
    readonly property string facts: {
        var name = root.path.substring(root.path.lastIndexOf("/") + 1)
        var dot = name.lastIndexOf(".")
        var suffix = dot > 0 ? name.substring(dot + 1).toLowerCase() : ""
        var bytes = root.size > 0 ? Format.size(root.size) : ""
        if (bytes.length === 0) return suffix
        return suffix.length === 0 ? bytes : bytes + " · " + suffix
    }

    readonly property alias position: player.position
    readonly property alias duration: player.duration

    // Preview.qml's own strip drives these two; kept here, not there, so Preview.qml never has to
    // import QtMultimedia itself, the isolation the file comment above exists to protect.
    function togglePlay() {
        if (player.playbackState === MediaPlayer.PlayingState) player.pause()
        else player.play()
    }

    function seekTo(ms) {
        player.position = Math.max(0, Math.min(player.duration, ms))
    }

    MediaPlayer {
        id: player
        source: root.path === "" ? "" : Format.fileUri(root.path)
        autoPlay: root.autoStart
        audioOutput: AudioOutput { id: audio }
        videoOutput: video
    }

    // A clip almost never matches the frame, so the letterbox bars are the canvas's own darker
    // ground rather than the surface the rest of the overlay sits on.
    Rectangle {
        anchors.fill: parent
        visible: root.kind === "video"
        color: Theme.color.background
    }

    VideoOutput {
        id: video
        anchors.fill: parent
        visible: root.kind === "video"
    }

    Flea.Glyph {
        anchors.centerIn: parent
        width: Theme.markSize
        height: Theme.markSize
        visible: root.poster
        name: "play"
        color: Theme.color.muted
        opacity: 0.7
    }

    // Nothing else to look at for audio, so the kind's mark and the filename stand in for the missing
    // picture; the strip Preview.qml draws over this carries the transport and the clock. The canvas
    // sets the mark beside a left-aligned two-line stack, not above a centred one.
    Row {
        id: audioPane
        anchors.centerIn: parent
        spacing: Theme.spacing.rowPaddingX
        visible: root.kind === "audio" && !root.failed

        Flea.Glyph {
            anchors.verticalCenter: parent.verticalCenter
            width: Theme.markSize
            height: Theme.markSize
            name: "music"
            color: Theme.color.muted
            opacity: 0.7
        }

        Column {
            anchors.verticalCenter: parent.verticalCenter
            // Whatever the pane's own padding leaves beside the mark, so a long name elides here
            // instead of pushing the mark off the surface.
            width: Math.max(0, root.width - 2 * Theme.spacing.rowPaddingX
                               - Theme.markSize - audioPane.spacing)
            spacing: 0

            // corner: a filename is arbitrary text, so PlainText, the same rule every name on this surface follows.
            Text {
                width: parent.width
                text: root.path.substring(root.path.lastIndexOf("/") + 1)
                color: Theme.color.foreground
                font.family: Theme.font.family
                font.pixelSize: Theme.font.body
                textFormat: Text.PlainText
                elide: Text.ElideRight
            }

            Text {
                width: parent.width
                visible: root.facts.length > 0
                text: root.facts
                color: Theme.color.muted
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                textFormat: Text.PlainText
                elide: Text.ElideRight
            }
        }
    }

    // Nothing drew this before, so an unplayable file left the overlay blank with no sentence at all.
    Column {
        anchors.centerIn: parent
        width: parent.width - 2 * Theme.spacing.rowPaddingX
        spacing: Theme.spacing.gap
        visible: root.failed

        Flea.Glyph {
            anchors.horizontalCenter: parent.horizontalCenter
            width: Theme.markSize
            height: Theme.markSize
            name: "alert"
            color: Theme.color.error
        }

        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: root.status
            color: Theme.color.foreground
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }
    }
}
