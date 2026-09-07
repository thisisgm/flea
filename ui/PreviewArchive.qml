import QtQuick
import qs.Commons
import "." as Flea
import "js/Facts.js" as Facts

// The Archive tile's frame: the entries the wire carried, as many as the frame has room for, and the
// count it could not name. One row per line box, less the "+ N more" line, which always has to be
// visible: a list that ran past the frame would hide the very number it exists to state.
Item {
    id: root

    // The meta answer for the archive row, or null before one has arrived.
    property var meta: null

    readonly property real lineHeight: Math.max(Theme.bodyLineHeight, Math.round(Theme.font.body * Theme.lineBoxRatio))
    readonly property int shown: Math.max(0, Math.floor(height / lineHeight) - 1)

    Column {
        anchors.fill: parent
        clip: true
        spacing: 0

        Repeater {
            model: Facts.archiveEntries(root.meta, root.shown)

            delegate: Row {
                required property var modelData
                width: root.width
                height: root.lineHeight
                spacing: Theme.spacing.gap

                Flea.Glyph {
                    anchors.verticalCenter: parent.verticalCenter
                    width: Theme.font.caption
                    height: Theme.font.caption
                    name: modelData.d ? "folder" : "file"
                    color: Theme.color.muted
                }

                // corner: an archive holds arbitrary names, so PlainText, the same rule every name follows.
                Text {
                    text: modelData.n
                    color: Theme.color.muted
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.body
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                }
            }
        }

        Text {
            height: root.lineHeight
            visible: Facts.archiveMore(root.meta, root.shown) > 0
            text: "+ " + Facts.archiveMore(root.meta, root.shown) + " more"
            color: Theme.color.muted
            opacity: 0.6
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            textFormat: Text.PlainText
        }
    }
}
