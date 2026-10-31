.pragma library
.import "Search.js" as Search
.import "Tabs.js" as Tabs

// Hit the drawn label instead of the whole name slot, including centered grid captions.
function onName(label, owner, point, centered) {
    if (!label || !label.visible) return false
    var at = label.mapFromItem(owner, point.x, point.y)
    var width = Math.min(label.width, label.contentWidth === undefined ? label.implicitWidth : label.contentWidth)
    var height = Math.min(label.height, label.contentHeight === undefined ? label.implicitHeight : label.contentHeight)
    var left = centered ? (label.width - width) / 2 : 0
    return at.x >= left && at.x < left + width && at.y >= 0 && at.y < height
}

// The pointer contract, declared in keys.toml's [[pointer]] table and decided here and nowhere
// else. tests/js/tap.js drives every row of Keymap.POINTER through the three functions below, so a
// click cannot change meaning without the table saying so and the table cannot advertise a click the
// code does not make.
//
// The rule is macOS's and the operator's: one tap selects, the second opens. The single-tap action is
// idempotent on a row, so it runs on both taps of a double click rather than behind a double-click
// timer, which would delay every selection by the whole mouseDoubleClickInterval.

// A view keeps its delegates across a re-list, so a tap count can outlive the row its first tap named.
var firstTap = null
// A later tap counts only on the row its gesture's first tap named; a tap on another row answers 0 and starts the count again.
function gestureCount(index, tapCount, root) {
    var row = index >= 0 ? root.rowFor(index) : null
    var key = String(root.path) + "\n" + (row ? row.n : "")
    if (tapCount === 1 || firstTap === null || firstTap.pane !== root) {
        firstTap = { pane: root, key: key, base: 0 }
        return tapCount
    }
    if (firstTap.key !== key) {
        firstTap = { pane: root, key: key, base: tapCount }
        return 0
    }
    return tapCount - firstTap.base
}

// One tap selects, the second opens; a click carries context 0 so the list never moves under the pointer.
function tapped(index, tapCount, modifiers, root) {
    if (index < 0) return
    tapCount = gestureCount(index, tapCount, root)
    if (tapCount === 0) return
    // Single-click mode opens on the first tap alone, so later taps of one gesture add nothing.
    if (root.singleClick === true && tapCount !== 1) return
    // Finder's two selection modifiers. Neither ever opens, and only the first tap of one counts,
    // so a modified double click selects once instead of toggling itself back off.
    if (modifiers & Qt.ControlModifier) {
        if (tapCount === 1) root.toggleSelectAt(index)
        return
    }
    if (modifiers & Qt.ShiftModifier) {
        if (tapCount === 1) root.extendSelectionTo(index)
        return
    }
    // What a tap means is the search's to say, not this file's: on a result it is
    // that it takes you to the file rather than launching it, and ui/js/Search.js activateAction
    // answers "open" everywhere else. A result answers on the first tap (2026-09-11), so the reveal
    // has already moved the pane by the time a second one arrives and that tap belongs to another
    // directory's row: it selects nothing and reveals nothing, tappedColumn's rule for its own reveal.
    var verb = Search.activateAction(root)
    if (verb === "reveal" && tapCount !== 1)
        return
    // Finder commits an open rename on click-away first, so its text lands before the selection moves.
    root.commitOpenRename()
    // A plain tap replaces the selection, so the next shift+click extends from a row on screen.
    root.selectOnly(index, 0)
    if (tapCount === 2 || verb === "reveal")
        root.act(verb)
    // Single-click mode opens files and folders on one tap, like the middle column already does.
    if (tapCount === 1 && verb === "open" && root.singleClick === true)
        root.act("open")
}

