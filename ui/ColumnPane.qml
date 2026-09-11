import QtQuick
import qs.Commons
import "." as Flea
import "js/Filter.js" as Filter
import "js/Tap.js" as Tap
import "js/Thumbs.js" as Thumbs

// One Miller column: a scrolling list of ColumnRows over either a peeked directory or the pane's
// own listing window. It owns no state; the area above it decides which row is which.
Item {
    id: root

    // [{n, d, i}], from a peek or from the pane's own held window.
    property var rows: []
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
    function restartCoalesce() { coalesce.restart() }
    function primeSettle() { settle.interval = root.pane.firstSettleMs }

    function visibleRange() {
        var visibleRows = Math.max(1, Math.ceil(view.height / Theme.fileRowHeight))
        var fallback = Thumbs.viewport(view.contentY - view.originY, Theme.fileRowHeight, visibleRows, view.count)
        if (root.pane && root.pane.renamingIndex >= 0) {
            var first = view.indexAt(0, view.contentY)
            var last = view.indexAt(0, view.contentY + view.height - 1)
            first = first < 0 ? fallback.first : first
            return {first: first, last: last < 0 ? Math.min(view.count - 1, first + visibleRows) : last}
        }
        return fallback
    }

    // The active column uses List/Grid's integer model and refills only around its viewport.
    function requestIfDrifted() {
        if (root.pane === null || !root.visible || root.pane.listInFlight
                || root.pane.total === 0 || root.pane.shown !== null)
            return
        var range = root.visibleRange()
        var heldEnd = root.pane.held + root.pane.rows.length
        if (root.pane.rows.length === 0
                || (range.first - root.pane.held < root.pane.refetchMargin && root.pane.held > 0)
                || (heldEnd - range.last < root.pane.refetchMargin && heldEnd < root.pane.total))
            root.pane.backend.window(Math.max(0, range.first - root.pane.buffer), root.pane.windowSize)
    }

    // The viewport's rows and no more, rule 1: the same plan the list and the grid run, over this column's own scroll position.
    function requestThumbs() {
        // visible is effective visibility, so the column kept alive under another view plans nothing against the shared state.
        if (root.pane === null || !root.visible || root.pane.total === 0 || root.pane.listInFlight)
            return
        var range = root.visibleRange()
        var span = Filter.span(root.pane.shown, range.first, range.last)
        var work = Filter.cut(Thumbs.plan(root.pane.thumbState, root.pane.rows, root.pane.held,
            span.first, span.last, ViewState.thumbnailMode), root.pane.shown, root.pane.thumbState)
        // Only the loaded preview owns an off-viewport request; manual cursor movement asks nothing extra.
        work.drop = work.drop.filter(function (index) { return index !== root.pane.previewIndex })
        root.pane.backend.thumbcancel(work.drop)
        root.pane.backend.thumb(work.ask)
        if (work.ask.length > 0) settle.interval = root.pane.settleMs
        root.thumbsApplied(work)
    }

    Connections {
        target: ViewState
        function onThumbnailModeChanged() { if (root.visible) settle.restart() }
    }

    Timer {
        id: coalesce
        interval: root.pane ? root.pane.coalesceMs : 0
        repeat: false
        onTriggered: root.requestIfDrifted()
    }

    Timer {
        id: settle
        interval: root.pane ? root.pane.settleMs : 120
        repeat: false
        onTriggered: root.requestThumbs()
    }
    onRowsChanged: if (root.pane !== null) settle.restart()
    onVisibleChanged: if (root.visible && root.pane !== null) { coalesce.restart(); settle.restart() }
    onHeightChanged: if (root.visible && root.pane !== null) { coalesce.restart(); settle.restart() }

    Flea.FileDrag {
        id: dragSession
        pane: root.pane
    }

    // Only the active column owns this directory; neighboring peek floors cannot target its listing.
    Flea.DropInto {
        anchors.fill: parent
        enabled: root.pane !== null && !root.pane.trash.opened && root.pane.searchMode === ""
        pane: root.pane
        dest: root.pane ? root.pane.path : ""
        destDev: root.pane && root.pane.backend && !root.pane.listInFlight ? root.pane.backend.dirDev : 0
    }

    ListView {
        id: view
        anchors.fill: parent

        model: root.pane ? root.pane.shownTotal : root.rows.length
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        onContentYChanged: if (root.pane !== null) { coalesce.start(); settle.restart() }
        reuseItems: true

        // G7 needs an empty press target below the final row even when a long column fills the viewport.
        footer: Item {
            width: view.width
            height: root.pane ? Theme.spacing.rowPaddingY : 0
        }

        Flea.FastScrollHandler {
            parent: view
            flickable: view
        }

        Flea.SelectionBand {
            parent: view
            pane: root.pane
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
            id: cell
            required property int index
            readonly property int listingIndex: root.pane ? Filter.at(root.pane.shown, index) : index
            width: view.width
            // A shrunk listing subscripts out of range under a delegate not yet released, and QML
            // hands that back as undefined; every row reader in the tree tests against a real null.
            row: root.pane ? root.pane.rowFor(listingIndex) : root.rows[index] !== undefined ? root.rows[index] : null
            thumb: root.pane !== null && Thumbs.allowed(row, ViewState.thumbnailMode) ? root.pane.thumbFor(listingIndex) : ""
            cursor: root.selectedIndex >= 0 && listingIndex === root.selectedIndex
            // The list and the grid both mark a selection member apart from the cursor; so does this.
            selected: root.pane !== null && root.pane.isSelected(listingIndex)
            dropTarget: dragSession.dropIndex >= 0 && listingIndex === dragSession.dropIndex
            dropCopying: dragSession.dragCopy
            // Read off the normalised row above: subscripting rows again hands a shrunk listing's undefined to a bool.
            lifted: root.liftedName.length > 0 && row !== null && row.n === root.liftedName
            dim: root.dim && !lifted

            TapHandler {
                id: tap
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                onTapped: function (eventPoint, button) {
                    if (root.pane !== null) {
                        if (cell.listingIndex < 0 || !cell.row) return
                        if (button === Qt.RightButton)
                            root.menuRequested(cell.listingIndex, eventPoint)
                        else
                            root.picked(cell.listingIndex, tap.tapCount, tap.point.modifiers)
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

            Flea.RowDrag {
                session: dragSession
                listingIndex: cell.listingIndex
                row: cell.row
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

    // corner: one editor for this column, never one inside each row. A delegate binding that follows
    // the pane's renamingIndex costs this column its keys, measured on the box: the comma that opens
    // Settings stopped reaching ui/js/Focus.js, which case_overlays catches. An overlay follows no
    // delegate, so the keys are safe by construction and only the active column ever draws one.
    readonly property int renameViewIndex: root.pane !== null && root.pane.renamingIndex >= 0
                                           ? Filter.viewOf(root.pane.shown, root.pane.renamingIndex) : -1
    readonly property bool renaming: root.renameViewIndex >= 0
    // What ui/Pane.qml's renameEditor() hands ui/Ipc.qml, the shape a list delegate hands it.
    readonly property Item editorField: renameLoader.item
    readonly property string editorText: renameLoader.item ? renameLoader.item.current : ""
    function commitEditor() { return renameLoader.item ? renameLoader.item.commit() : false }

    // The span ui/ColumnRow.qml draws its name in: the mark slot to its left, the chevron to its
    // right. The editor covers exactly that, so the row's icon and its chevron stay where they are.
    readonly property real renameLeft: Theme.spacing.rowPaddingX + Theme.iconSize + Theme.spacing.gap
    readonly property real renameRight: Theme.spacing.rowPaddingX + Theme.font.caption + Theme.spacing.gap

    // Opaque, and painted in the row's own roles: the row underneath goes on drawing its name, and
    // without this the two texts overprinted each other. The renaming row is always the cursor row.
    Rectangle {
        parent: view.contentItem
        visible: root.renaming
        x: root.renameLeft
        y: root.renameViewIndex * Theme.fileRowHeight
        width: Math.max(0, view.width - root.renameLeft - root.renameRight)
        height: Theme.fileRowHeight
        z: 1
        color: Theme.color.surface

        Rectangle {
            anchors.fill: parent
            color: Style.selectedAccentFill
        }
    }

    Loader {
        id: renameLoader
        parent: view.contentItem
        // Loaded only while a rename is open: a RenameField built beside every row reports its own
        // hide at creation, and that hide is an abandon.
        active: root.renaming
        x: root.renameLeft
        y: root.renameViewIndex * Theme.fileRowHeight
        width: Math.max(0, view.width - root.renameLeft - root.renameRight)
        height: Theme.fileRowHeight
        z: 2
        sourceComponent: Flea.RenameField {
            anchors.fill: parent
            pane: root.pane
            name: root.pane && root.pane.rowFor(root.pane.renamingIndex)
                  ? String(root.pane.rowFor(root.pane.renamingIndex).n).split("/").pop() : ""
            onCommitted: function (newName) { root.pane.commitRename(newName) }
            onAbandoned: root.pane.renamingIndex = -1
        }
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
            view.positionViewAtIndex(root.pane ? Filter.viewOf(root.pane.shown, root.selectedIndex) : root.selectedIndex, ListView.Contain)
    }
}
