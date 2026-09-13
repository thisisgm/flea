import QtQuick
import qs.Commons
import "." as Flea
import "js/Settings.js" as Settings

// The settings panel's section rail, drawn the way the window's own sidebar draws its rows: the
// accent fill and the inset bar on the chosen one, the rail row height, and the rail icon slot.
// Split out of ui/SettingsPanel.qml, which owns the panel's keyboard and its two-sided cursor.
Flickable {
    id: root

    property string section: ""
    // True while Tab has given the panel's cursor to the rail, which is what lights the row.
    property bool focused: false
    readonly property real focusOpacity: 0.08
    implicitHeight: railColumn.implicitHeight
    contentHeight: railColumn.implicitHeight
    contentWidth: width
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    onSectionChanged: showCurrent()
    onHeightChanged: showCurrent()
    onFocusedChanged: if (focused) showCurrent()
    Component.onCompleted: showCurrent()

    function showCurrent() {
        var item = itemFor(root.section)
        if (!item) return
        if (item.y < contentY) contentY = item.y
        else if (item.y + item.height > contentY + height)
            contentY = Math.max(0, item.y + item.height - height)
    }

    Flea.FastScrollHandler {
        parent: root
        flickable: root
    }

    signal chosen(string id)

    // The row for a section id, so a driven click can land on it: tests/ui.sh clickthrough.
    function itemFor(id) {
        for (var i = 0; i < rows.count; i++) {
            var row = rows.itemAt(i)
            if (row && row.modelData.id === id)
                return row
        }
        return null
    }

    Column {
        id: railColumn
        width: root.width

        Repeater {
            id: rows
            model: Settings.SECTIONS

            delegate: Item {
                id: railRow
                required property var modelData
                width: root.width
                height: Theme.railRowHeight

                readonly property bool current: root.section === railRow.modelData.id

                Rectangle {
                    anchors.fill: parent
                    visible: railRow.current
                    color: Style.selectedAccentFill
                }

                // The inset bar the window's rail draws down the left edge of its own selected row.
                Rectangle {
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    visible: railRow.current
                    width: Theme.spacing.hairline * 2
                    color: Theme.color.accent
                }

                Rectangle {
                    anchors.fill: parent
                    color: Theme.color.foreground
                    opacity: root.focused && railRow.current ? root.focusOpacity : 0
                }

                Flea.Glyph {
                    id: mark
                    anchors.left: parent.left
                    anchors.leftMargin: Theme.spacing.rowPaddingX
                    anchors.verticalCenter: parent.verticalCenter
                    width: Theme.railIconSize
                    height: Theme.railIconSize
                    maxSize: Theme.railIconSize
                    name: railRow.modelData.glyph
                    color: railRow.current ? Theme.color.accent : Theme.color.muted
                }

                Text {
                    anchors.left: mark.right
                    anchors.leftMargin: Theme.spacing.gap
                    anchors.right: parent.right
                    anchors.rightMargin: Theme.spacing.gap
                    anchors.verticalCenter: parent.verticalCenter
                    text: railRow.modelData.label
                    color: Theme.color.foreground
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.body
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                }

                HoverHandler {
                    cursorShape: Qt.PointingHandCursor
                }

                TapHandler {
                    gesturePolicy: TapHandler.ReleaseWithinBounds
                    onTapped: root.chosen(railRow.modelData.id)
                }
            }
        }
    }
}
