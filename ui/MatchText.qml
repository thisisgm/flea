import QtQuick

// A name with the search or filter match run marked by an accent wash. Three PlainText runs laid
// out left to right, never StyledText: a filename is arbitrary text and StyledText would render any
// markup found inside it. SearchFilter rule 3: the run keeps the foreground ink and the accent goes
// behind it, because accent against foreground is under 1.5:1 on nine of the 22 installed palettes
// and exactly 1.00 on kanagawa, where inked matches were the dimmest part of the name.
Item {
    id: root

    property string text: ""
    // Where the run starts inside text, -1 for no run; ui/js/Match.js computes both.
    property int matchStart: -1
    property int matchLength: 0
    property color color: Theme.color.foreground
    property color accent: Theme.color.accent
    property int pixelSize: Theme.font.body
    // True while the strip that explains the run is up, so the content builds with it and no keystroke pays for it.
    property bool armed: false
    // Opt-in middle elision, so the match run keeps its place.
    property bool elideMiddle: false

    readonly property bool marked: root.matchStart >= 0 && root.matchLength > 0
    readonly property string before: root.marked ? root.text.substring(0, root.matchStart) : root.text
    readonly property string run: root.marked ? root.text.substring(root.matchStart, root.matchStart + root.matchLength) : ""
    readonly property string after: root.marked ? root.text.substring(root.matchStart + root.matchLength) : ""

    implicitHeight: beforeText.implicitHeight
    // The marked run and its tail exist only while a match is marked, so their widths are the loaded item's.
    implicitWidth: beforeText.implicitWidth + (markLoader.item ? markLoader.item.runsImplicitWidth : 0)

    Text {
        id: beforeText
        anchors.verticalCenter: parent.verticalCenter
        // Each run takes what is left of the slot, so the tail elides and nothing ever overflows.
        width: Math.min(implicitWidth, root.width)
        // A run clamped to zero paints nothing: an elided Text at width 0 still draws its full
        // text on its first layout, so a late match (a head that fills the slot) overpaints
        // whatever follows until any relayout. The accent wash below already gates the same way.
        visible: width > 0
        text: root.before
        color: root.color
        font.family: Theme.font.family
        font.pixelSize: root.pixelSize
        elide: root.elideMiddle && !root.marked ? Text.ElideMiddle : Text.ElideRight
        textFormat: Text.PlainText
        maximumLineCount: 1
    }

    // Built only while a match is marked: an unmarked name is the one run above, and most rows are unmarked.
    Loader {
        id: markLoader
        active: root.marked || root.armed
        anchors.left: beforeText.right
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: Math.max(0, root.width - beforeText.width)
        sourceComponent: Item {
            readonly property real runsImplicitWidth: runText.implicitWidth + afterText.implicitWidth

            Rectangle {
                anchors.fill: runText
                visible: runText.width > 0
                // The accent carries the row dim, so the wash multiplies it instead of replacing it.
                color: Qt.rgba(root.accent.r, root.accent.g, root.accent.b, root.accent.a * Theme.washActive)
            }

            Text {
                id: runText
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, parent.width)
                visible: width > 0
                text: root.run
                color: root.color
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
                width: Math.max(0, parent.width - runText.width)
                visible: width > 0
                text: root.after
                color: root.color
                font.family: Theme.font.family
                font.pixelSize: root.pixelSize
                elide: Text.ElideRight
                textFormat: Text.PlainText
                maximumLineCount: 1
            }
        }
    }
}
