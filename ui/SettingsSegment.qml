import QtQuick
import qs.Commons

// The Display board's two-value control: both names side by side with the accent on the current one,
// rather than the chevron walk a longer list needs. ui/SettingsRow.qml draws it for any row whose
// model names its options, and the row above owns the writer, so nothing here holds state.
Row {
    id: root

    property var options: []
    property string value: ""

    // The option a pointer chose, which is never the one already showing.
    signal picked(int index)

    spacing: Theme.spacing.hairline * 2

    Repeater {
        model: root.options

        delegate: Rectangle {
            id: segment
            required property var modelData
            required property int index

            readonly property bool current: segment.modelData === root.value

            // The WCAG floor the Blueprint sets for a compact control, which this row has room for.
            width: name.implicitWidth + 2 * Theme.spacing.gap
            height: Theme.hitMin
            // selectedAccentFill already carries the theme's own selected alpha, as ui/MenuRow.qml has it.
            color: segment.current ? Style.selectedAccentFill : "transparent"

            Text {
                id: name
                anchors.centerIn: parent
                text: segment.modelData
                color: segment.current ? Theme.color.accent : Theme.color.foreground
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                textFormat: Text.PlainText
            }

            HoverHandler {
                cursorShape: Qt.PointingHandCursor
            }

            TapHandler {
                acceptedButtons: Qt.LeftButton
                onTapped: if (!segment.current) root.picked(segment.index)
            }
        }
    }
}
