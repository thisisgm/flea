import QtQuick
import qs.Commons
import "." as Flea
import "js/Tap.js" as Tap
import "js/Thumbs.js" as Thumbs

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
    // A right click that landed on no row, which only the pane's own column can answer for the same
    // reason: the background menu acts on the directory being shown and a peek is not that directory.
    signal backgroundMenuRequested(var eventPoint)
    // A right click on a peek's row: the peek's directory becomes the listing with this row as the cursor, and the menu opens there.
    signal neighbourMenuRequested(string name)
    // The thumbnail plan for this column's viewport, computed here and written by the pane, the grid's own contract.
    signal thumbsApplied(var work)

    // The listArea contract ui/ColumnsArea.qml drives the middle column through; the view is private.
    function positionViewAtIndex(index, mode) { view.positionViewAtIndex(index, mode) }
    function itemAtIndex(index) { return view.itemAtIndex(index) }
    function contentY() { return view.contentY }
    function restartSettle() { settle.restart() }

    // The viewport's rows and no more, rule 1: the same plan the list and the grid run, over this column's own scroll position.
    function requestThumbs() {
        // visible is effective visibility, so the column kept alive under another view plans nothing against the shared state.
        if (root.pane === null || !root.visible || root.pane.total === 0 || root.pane.listInFlight)
            return
        var span = Thumbs.viewport(view.contentY, Theme.rowHeight, Math.max(1, Math.ceil(view.height / Theme.rowHeight)), root.rows.length)
        var first = root.offset + span.first
        var last = root.offset + span.last
        var work = Thumbs.plan(root.pane.thumbState, root.pane.rows, root.pane.held, first, last)
        // The preview column draws the cursor row whatever this viewport shows, so its thumbnail is asked for and never dropped.
        var cursor = root.pane.cursorIndex
        if (cursor >= 0 && (cursor < first || cursor > last)) {
            work.drop = work.drop.filter(function (i) { return i !== cursor })
            var cursorRow = root.rows[cursor - root.offset]
            if (cursorRow && cursorRow.t && root.pane.thumbState.file[cursor] === undefined)
                work.ask.push(cursor)
        }
        root.pane.backend.thumbcancel(work.drop)
        root.pane.backend.thumb(work.ask)
        root.thumbsApplied(work)
    }

    Timer {
        id: settle
        interval: root.pane ? root.pane.settleMs : 120
        repeat: false
        onTriggered: root.requestThumbs()
    }
    onRowsChanged: if (root.pane !== null) settle.restart()
    onVisibleChanged: if (root.visible && root.pane !== null) settle.restart()

    ListView {
        id: view
        anchors.fill: parent

        model: root.rows.length
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        onContentYChanged: if (root.pane !== null) settle.restart()
        reuseItems: true

        Flea.FastScrollHandler {
            parent: view
            flickable: view
        }

        // Empty space below the last row, the same rule ui/List.qml carries; pane is what says this
        // column draws the pane's own listing rather than a peek.
        TapHandler {
            acceptedButtons: Qt.RightButton
            onTapped: function (eventPoint) {
                if (root.pane !== null
                        && view.indexAt(view.contentX + eventPoint.position.x, view.contentY + eventPoint.position.y) < 0)
                    root.backgroundMenuRequested(eventPoint)
            }
        }

        delegate: Flea.ColumnRow {
            required property int index
            width: view.width
            // A shrunk listing subscripts out of range under a delegate not yet released, and QML
            // hands that back as undefined; every row reader in the tree tests against a real null.
            row: root.rows[index] !== undefined ? root.rows[index] : null
            thumb: root.pane !== null ? root.pane.thumbFor(root.offset + index) : ""
            cursor: root.selectedIndex >= 0 && root.offset + index === root.selectedIndex
            // The list and the grid both mark a selection member apart from the cursor; so does this.
            selected: root.pane !== null && root.pane.isSelected(root.offset + index)
            // Read off the normalised row above: subscripting rows again hands a shrunk listing's undefined to a bool.
            lifted: root.liftedName.length > 0 && row !== null && row.n === root.liftedName
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
                    if (button === Qt.RightButton && root.rows[index]) {
                        root.neighbourMenuRequested(root.rows[index].n)
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

    // For ui/Ipc.qml's columnChildEmpty readers: the tile's state and its mark's box.
    readonly property Item emptyItem: emptyTile
    // The same hero the list draws, per the operator: a peeked empty directory animates like the pane's own.
    Flea.EmptyState {
        id: emptyTile
        anchors.fill: parent
        visible: root.drawsEmpty && root.rows.length === 0 && root.lockedMode < 0
    }

    // The cursor can move off screen through the keyboard, so the column follows it.
    onSelectedIndexChanged: {
        if (root.selectedIndex >= 0)
            view.positionViewAtIndex(root.selectedIndex - root.offset, ListView.Contain)
    }
}
