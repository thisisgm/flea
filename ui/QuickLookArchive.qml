import QtQuick
import qs.Commons
import "." as Flea
import "js/Facts.js" as Facts

// The canvas's Archive tile at Quick Look size: the name, the count the index gave, then the entries.
Column {
    id: root
    spacing: Theme.spacing.gap
    property string path: ""
    property var meta: null

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
        text: Facts.archiveLine(root.meta)
        color: Theme.color.muted
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
    }

    Flea.PreviewArchive {
        width: parent.width
        height: parent.height - y
        meta: root.meta
    }
}
