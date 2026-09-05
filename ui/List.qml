import QtQuick
import "." as Flea
import "js/DirSizes.js" as DirSizes
import "js/Drag.js" as DragOps
import "js/Filter.js" as Filter
import "js/Tap.js" as Tap
import "js/Thumbs.js" as Thumbs
import "js/Trash.js" as Trash

// The listing's render, scroll and settle-triggered refetch, split out of Pane.qml; reaches Pane's state through the pane reference and the context menu through menu, both handed in at instantiation.
ListView {
    id: root

    property var pane: null
    property var menu: null

    // Pane owns cursorIndex, thumbState and dirSizeState; List only computes what changed and hands it back.
    // Both ends of cursorClamped are view positions, not listing rows: under a filter they differ.
    signal cursorClamped(int first, int last)
    signal thumbsApplied(var work)
    signal dirSizesApplied(var ask)
    signal dirSizesCancelled()

    // The drag in flight: the listing rows it carries, the folder row under it and whether ctrl is
    // making it a copy. Every row's frame and label bind to these, so they live on the view rather
    // than the delegate, and Pane.qml, at its line cap, holds none of it. ui/js/Drag.js decides.
    property var dragRows: []
    property int dropIndex: -1
    property bool dragCopy: false
    // The type Flea's own drag carries, so a drop can tell it from a foreign one: the compositor
    // hands this window's own platform drag back to these same DropAreas.
    readonly property string dragKey: DragOps.ROWS_MIME
    // What the lifted rows put on the wire, rebuilt at each lift and cleared with the gesture.
    property var dragMime: ({})

    focus: true
    model: pane.shownTotal
    clip: true
    cacheBuffer: Theme.rowHeight * pane.cacheRows
    boundsBehavior: Flickable.StopAtBounds
    highlightMoveDuration: 0
    // Every property the delegate draws is a binding on index, so a row leaving the buffer is re-bound rather than rebuilt.
    reuseItems: true

    delegate: Flea.Row {
        id: cell
        required property int index
        // index is where the row is drawn; listingIndex is the row the backend numbers, and under a
        // filter the two are different. Everything that leaves this delegate takes the listing one.
        readonly property int listingIndex: Filter.at(root.pane.shown, index)
        width: root.width
        row: root.pane.rowFor(listingIndex)
        cursor: listingIndex === root.pane.cursorIndex
        hovered: hover.hovered
        thumb: root.thumbFor(listingIndex)
        // The zebra follows the drawn position, so a narrowed listing still alternates row by row.
        alternate: index % 2 === 1
        selected: root.pane.isSelected(listingIndex)
        kindNames: root.pane.kindNames
        dirSize: root.dirSizeFor(listingIndex)
        // A filter paints its run the same way a search does; filtering below is what keeps the
        // ordinary columns, because these rows are still this directory's own and not walk results.
        searchQuery: root.pane.searchMode.length > 0 ? root.pane.searchQuery : root.pane.filterQuery
        filtering: root.pane.shown !== null
        renaming: listingIndex === root.pane.renamingIndex
        // -1 is also what Filter.at answers for a stale delegate, so an idle list must never light one.
        dropTarget: root.dropIndex >= 0 && listingIndex === root.dropIndex
        dropCopying: root.dragCopy
        armed: root.pane.trashArmedAt > 0 && Trash.targeted(root.pane, listingIndex)

        onRenameCommitted: function (newName) { root.pane.commitRename(newName) }
        onRenameAbandoned: root.pane.renamingIndex = -1

        HoverHandler {
            id: hover
        }

        TapHandler {
            id: tap
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onTapped: function (eventPoint, button) {
                if (button === Qt.RightButton)
                    Tap.tappedMenu(listingIndex, eventPoint, root.pane, root.menu)
                else
                    Tap.tapped(listingIndex, tap.tapCount, tap.point.modifiers, root.pane)
            }
        }

        // A press that moves past the threshold lifts the row and cancels the tap above. Both grab
        // transitions only clear state now: the compositor owns the drop decision once the platform
        // drag has started, so a stolen grab can no longer be mistaken for a release.
        DragHandler {
            id: lift
            target: null
            // The list is a Flickable and would take the grab past its own threshold; without ApprovesTakeOverByItems it cannot.
            grabPermissions: PointerHandler.CanTakeOverFromItems | PointerHandler.CanTakeOverFromHandlersOfDifferentType | PointerHandler.ApprovesTakeOverByHandlersOfSameType
            onActiveChanged: if (active) root.liftBegan(cell.listingIndex, ghost, lift.centroid)
            onCentroidChanged: if (active) root.liftMoved(lift.centroid)
            onGrabChanged: function (transition, point) {
                if (transition === PointerDevice.UngrabExclusive || transition === PointerDevice.CancelGrabExclusive)
                    root.liftEnded(ghost)
            }
        }

        // The item Qt hangs the drag off. Automatic makes it a real Wayland drag, so it reaches
        // other applications, and the compositor delivers it back to this window's own DropAreas,
        // which is how an internal drop still lands. The canvas draws no drag image; the folder's
        // frame and the status line are the whole feedback.
        Item {
            id: ghost
            Drag.dragType: Drag.Automatic
            Drag.supportedActions: Qt.CopyAction | Qt.MoveAction
            Drag.proposedAction: root.dragCopy ? Qt.CopyAction : Qt.MoveAction
            Drag.mimeData: root.dragMime
        }

        DropArea {
            anchors.fill: parent
            // The second key is how a drag from another application arrives: a DropArea matches an
            // external drag on its mime types, so naming the type here is the whole of accepting one.
            keys: [root.dragKey, "text/uri-list"]
            onEntered: function (drag) {
                if (!DragOps.canDrop(root.dragRows, cell.listingIndex, cell.row)) {
                    drag.accepted = false
                    return
                }
                // A foreign drag always copies; Flea's own takes the action Qt negotiated from the
                // modifier, because a platform drag runs a nested event loop in which this window
                // receives no key events and the Keys handler that used to carry ctrl cannot fire.
                root.dragCopy = DragOps.verbFor(DragOps.isOwnDrag(drag.getDataAsString(root.dragKey)), drag.proposedAction === Qt.CopyAction, root.pane.backend.dirDev, cell.row ? cell.row.v : 0) === "copy"
                root.dropIndex = cell.listingIndex
            }
            onPositionChanged: function (drag) {
                if (root.dropIndex === cell.listingIndex)
                    root.dragCopy = DragOps.verbFor(DragOps.isOwnDrag(drag.getDataAsString(root.dragKey)), drag.proposedAction === Qt.CopyAction, root.pane.backend.dirDev, cell.row ? cell.row.v : 0) === "copy"
            }
            onExited: {
                if (root.dropIndex === cell.listingIndex) root.dropIndex = -1
                // Only an external drag leaves this set with no lift to clear it: the internal one owns
                // the flag while it holds rows, and liftEnded is what resets it there.
                if (root.dragRows.length === 0) root.dragCopy = false
            }
            onDropped: function (drop) {
                // Only this window's own drag takes the internal path. The row marker names the
                // application and not the process, so another Flea window matched it, resolved its
                // indices against this listing's own empty selection, and dropped nothing at all.
                var own = DragOps.isOwnDrag(drop.getDataAsString(root.dragKey))
                if (own) {
                    var copying = DragOps.verbFor(true, drop.proposedAction === Qt.CopyAction, root.pane.backend.dirDev, cell.row ? cell.row.v : 0) === "copy"
                    root.dropped(cell.listingIndex, copying)
                    drop.accept(copying ? Qt.CopyAction : Qt.MoveAction)
                    return
                }
                // Another Flea window is a foreign source like any other: it arrives by path, never
                // by row, and it copies. Accepted as a copy explicitly and never as the proposed
                // action, so no source deletes its own file on the strength of this drop.
                DragOps.dropExternal(root.pane, drop.urls, cell.listingIndex)
                root.dropIndex = -1
                root.dragCopy = false
                drop.accept(Qt.CopyAction)
            }
        }
    }

    // States.dc.html "Filter active" draws this under the rows: caption type, muted, and gone the
    // moment there is nothing to account for. A footer scrolls with the rows, which is where it sits.
    footer: Item {
        width: root.width
        height: note.text.length > 0 ? Theme.chromeHeight : 0

        Text {
            id: note
            anchors.fill: parent
            anchors.leftMargin: Theme.spacing.rowPaddingX
            anchors.rightMargin: Theme.spacing.rowPaddingX
            verticalAlignment: Text.AlignVCenter
            text: Filter.note(root.pane.shown, root.pane.rows.length, root.pane.filterQuery)
            color: Theme.color.muted
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            elide: Text.ElideRight
            textFormat: Text.PlainText
        }
    }

    onContentYChanged: {
        // The wheel moves the view and not the cursor, so the cursor follows the viewport here.
        var first = Math.floor(root.contentY / Theme.rowHeight)
        var last = Math.min(root.pane.shownTotal - 1, first + root.pane.visibleRows - 1)
        if (last >= first) {
            root.cursorClamped(first, last)
        }
        root.menu.close()
        // Gated on hasPending (see DirSizes.js) rather than diffed at settle like thumbcancel, see docs/protocol.md "dirsizecancel".
        if (DirSizes.hasPending(root.pane.dirSizeState)) {
            root.pane.backend.dirsizecancel()
            root.dirSizesCancelled()
        }
        coalesce.restart()
        settle.restart()
    }

    // The drag begins once the pointer is past the threshold; the ghost goes under it first, because
    // Drag.start() delivers its enter at the ghost's own position.
    function liftBegan(index, ghost, centroid) {
        root.dragRows = DragOps.carried(root.pane, index)
        root.dropIndex = -1
        root.liftMoved(centroid)
        root.dragMime = DragOps.mimeFor(root.pane, root.dragRows)
        // Automatic starts the platform drag on this assignment and does not return until the drop,
        // so everything the gesture needs is already set above. The drop lands in a DropArea, this
        // window's own or another application's, while this line blocks.
        ghost.Drag.active = true
    }

    // Only the modifier survives here: the platform drag takes its position from the pointer, and
    // once it starts the compositor owns the pointer and this stops being called at all.
    function liftMoved(centroid) {
        root.dragCopy = DragOps.copying(centroid.modifiers)
    }

    // The delegate drawing the editor, or null when the row was released past the cache buffer,
    // never built, or rebuilt under a listing that no longer holds it. renamingIndex is a listing
    // row and itemAtIndex wants a view position, and a filter makes those different.
    function renameEditor() {
        if (root.pane.renamingIndex < 0)
            return null
        var item = root.itemAtIndex(Filter.viewOf(root.pane.shown, root.pane.renamingIndex))
        return item && item.renaming ? item : null
    }

    // The row drawing the editor owns its text, so it is asked to commit rather than the pane
    // guessing a name.
    function commitOpenRename() {
        var item = root.renameEditor()
        if (!item)
            return
        // The pointer chose a row of its own, so the rename's reply must reveal nothing over it. Set
        // before the commit because the backend's reply is what reads it, and taken back when the
        // editor abandoned instead: a leaked flag made the next rename drop the cursor to the top.
        root.pane.renameKeepsPointerRow = true
        if (!item.commitEditor())
            root.pane.renameKeepsPointerRow = false
    }

    // The platform drag has already finished inside liftBegan and the compositor decided whether a
    // drop happened, so this only clears the gesture's own state.
    function liftEnded(ghost) {
        ghost.Drag.active = false
        root.dragRows = []
        root.dragMime = ({})
        root.dropIndex = -1
        root.dragCopy = false
        root.say("")
    }

    function dropped(index, copying) { DragOps.drop(root.pane, root.dragRows, index, copying) }

    // The bar's sticky slot belongs to a running transfer, so a drag only borrows it while none runs.
    function say(text) { if (!root.pane.transfer.running) root.pane.sticky(text) }

    // One line for the whole gesture, said again whenever the count, the folder under the pointer or
    // the verb moves. The row is looked up here and not through a bound property: a binding on
    // dropIndex is not yet refreshed inside onDropIndexChanged, and the line read "to a folder"
    // over a folder whose frame was already up.
    function sayDrag() {
        if (root.dragRows.length === 0)
            return
        var row = root.pane.rowFor(root.dropIndex)
        // The note follows the payload itself, so the bar can never promise a reach the wire does not
        // carry: no uri-list on the drag means no other application can take it.
        root.say(DragOps.line(root.dragRows.length, row ? row.n : "", root.dragCopy)
                 + DragOps.reachNote(root.dragMime.hasOwnProperty("text/uri-list")))
    }
    onDragRowsChanged: root.sayDrag()
    // dragMime is built after dragRows inside liftBegan, so the line above would otherwise be said
    // once against the previous gesture's payload and never corrected.
    onDragMimeChanged: root.sayDrag()
    onDropIndexChanged: root.sayDrag()
    onDragCopyChanged: root.sayDrag()

    // Pane's own open() and its Connections.onRows reach these two through the wrapper functions below.
    Timer {
        id: coalesce
        interval: root.pane.coalesceMs
        repeat: false
        onTriggered: root.requestIfDrifted()
    }

    // A fast listing beats the compositor's resize, so a viewport change restarts the settle exactly like a scroll does.
    Connections {
        target: root.pane
        function onVisibleRowsChanged() { settle.restart() }
    }

    Timer {
        id: settle
        interval: root.pane.firstSettleMs
        repeat: false
        onTriggered: { root.requestThumbs(); root.requestDirSizes() }
    }

    // Pane.open() primes the next settle to the short first-screen interval before any row arrives.
    function primeSettle() { settle.interval = root.pane.firstSettleMs }
    function restartCoalesce() { coalesce.restart() }
    function restartSettle() { settle.restart() }

    // Only the visible rows, only once each, and only after the list has stopped moving.
    function requestThumbs() {
        if (root.pane.shownTotal === 0 || root.pane.listInFlight)
            return
        var view = Thumbs.viewport(root.contentY, Theme.rowHeight, root.pane.visibleRows, root.pane.shownTotal)
        // A filtered viewport covers a set and not a run, so the run it spans is what the planner
        // gets and Filter.cut takes back every row inside that run the filter is hiding.
        var span = Filter.span(root.pane.shown, view.first, view.last)
        var work = Filter.cut(Thumbs.plan(root.pane.thumbState, root.pane.rows, root.pane.held, span.first, span.last), root.pane.shown)
        root.pane.backend.thumbcancel(work.drop)
        root.pane.backend.thumb(work.ask)
        // The short first settle latches to the fling debounce only once a request has actually gone out.
        if (work.ask.length > 0)
            settle.interval = root.pane.settleMs
        root.thumbsApplied(work)
    }

    function thumbFor(index) {
        return Thumbs.fileFor(root.pane.thumbState, index)
    }

    function dirSizeFor(index) {
        return DirSizes.sizeFor(root.pane.dirSizeState, index)
    }

    // Same idiom as requestThumbs, minus a cancel: onContentYChanged already sent it, see above.
    function requestDirSizes() {
        if (root.pane.shownTotal === 0 || root.pane.listInFlight)
            return
        // Thumbs.viewport() is reused: it takes no thumb-specific state, only geometry.
        var view = Thumbs.viewport(root.contentY, Theme.rowHeight, root.pane.visibleRows, root.pane.shownTotal)
        var span = Filter.span(root.pane.shown, view.first, view.last)
        var ask = Filter.keep(DirSizes.plan(root.pane.dirSizeState, root.pane.rows, root.pane.held, span.first, span.last), root.pane.shown)
        if (ask.length > 0) {
            root.pane.backend.dirsize(ask)
            settle.interval = root.pane.settleMs
        }
        root.dirSizesApplied(ask)
    }

    // Handle an empty held window explicitly before applying held-edge arithmetic.
    function requestIfDrifted() {
        // A filter narrows rows the pane is already holding, so it can never scroll past them: no
        // window request goes out while one stands, which is what "no round trip" means here.
        if (root.pane.total === 0 || root.pane.shown !== null)
            return
        var firstVisible = Math.floor(root.contentY / Theme.rowHeight)
        var lastVisible = firstVisible + root.pane.visibleRows
        if (root.pane.rows.length === 0) {
            root.requestAround(firstVisible)
            return
        }
        var heldEnd = root.pane.held + root.pane.rows.length
        if (firstVisible - root.pane.held < root.pane.refetchMargin && root.pane.held > 0) {
            root.requestAround(firstVisible)
        } else if (heldEnd - lastVisible < root.pane.refetchMargin && heldEnd < root.pane.total) {
            root.requestAround(firstVisible)
        }
    }

    function requestAround(firstVisible) {
        var start = Math.max(0, firstVisible - root.pane.buffer)
        root.pane.backend.window(start, root.pane.windowSize)
    }
}
