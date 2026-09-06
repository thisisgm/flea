import QtQuick

// The caption-type table under every preview state's frame, one label and value pair a row. Every
// value starts at the same x in all twelve states, which is what makes them read as one anatomy.
Column {
    id: root

    // { label, value } pairs, as ui/js/Facts.js facts and multiFacts build them.
    property var rows: []

    spacing: Theme.spacing.hairline * 3

    // The widest label the twelve states use, by advanceWidth because width drops the trailing gutter space.
    readonly property int labelWidth: labelMetrics.advanceWidth

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
            height: Math.round(Theme.font.caption * 1.5)

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

            Text {
                anchors.left: factLabel.right
                anchors.right: parent.right
                text: modelData.value
                color: Theme.color.foreground
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                textFormat: Text.PlainText
                elide: Text.ElideRight
            }
        }
    }
}
