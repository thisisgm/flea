import QtQuick
import qs.Commons
import "." as Flea
import "js/Tap.js" as Tap

// One Miller column: a scrolling list of ColumnRows over either a peeked directory or the pane's
// own listing window. It owns no state; the area above it decides which row is which.
Item {
    id: root

    // [{n, d, i}], from a peek or from the pane's own held window.
    property var rows: []
    // The pane's held offset, so a listing window's row index maps back to a real cursor index.
    property int offset: 0
    // The row this column's cursor is on, as an absolute index; -1 when this column has no cursor.
    property int selectedIndex: -1
    // The row named here is the one the trail passes through: lifted like a hover, never accented.
    property string liftedName: ""
    // The pane whose listing this column draws, or null for a peek. Only that one column has a
    // selection to paint, and pane.isSelected reads selectionVersion, so the delegate follows it.
    property var pane: null
    // A column that is not the active one reads back.
    property bool dim: false
    // The mode of a denied peek, or -1 when this column's directory was read and its rows are true.
    property int lockedMode: -1
    // Whether zero rows here means empty: false while a peek is still out, because a pending peek answers zero rows too, and false for the pane's own listing, whose empty answer is the hero ui/shell.qml lays over the area.
    property bool drawsEmpty: false

    // isDir says which of the two things a neighbour column's row is: a directory the pane opens as
    // its own listing, or a file it hands to the opener. See keys.toml's [[pointer]] table.
    signal activated(string name, bool isDir)
    signal picked(int index, int tapCount, int modifiers)
    // The row under a right click. Only the column carrying the pane's own listing answers it, because a peeked column's rows are another directory's and every menu action addresses the pane's cursor.
    signal menuRequested(int index, var eventPoint)

    // The listArea contract ui/ColumnsArea.qml drives the middle column through; the view is private.
    function positionViewAtIndex(index, mode) { view.positionViewAtIndex(index, mode) }
    function itemAtIndex(index) { return view.itemAtIndex(index) }

    ListView {
        id: view
        anchors.fill: parent

        model: root.rows.length
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        reuseItems: true

        delegate: Flea.ColumnRow {
            required property int index
            width: view.width
            // A shrunk listing subscripts out of range under a delegate not yet released, and QML
            // hands that back as undefined; every row reader in the tree tests against a real null.
            row: root.rows[index] !== undefined ? root.rows[index] : null
            cursor: root.selectedIndex >= 0 && root.offset + index === root.selectedIndex
            // The list and the grid both mark a selection member apart from the cursor; so does this.
            selected: root.pane !== null && root.pane.isSelected(root.offset + index)
            lifted: root.liftedName.length > 0 && root.rows[index] && root.rows[index].n === root.liftedName
            dim: root.dim && !lifted

            TapHandler {
                id: tap
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                onTapped: function (eventPoint, button) {
                    // selectedIndex is given to the pane's own column and to no other, so it is what
                    // says this column takes the listing's click contract rather than a peek's.
                    if (root.selectedIndex >= 0) {
                        if (button === Qt.RightButton)
                            root.menuRequested(root.offset + index, eventPoint)
                        else
                            root.picked(root.offset + index, tap.tapCount, tap.point.modifiers)
                        return
                    }
                    var verb = Tap.tappedColumn(root.rows[index], button, tap.tapCount)
                    if (verb.length > 0)
                        root.activated(root.rows[index].n, verb === "reveal")
                }
            }
        }
    }

    // A denied peek answers zero rows, the exact count an empty directory answers, so a locked column draws States.dc.html's Locked tile rather than reading as an empty one.
    Flea.StateMessage {
        anchors.fill: parent
        listingState: root.lockedMode >= 0 ? "locked" : "ready"
        lockedMode: root.lockedMode
        total: root.rows.length
    }

    // The empty answer drawn quiet: the mark size Locked and Error already take in this slot, the hero's own first phrase, and a fixed caption, which is what keeps EmptyState's rotation off in a column the cursor rebuilds on every step.
    Flea.EmptyState {
        id: emptyTile
        anchors.fill: parent
        visible: root.drawsEmpty && root.rows.length === 0 && root.lockedMode < 0
        caption: emptyTile.messages[0]
        mark: "folder"
    }

    // The cursor can move off screen through the keyboard, so the column follows it.
    onSelectedIndexChanged: {
        if (root.selectedIndex >= 0)
            view.positionViewAtIndex(root.selectedIndex - root.offset, ListView.Contain)
    }
}
