import QtQuick
import qs.Commons
import "js/GitGraph.js" as GitGraph

// One commit row in the git graph panel: refs, short hash, subject, date. Lane ink comes from GitGraph.
Item {
    id: root

    property var commit: ({})
    property real lanePad: 0

    readonly property real rowH: Theme.fileRowHeight
    height: root.rowH
    width: parent ? parent.width : 0

    Row {
        anchors.fill: parent
        anchors.leftMargin: root.lanePad
        anchors.rightMargin: Theme.spacing.rowPaddingX
        spacing: Theme.spacing.gap

        // Ref pills, one per decorated name on this commit.
        Repeater {
            model: root.commit && root.commit.refs ? root.commit.refs : []
            delegate: Rectangle {
                required property string modelData
                height: Math.round(Theme.font.caption * 1.6)
                width: refText.implicitWidth + Theme.spacing.gap
                radius: Math.max(2, Math.round(Style.cornerRadius / 2))
                color: GitGraph.laneColor(root.commit.lane || 0)
                anchors.verticalCenter: parent.verticalCenter
                Text {
                    id: refText
                    anchors.centerIn: parent
                    text: parent.modelData
                    color: Theme.color.background
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    textFormat: Text.PlainText
                }
            }
        }

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: root.commit && root.commit.short ? root.commit.short : ""
            color: root.commit && root.commit.is_head ? Theme.color.accent : Theme.color.muted
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            textFormat: Text.PlainText
        }

        Text {
            anchors.verticalCenter: parent.verticalCenter
            width: Math.max(0, parent.width - x - dateText.width - Theme.spacing.gap)
            text: root.commit && root.commit.subject ? root.commit.subject : ""
            color: Theme.color.foreground
            font.family: Theme.font.family
            font.pixelSize: Theme.font.bodySmall
            elide: Text.ElideRight
            textFormat: Text.PlainText
        }

        Text {
            id: dateText
            anchors.verticalCenter: parent.verticalCenter
            text: GitGraph.shortDate(root.commit && root.commit.date ? root.commit.date : "")
            color: Theme.color.muted
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            textFormat: Text.PlainText
        }
    }

    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: Theme.spacing.hairline
        color: Theme.color.foreground
        opacity: 0.08
    }
}
