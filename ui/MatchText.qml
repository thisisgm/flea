import QtQuick

// A name with the search or filter match run in the accent role. Three PlainText runs laid out
// left to right, never StyledText: a filename is arbitrary text and StyledText would render any
// markup found inside it.
Item {
    id: root

    property string text: ""
    // Where the run starts inside text, -1 for no run; ui/js/Match.js computes both.
    property int matchStart: -1
    property int matchLength: 0
    property color color: Theme.color.foreground
    property color accent: Theme.color.accent
    property int pixelSize: Theme.font.body

    readonly property bool marked: root.matchStart >= 0 && root.matchLength > 0
    readonly property string before: root.marked ? root.text.substring(0, root.matchStart) : root.text
    readonly property string run: root.marked ? root.text.substring(root.matchStart, root.matchStart + root.matchLength) : ""
    readonly property string after: root.marked ? root.text.substring(root.matchStart + root.matchLength) : ""

    implicitHeight: beforeText.implicitHeight
    implicitWidth: beforeText.implicitWidth + runText.implicitWidth + afterText.implicitWidth
    clip: true

    Text {
        id: beforeText
        anchors.verticalCenter: parent.verticalCenter
        // Each run takes what is left of the slot, so the tail elides and nothing ever overflows.
        width: Math.min(implicitWidth, root.width)
        text: root.before
        color: root.color
        font.family: Theme.font.family
        font.pixelSize: root.pixelSize
        elide: Text.ElideRight
        textFormat: Text.PlainText
        maximumLineCount: 1
    }

    Text {
        id: runText
        anchors.left: beforeText.right
        anchors.verticalCenter: parent.verticalCenter
        width: Math.min(implicitWidth, Math.max(0, root.width - beforeText.width))
        text: root.run
        color: root.accent
        font.family: Theme.font.family
        font.pixelSize: root.pixelSize
        elide: Text.ElideRight
        textFormat: Text.PlainText
        maximumLineCount: 1
    }

    Text {
        id: afterText
        anchors.left: runText.right
        anchors.verticalCenter: parent.verticalCenter
        width: Math.max(0, root.width - beforeText.width - runText.width)
        text: root.after
        color: root.color
        font.family: Theme.font.family
        font.pixelSize: root.pixelSize
        elide: Text.ElideRight
        textFormat: Text.PlainText
        maximumLineCount: 1
    }
}