// The columns view's own middle column, and ui/ColumnsArea.qml is its only caller, which is what
// keeps this out of the list and the grid. Its two neighbour columns already go into a directory on
// one tap, and the operator's 2026-09-11 ruling is that the column holding the cursor does the same,
// which is Finder's column view. A directory alone: everything else is an ordinary listing tap and
// reads tapped() above, so a file still opens on the second tap and a modifier still only selects.
function tappedMiddle(index, tapCount, modifiers, root) {
    var row = index >= 0 ? root.rowFor(index) : null
    var plain = (modifiers & (Qt.ControlModifier | Qt.ShiftModifier)) === 0
    // tapped() counts the gesture itself, so a tap is counted once on either path.
    if (!row || !row.d || !plain) {
        tapped(index, tapCount, modifiers, root)
        return
    }
    tapCount = gestureCount(index, tapCount, root)
    if (tapCount === 0) return
    // The first tap moved the pane, so a second one is another directory's row, the same reason
    // tappedColumn refuses to reveal a neighbour twice.
    if (tapCount !== 1)
        return
    root.commitOpenRename()
    root.selectOnly(index, 0)
    // A directory listed as a search result is still a result, so the one decision point answers here
    // too: it reveals rather than opening, exactly as the same row does in the list view.
    root.act(Search.activateAction(root))
}

// Right click, in all three views: the row under the pointer takes the cursor and the menu opens there.
// Which rows it then means is decided by the pressed row, ui/js/Drag.js carried()'s rule for the
// pointer's other gesture: pressed inside the selection the menu addresses all of it, pressed outside
// it that row replaces the selection. Every entry ui/ContextMenu.qml draws is built from the cursor
// row while Move to Trash, Compress and Move to Dropbox dispatch through Ops.targetIndices, which
// prefers the selection, so a right click that left a selection elsewhere standing trashed rows the
// menu had never described.
function tappedMenu(index, eventPoint, root, menu) {
    var picked = root.selectedIndices()
    if (picked.length > 0 && picked.indexOf(index) < 0)
        root.clearSelection()
    root.setCursor(index, 0)
    menu.openAt(eventPoint.scenePosition)
    root.cancelSlowClick()
}

// A right click that landed on no row raises the directory's own menu, in all three views and the
// Trash; a row's own TapHandler has already answered for a row. A TapHandler declared inside a
// ListView or GridView belongs to its contentItem, so its position is already the content coordinate
// indexAt takes: adding contentY again read every row past the first screenful as empty space.
function onBackground(view, eventPoint) {
    return view.indexAt(eventPoint.position.x, eventPoint.position.y) < 0
}

// Alt+right-click is Tangerine's own pointer gesture for the wheel, kept beside its click contract.
// The row under the pointer takes the cursor and the wheel opens there; the action set itself reads
// the selection, exactly as the menu's rows do. A background press leaves the selection standing.
function tappedWheel(index, root, scenePoint) {
    if (index >= 0) {
        var picked = root.selectedIndices()
        if (picked.length > 0 && picked.indexOf(index) < 0)
            root.clearSelection()
        root.setCursor(index, 0)
    }
    root.cancelSlowClick()
    root.openActionWheel(scenePoint)
}

// A neighbour column in the columns view is a peek with no cursor of its own, so its rows answer a
// verb rather than acting. One tap on a directory makes it the pane's listing, which is the column
// view's own reveal and not an open; only a second tap opens a file. A peeked row belongs to another
// directory and every menu action addresses the pane's cursor, so a right click there is routed by
// ColumnPane to menuOnNeighbour before this is asked.
function tappedColumn(row, button, tapCount) {
    if (!row || button === Qt.RightButton)
        return ""
    if (button === Qt.MiddleButton)
        return row.d && tapCount === 1 ? "openTab" : ""
    if (row.d)
        return tapCount === 1 ? "reveal" : ""
    return tapCount === 2 ? "open" : ""
}

// Middle click on a directory opens it in a new tab; a file has no directory to show.
function tabTarget(row, base, root) {
    return row && row.d && typeof row.n === "string" ? root.join(base, row.n) : ""
}

function tappedTab(row, base, root) {
    var target = tabTarget(row, base, root)
    if (target.length === 0)
        return
    root.commitOpenRename()
    Tabs.openNew(root, target)
}

// The slow click renames on the pane's timer; a second tap opens through tapped() instead, so only a lone tap on the name arms it.
function slowTap(pane, armed, index, modifiers, dragActive, wasSole) {
    if (armed)
        pane.armSlowClick(index, modifiers, dragActive, wasSole)
    else
        pane.cancelSlowClick()
}
