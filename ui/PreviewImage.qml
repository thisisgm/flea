import QtQuick
import qs.Commons
import "." as Flea
import "js/Format.js" as Format

// The Quick Look's image pane, reached only through Preview.qml's Loader. It draws the file itself:
// the 256 px cache file the column draws would be an eightfold upscale on a surface this size.
Item {
    id: root

    property string path: ""

    // The same name the media and PDF panes give their unreadable state, so Preview.qml tests one property.
    readonly property bool failed: picture.status === Image.Error
    // Every state is terminal: a decode ends Ready or Error, and a vanished file ends Error too.
    readonly property string status: {
        if (root.failed) return "This image could not be read."
        return picture.status === Image.Ready ? "image" : "loading"
    }
    readonly property string name: root.path.substring(root.path.lastIndexOf("/") + 1)

    // The letterbox ground the video pane uses: a photo almost never matches the surface's proportions.
    Rectangle {
        anchors.fill: parent
        color: Theme.color.background
    }

    Image {
        id: picture
        anchors.fill: parent
        visible: picture.status === Image.Ready
        // Format.fileUri, not a concatenation: a # or a ? in the name would truncate a hand-built URI.
        source: root.path.length > 0 ? Format.fileUri(root.path) : ""
        fillMode: Image.PreserveAspectFit
        asynchronous: true
        cache: false
        // Decoded no larger than the surface: the same 6016x3900 PNG is 94 MB of texture at full size
        // and 8 MB bound to this box's 2099x1156 surface, measured, for 17 ms more decode.
        // corner: a zero here means unbounded to Qt, so the floor is 1 and never 0.
        sourceSize.width: Math.max(1, Math.round(root.width))
        sourceSize.height: Math.max(1, Math.round(root.height))
    }

    // A failed decode is a mark and a sentence, never a bare ground: the blank-frame class again otherwise.
    Column {
        anchors.centerIn: parent
        width: parent.width - 2 * Theme.spacing.rowPaddingX
        spacing: Theme.spacing.gap
        visible: root.failed

        Flea.Glyph {
            anchors.horizontalCenter: parent.horizontalCenter
            // A failure mark stands alone, so it takes the pane-state ceiling States.dc.html draws at 40.
            maxSize: Theme.stateMarkSize
            width: Theme.stateMarkSize
            height: Theme.stateMarkSize
            name: "alert"
            color: Theme.color.error
        }

        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: root.status
            color: Theme.color.foreground
            font.family: Theme.font.family
            font.pixelSize: Theme.font.body
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }

        // corner: a filename is arbitrary text, so PlainText, the same rule every name on this surface follows.
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: root.name
            color: Theme.color.muted
            font.family: Theme.font.family
            font.pixelSize: Theme.font.body
            textFormat: Text.PlainText
            elide: Text.ElideMiddle
        }
    }
}
