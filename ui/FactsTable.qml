import QtQuick
import "." as Flea

// The caption-type table under every preview state's frame, one label and value pair a row. Every
// value starts at the same x in all twelve states, which is what makes them read as one anatomy.
Column {
    id: root

    // { label, value } pairs, as ui/js/Facts.js facts and multiFacts build them.
    property var rows: []
    property int labelWidthOverride: -1
    property bool wrapValues: false
    signal copyRequested(string label, string value)

    spacing: Theme.spacing.hairline * 3

    // The widest label the twelve states use, by advanceWidth because width drops the trailing gutter space.
    readonly property int labelWidth: root.labelWidthOverride >= 0 ? root.labelWidthOverride : labelMetrics.advanceWidth

    TextMetrics {
        id: labelMetrics
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        text: "Points at "
    }

    Repeater {
        model: root.rows

        delegate: Item {
            required property var modelData
            width: root.width
            height: root.wrapValues && (modelData.label === "Name" || modelData.label === "Path")
                    ? Math.max(Math.round(Theme.font.caption * 1.5), value.implicitHeight)
                    : Math.round(Theme.font.caption * 1.5)

            HoverHandler { id: rowHover }

            Text {
                id: factLabel
                anchors.left: parent.left
                width: root.labelWidth
                text: modelData.label
                color: Theme.color.muted
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                textFormat: Text.PlainText
            }

            Item {
                id: valueArea
                anchors.left: factLabel.right
                anchors.right: parent.right
                height: parent.height

                Text {
                    id: value
                    anchors.left: parent.left
                    anchors.top: parent.top
                    width: root.wrapValues && (modelData.label === "Name" || modelData.label === "Path")
                           ? Math.max(0, parent.width - copyButton.width - Theme.spacing.gap)
                           : Math.min(implicitWidth, Math.max(0, parent.width - copyButton.width - Theme.spacing.gap))
                    text: modelData.value
                    color: Theme.color.foreground
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    textFormat: Text.PlainText
                    elide: root.wrapValues && (modelData.label === "Name" || modelData.label === "Path") ? Text.ElideNone : Text.ElideRight
                    wrapMode: root.wrapValues && (modelData.label === "Name" || modelData.label === "Path") ? Text.Wrap : Text.NoWrap
                    height: root.wrapValues ? implicitHeight : parent.height
                }

                Flea.ChromeButton {
                id: copyButton
                anchors.left: value.right
                anchors.leftMargin: Theme.spacing.gap
                anchors.verticalCenter: parent.verticalCenter
                width: Theme.hitMin
                height: Theme.hitMin
                visible: rowHover.hovered && modelData.value.length > 0
                glyph: "copy"
                accessName: "Copy " + modelData.label
                restingColor: Theme.color.muted
                onActivated: root.copyRequested(modelData.label, modelData.value)
                }
            }
        }
    }
}
