import QtQuick
import Quickshell.Io

// Text previews: FileView reads the whole file, so a row over the gate is refused, not truncated.
Item {
    id: root

    property bool active: false
    property string path: ""
    property int size: 0

    // FileView reads the whole file into memory, so this is the largest read a preview will start.
    readonly property int maxBytes: 1048576
    readonly property bool tooLarge: root.size > root.maxBytes
    property bool readFailed: false

    readonly property string status: {
        if (root.tooLarge) return "This file is too large to preview."
        if (root.readFailed) return "This file could not be read."
        return file.loaded ? "ready" : "loading"
    }

    visible: root.active

    FileView {
        id: file
        path: (root.active && !root.tooLarge) ? root.path : ""
        printErrors: false
        onLoadFailed: root.readFailed = true
        onPathChanged: root.readFailed = false
    }

    Flickable {
        id: textFlick
        anchors.fill: parent
        clip: true
        contentWidth: width
        contentHeight: Math.max(height, body.implicitHeight)
        visible: !root.tooLarge && !root.readFailed

        FastScrollHandler {
            parent: textFlick
            flickable: textFlick
        }

        Text {
            id: body
            width: parent.width
            text: file.text()
            // MarkdownText resolves inline image references, so a downloaded README would fetch from
            // the network on cursor movement; the canvas asks for the file verbatim in any case.
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            color: Theme.color.foreground
            font.family: Theme.font.family
            font.pixelSize: Theme.font.bodySmall
        }
    }

    Text {
        anchors.centerIn: parent
        visible: root.tooLarge || root.readFailed
        text: root.status
        color: Theme.color.muted
        font.family: Theme.font.family
        font.pixelSize: Theme.font.bodySmall
        textFormat: Text.PlainText
    }
}
