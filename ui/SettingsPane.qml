import QtQuick
import "." as Flea
import "js/Settings.js" as Settings

// View supplies the compact card's stable height; longer sections scroll within the same viewport.
Flickable {
    id: root

    property string section: "display"
    // The one object ui/js/Settings.js rows() reads, built by the panel so both files see the same
    // values. Not named state: that is Item's own property, and shadowing it is a qmllint override.
    property var values: ({})
    property int cursor: 0
    property string side: "pane"
    readonly property real compactHeight: sections.count > 0 && sections.itemAt(0) ? sections.itemAt(0).implicitHeight : 0

    signal activated(int index)
    signal pointerMoved(int index)
    signal favouriteMoved(int index, int to)
    signal stepped(int index, int direction)
    signal stopPicked(int index, int stop)

    // count is read so the binding re-evaluates once the Repeater has built its columns.
    readonly property Item current: sections.count > 0
        ? sections.itemAt(root.section === "columns" ? Settings.SECTIONS.length : Settings.sectionIndex(root.section)) : null

    onContentYChanged: inspection.restart()
    onSectionChanged: inspection.restart()
    onVisibleChanged: if (visible) inspection.restart()
    Timer {
        id: inspection
        interval: 120
        onTriggered: {
            if (!root.visible || root.section !== "places" || !root.current) return
            var indices = []
            for (var i = 0; i < root.current.rows.count; i++) {
                var item = root.current.rows.itemAt(i)
                if (item && item.row.kind === "favourite" && item.y + item.height > root.contentY && item.y < root.contentY + root.height)
                    indices.push(item.row.favouriteIndex)
            }
            Favourites.inspect(indices)
        }
    }
    contentWidth: width
    contentHeight: root.current ? root.current.implicitHeight : 0
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    onHeightChanged: root.showCursor(root.cursor)

    Flea.FastScrollHandler {
        parent: root
        flickable: root
    }

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

    Repeater {
        id: sections
        // Keep View's measurement intact while its Columns subpage is open.
        model: Settings.SECTIONS.concat([{id: "columns"}])

        delegate: Column {
            id: column
            required property var modelData
            readonly property alias rows: rowItems
            width: root.width
            visible: modelData.id === root.section

            Repeater {
                id: rowItems
                model: Settings.rows(column.modelData.id, root.values)

                delegate: Flea.SettingsRow {
                    required property var modelData
                    required property int index
                    width: column.width
                    row: modelData
                    firstRow: index === 0
                    current: column.visible && root.side === "pane" && root.cursor === index
                    onActivated: root.activated(index)
                    onPointerMoved: root.pointerMoved(index)
                    onFavouriteMoved: function (to) { root.favouriteMoved(index, to) }
                    onStepped: function (direction) { root.stepped(index, direction) }
                    onStopPicked: function (stop) { root.stopPicked(index, stop) }
                }
            }
        }
    }
}
