import QtQuick
import qs.Commons
import "." as Flea
import "js/Format.js" as Format

Rectangle {
    id: root

    property bool opened: false
    property string path: ""
    property string kind: ""
    property int size: 0
    property var meta: null
    property string mediaError: ""
    signal copyRequested(string label, string value)
    color: Theme.color.surface
    border.width: 0
    visible: root.opened

    function fileName() {
        var cut = root.path.lastIndexOf("/")
        return cut >= 0 ? root.path.substring(cut + 1) : root.path
    }

    function rows() {
        var m = root.meta || ({})
        var out = [
            {label: "Name", value: root.fileName()},
            {label: "Path", value: root.path},
            {label: "Type", value: root.kind},
            {label: "Size", value: root.size > 0 ? Format.size(root.size) : ""}
        ]
        if (m.w > 0 && m.h > 0) out.push({label: "Resolution", value: m.w + " × " + m.h})
        if (m.ms > 0) out.push({label: "Duration", value: Format.duration(m.ms)})
        if (m.rate > 0) out.push({label: "Sample rate", value: Format.sampleRate(m.rate)})
        if (m.fps > 0) out.push({label: "Frame rate", value: Format.frameRate(m.fps)})
        if (m.bitrate > 0) out.push({label: "Bitrate", value: Format.bitrate(m.bitrate)})
        if (root.mediaError.length > 0) out.push({label: "Error", value: root.mediaError})
        if (m.owner && m.owner.length > 0) out.push({label: "Owner", value: m.owner})
        if (m.target && m.target.length > 0) out.push({label: "Target", value: m.target})
        return out.filter(function (row) { return row.value.length > 0 })
    }

    Rectangle {
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: Theme.spacing.hairline
        color: Theme.color.muted
        opacity: 0.7
    }

    Column {
        anchors.fill: parent
        anchors.margins: Theme.spacing.rowPaddingX
        spacing: Theme.spacing.gap

        Flickable {
            id: infoFlick
            anchors.fill: parent
            contentWidth: width
            contentHeight: infoContent.implicitHeight
            clip: true
            Flea.ViewportScrollBars { parent: infoFlick; flickable: infoFlick }

            Column {
                id: infoContent
                width: parent.width
                spacing: Theme.spacing.gap

                Flea.FactsTable {
                    id: facts
                    width: parent.width
                    labelWidthOverride: 96
                    wrapValues: true
                    rows: root.rows()
                    onCopyRequested: function (label, value) { root.copyRequested(label, value) }
                }
            }
        }
    }
}
