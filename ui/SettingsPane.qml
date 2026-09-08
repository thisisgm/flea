import QtQuick
import "." as Flea
import "js/Settings.js" as Settings

// The settings panel's scrolling pane. Every section's rows are built, and only the chosen one shows,
// so the card can take the tallest section and stay one size whichever section is up: GM's ruling
// for mouse users, over the board's clamp-to-content reading. The panel itself is built on first open.
Flickable {
    id: root

    property string section: "display"
    // The one object ui/js/Settings.js rows() reads, built by the panel so both files see the same
    // values. Not named state: that is Item's own property, and shadowing it is a qmllint override.
    property var values: ({})
    property int cursor: 0
    property string side: "pane"
    // The tallest section's rows, the height the card keeps for every section the way the network dialog keeps its tallest form's.
    property real tallest: 0

    signal activated(int index)
    signal stepped(int index, int direction)
    signal stopPicked(int index, int stop)

    // count is read so the binding re-evaluates once the Repeater has built its columns.
    readonly property Item current: sections.count > 0 ? sections.itemAt(Settings.sectionIndex(root.section)) : null

    contentWidth: width
    contentHeight: root.current ? root.current.implicitHeight : 0
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    // The row item at an index of the chosen section, or null before the columns exist.
    function rowItem(index) { return root.current ? root.current.rows.itemAt(index) : null }

    // The Column inside the Flickable holds rows of two different heights, so the visible window is
    // moved onto the row itself rather than derived from an index times a row height.
    function showCursor(index) {
        var item = root.rowItem(index)
        if (!item)
            return
        if (item.y < root.contentY)
            root.contentY = item.y
        else if (item.y + item.height > root.contentY + root.height)
            root.contentY = item.y + item.height - root.height
    }

    function remeasure() {
        var tallest = 0
        for (var i = 0; i < sections.count; i++) {
            var column = sections.itemAt(i)
            if (column && column.implicitHeight > tallest)
                tallest = column.implicitHeight
        }
        root.tallest = tallest
    }

    Repeater {
        id: sections
        model: Settings.SECTIONS

        delegate: Column {
            id: column
            required property var modelData
            readonly property alias rows: rowItems
            width: root.width
            visible: modelData.id === root.section
            onImplicitHeightChanged: root.remeasure()

            Repeater {
                id: rowItems
                model: Settings.rows(column.modelData.id, root.values)

                delegate: Flea.SettingsRow {
                    required property var modelData
                    required property int index
                    width: column.width
                    row: modelData
                    current: column.visible && root.side === "pane" && root.cursor === index
                    onActivated: root.activated(index)
                    onStepped: function (direction) { root.stepped(index, direction) }
                    onStopPicked: function (stop) { root.stopPicked(index, stop) }
                }
            }
        }
    }
}
