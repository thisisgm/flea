.pragma library
.import "Eject.js" as Eject
.import "Filter.js" as Filter
.import "Format.js" as Format
.import "Keymap.js" as Keymap
.import "Ops.js" as Ops
.import "PreviewKeys.js" as PreviewKeys
.import "RailKeys.js" as RailKeys
.import "Search.js" as Search
.import "Sort.js" as Sort
.import "Trash.js" as Trash
.import "Tabs.js" as Tabs

var LIST = "list"
var RAIL = "rail"

// Tab is the only thing that moves focus between views, so the rule lives in one function.
function next(current) {
    return current === LIST ? RAIL : LIST
}

function shareBrowserHere(root) {
    return !!(root.shareBrowser && root.shareBrowser.active && (!root.shareBrowser.owner || root.shareBrowser.owner === root))
}

// The one lookup Pane.qml's Keys.onPressed calls. "addNetwork" is a rail-only action (the
// dialog is reached from the rail's own "+" mark), so "a" does nothing in the list;
// filtering it here, not in Keymap.js, keeps the generated file a pure keys.toml mirror.
// seekBack/seekForward get the same treatment, scoped to an open MEDIA preview instead of the
// rail, so Left/Right stay silent everywhere else rather than reaching act()'s "not built yet".
function lookup(event, root) {
    var context = root.preview.active ? (root.preview.isPdf ? "pdf" : root.preview.isMedia ? "media" : "preview")
                  : shareBrowserHere(root) ? "menu" : root.focusView === RAIL ? "rail" : "listing"
    // Grid arrows address visual neighbours even when the preset uses them to open folders in List.
    if (context === "listing" && root.viewMode === "grid" && event.modifiers === Qt.NoModifier) {
        if (event.key === Qt.Key_Left) return "cursorLeft"
        if (event.key === Qt.Key_Right) return "cursorRight"
    }
    var action = Keymap.lookup(event.key, event.text, event.modifiers, context)
    // The share listing borrows the menu context for j/k/enter, but it has no submenu to step into.
    if (action === "menuRight" && shareBrowserHere(root)) return "open"
    // List and Grid filter held rows; search owns the header while its results are active.
    if (action === "filter")
        return (root.viewMode !== "columns" && root.searchMode.length === 0) ? action : ""
    // Left and Right seek inside a media preview and turn the page in a PDF one. With no preview
    // open they are free, and in the grid they are the only sensible way to move one tile sideways,
    // so the grid claims them there.
    if (action === "seekBack" || action === "seekForward") {
        if (root.preview.active && (root.preview.isMedia || root.preview.isPdf))
            return action
        if (root.viewMode === "grid")
            return action === "seekBack" ? "cursorLeft" : "cursorRight"
        return ""
    }
    // Minus, plus and e mean nothing outside a PDF. l is h's forward: page, else enter or preview.
    if (action === "zoomOut" || action === "zoomIn" || action === "expand")
        return (root.preview.active && root.preview.isPdf) ? action : ""
    if (action === "pageForward") {
        if (root.preview.active)
            return root.preview.isPdf ? action : ""
        if (shareBrowserHere(root) || root.focusView === RAIL)
            return "open"
        var row = root.rowFor(root.cursorIndex)
        return row && (row.d || (Format.isSymlink(row.p) && row.i === "folder")) ? "open" : (row ? "preview" : "")
    }
    // reveal only means something on a search result, so o is discarded everywhere else.
    if (action === "reveal" && root.searchMode !== Search.RESULTS)
        return ""
    // A sort ends the running walk in the backend and the search strip hides the mark that would
    // show it happening, so both sort keys go quiet for as long as a search owns the header.
    if (action === "sortNext" || action === "sortReverse")
        return root.searchMode.length === 0 ? action : ""
    return action
}

// Lifted from Pane.qml's Keys.onPressed: the map holds the keys, this holds the behaviour.
// Takes the Pane root because every case is a method call or a property read on it.
function act(action, root, menuId, paths) {
    switch (action) {
    // Letter bindings follow item order; physical grid arrows use gridArrow's visual neighbours.
    case "cursorDown": step(root, 1); return
    case "cursorUp": step(root, -1); return
    case "cursorLeft": step(root, -1); return
    case "cursorRight": step(root, 1); return
    case "cursorFirst": Filter.setCursorView(root, 0); return
    case "cursorLast": Filter.setCursorView(root, root.shownTotal - 1); return
    case "pageDown": step(root, Math.max(1, Math.floor(root.visibleRows / 2))); return
    case "pageUp": step(root, -Math.max(1, Math.floor(root.visibleRows / 2))); return
    case "open": root.openCursor(); return
    case "parent": root.openParent(); return
    case "historyBack": root.goBack(); return
    case "historyForward": root.goForward(); return
    case "togglePreview": root.togglePreviewColumn(); return
    case "loadPreview": root.loadSelectionPreview(); return
    case "focusPreview": root.focusPreviewColumn(); return
    case "windowNew": root.newWindow(); return
    case "toggleHidden": root.toggleHidden(); return
    // Popups handle Escape first; the focused listing then unwinds filter, search, status and marks.
    case "escape":
        if (root.filterTyping || root.filterQuery.length > 0) Filter.close(root)
        else if (root.searchMode.length > 0 && root.focusView === LIST) Search.cancel(root)
        else if (root.statusBar && root.statusBar.escapePressed()) return
        else root.escapePressed()
        return
    case "preview": PreviewKeys.open(root); return
    case "toggleSelect": root.toggleSelect(); return
    case "extendDown": root.extendSelection(1); return
    case "extendUp": root.extendSelection(-1); return
    case "selectAll": root.selectAll(); return
    // A walk replaces the listing the filter was narrowing, so the filter goes before the query line
    // does: leaving it up would hide every result that did not happen to match it.
    case "search": Filter.close(root); Search.start(root); return
    case "filter": Filter.start(root); return
    case "reveal": Search.reveal(root); return
    // The write operations; every one of them is reversible with undo, so none of them confirms.
    case "duplicate": Ops.duplicate(root, menuId); return
    case "trash": Ops.trash(root, menuId); return
    case "trashArm": Trash.arm(root); return
    case "copy": Ops.clip(root, false, paths); return
    case "copydirpath": root.copyDirPath(); return
    case "cut": Ops.clip(root, true, paths); return
    case "paste": Ops.paste(root); return
    case "movePaste":
        if (root.clipboard.paths.length === 0) { root.message("The clipboard is empty.", false); return }
        root.backend.send({c: "transfer", op: "move", paths: root.clipboard.paths, dest: root.path})
        root.clipboard = Ops.emptyClipboard()
        return
    case "undo": Ops.undo(root); return
    case "redo": root.backend.send({c: "redo"}); return
    // m. Mounts.raiseMenu says why a favourite has no menu; here the pane says whether a row was
    // under the cursor at all, and an empty or fully filtered listing gets the sentence, not silence.
    case "menu":
        if (!root.openCursorMenu())
            root.message("No row under the cursor to open a menu on.", false)
        return
    case "extract": Ops.extract(root, menuId); return
    case "dropbox": root.moveToDropbox(menuId); return
    case "sharelink": root.copyShareLink(); return
    // Convert opens the one popup this whole design has; every other operation answers without one.
    case "convert": root.openConvert(menuId); return
    // The header answers the same two through ui/Pane.qml, so the key and the click share one route.
    case "sortNext": Sort.next(root); return
    case "sortReverse": Sort.reverse(root); return
    case "addNetwork": root.sidebar.addRequested(); return
    case "eject": Eject.release(root, root.sidebar, false); return
    // Finder's Cmd+1/2/3; the chrome's three buttons write the same property, so they follow.
    case "viewList": root.chooseView("list"); return
    case "viewColumns": root.chooseView("columns"); return
    case "viewGrid": root.chooseView("grid"); return
    case "newFolder": Ops.newFolder(root); return
    // The directory being shown, not the row: the menu row and the chord both land here.
    case "openTerminal": root.openTerminal(); return
    }
    // A submenu row fires "<action>:<id>", which is how one signal covers Taildrop and Compress both.
    if (action.indexOf("compress:") === 0) {
        if (paths) Ops.compressResolved(root, paths, action.substring("compress:".length), menuId)
        else Ops.compress(root, action.substring("compress:".length))
        return
    }
    // The background menu's Sort by flyout, routed to the header click's own function so an aimed
    // click and an aimed menu row cannot come to mean different things.
    if (action.indexOf("sort:") === 0) {
        Sort.column(root, action.substring("sort:".length))
        return
    }
    // Both keys the Tui board drew ahead of their features are built now, so neither answers with
    // a sentence any more: tabs run here, and handleKey opens the path bar before the views see it.
    if (action.indexOf("tab") === 0) { Tabs.act(action, root); return }
    root.message(action + " is not built yet.", false)
}

// Arrows require an existing visual cell, even when item-order navigation wraps at the ends.
function gridArrow(event, action, root) {
    if (root.viewMode !== "grid") return false
    var columns = root.cursorStride
    var delta = event.key === Qt.Key_Down && (action === "cursorDown" || action === "extendDown") ? columns
              : event.key === Qt.Key_Up && (action === "cursorUp" || action === "extendUp") ? -columns
              : event.key === Qt.Key_Left && action === "cursorLeft" ? -1
              : event.key === Qt.Key_Right && action === "cursorRight" ? 1 : 0
    if (!delta) return false
    var index = Filter.viewOf(root.shown, root.cursorIndex)
    var nextIndex = index + delta
    if (nextIndex < 0 || nextIndex >= root.shownTotal
            || (event.key === Qt.Key_Left && index % columns === 0)
            || (event.key === Qt.Key_Right && nextIndex % columns === 0)) return true
    if (action === "extendDown" || action === "extendUp") root.extendSelection(delta)
    else Filter.moveCursor(root, delta)
    return true
}

// Only a step from an end wraps; page overshoots and selection extensions retain their clamps.
function step(root, delta) {
    var last = root.shownTotal - 1
    if (root.wrapAtEnds !== true || last < 0) {
        Filter.moveCursor(root, delta)
        return
    }
    var from = Filter.viewOf(root.shown, root.cursorIndex)
    var to = from + delta
    if (to < 0)
        to = from === 0 ? last : 0
    if (to > last)
        to = from === last ? 0 : last
    Filter.setCursorView(root, to)
}

// ui/ShareBrowser.qml's own overlay, the same j/k/open/escape shape PreviewKeys.act uses.
function shareBrowserAct(action, root) {
    switch (action) {
    case "cursorDown": root.shareBrowser.moveCursor(1); return
    case "cursorUp": root.shareBrowser.moveCursor(-1); return
    case "open": root.shareBrowser.activateCursor(); return
    case "escape": root.shareBrowser.close(); return
    }
}

// The cursor keys and only those, resolved through the generated table rather than through a second
// list of key codes, which is how issue 28's Home, End and page keys came with issue 12 for free. A
// printable character is excluded before the lookup, or j and k would leave the line instead of
// being typed into it.
var LEAVES_LINE = ["cursorDown", "cursorUp", "cursorFirst", "cursorLast", "pageDown", "pageUp"]

function leavesLine(event) {
    if (event.text.length === 1 && event.text >= " ")
        return false
    return LEAVES_LINE.indexOf(Keymap.lookup(event.key, event.text, event.modifiers)) >= 0
}

// Vim pairs are consecutive inputs on the same selection; pointer or navigation changes disarm them.
function sequenceAction(action, root) {
    var pairs = { copyArm: "copy", cutArm: "cut", pasteArm: "paste", cursorFirstArm: "cursorFirst" }
    var stamp = JSON.stringify([root.path, root.cursorIndex, root.selectionVersion, root.viewMode])
    var paired = pairs[action] && root.keySequence === action && root.keySequenceIdentity === stamp
    root.keySequence = paired || !pairs[action] ? "" : action
    root.keySequenceIdentity = paired || !pairs[action] ? "" : stamp
    return paired ? pairs[action] : pairs[action] ? "" : action
}

// Lifted whole from Pane.qml's Keys.onPressed, which had grown past its file's 400-line cap; returns whether the key was consumed.
function handleKey(event, root, sidebar) {
    if (root.selectionBand) {
        if (event.key === Qt.Key_Escape)
            root.selectionBand.cancel()
        return true
    }
    // Guards a key that reaches the list before a rename field's own focus transfer lands, the OEM's
    // "blocked:" lesson; the row editor and the rail's own field both need it. The index alone is not
    // asked, because an editor released by a scroll or hidden by a view change left it set with
    // nothing to give the keys to, and every later key was swallowed for the life of the window.
    if (sidebar.renameEditor() !== null || root.renameEditor() !== null) {
        return true
    }
    root.inputAt = Date.now()
    root.rowsAt = 0
    // Issue 12: a query line owns every key while it has the caret, which swallowed the cursor keys
    // and left a listing with more than one match unreachable from the keyboard. A cursor key commits
    // the line the way enter does and then goes on to mean what it means everywhere else: the filter
    // is left standing over the rows it narrowed, and the search walks once for that press rather
    // than once per keystroke, which is the sweep the design refused.
    if ((root.searchMode === Search.TYPING || root.filterTyping) && leavesLine(event)) {
        if (root.filterTyping) Filter.commit(root)
        else Search.run(root)
    }
    // The query line owns every key while it has the caret, the same way the rename field does above.
    if (root.searchMode === Search.TYPING) {
        return Search.typeKey(event, root)
    }
    // The filter's query line owns every key while it has the caret, the same as the search's above.
    if (root.filterTyping) {
        return Filter.typeKey(event, root)
    }
    var action = lookup(event, root)
    action = sequenceAction(action, root)
    // Anything that is not the second d of the pair disarms it, so an arm never outlives the key
    // after it; ui/js/Trash.js re-stamps on its own, which is why it reads the stamp before writing.
    if (action !== "trashArm") {
        root.trashArmedAt = 0
    }
    if (root.preview.active) {
        PreviewKeys.act(action, root)
        return true
    }
    if (shareBrowserHere(root)) {
        shareBrowserAct(action, root)
        return true
    }
    if (action === "focusNext" || action === "focusPrevious") {
        if (root.dualMode && root.focusView === LIST) root.switchPane()
        else root.focusView = next(root.focusView)
        return true
    }
    if (action === "focusPreview") {
        if (!root.dualMode) root.focusPreviewColumn()
        return true
    }
    // The sheet is global, unlike addNetwork, so it answers from the rail as well as the list. It
    // takes active focus itself, so nothing below has to route keys into it while it stands.
    if (action === "keymapSheet") {
        root.keymapSheet.open(root)
        return true
    }
    // Tabs are window-level, so t, w and the digits answer from the rail as well as the list.
    if (action.indexOf("tab") === 0) {
        root.act(action)
        return true
    }
    // Issue 9: the text size belongs to the window, so it answers from either view.
    if (action.indexOf("textSize") === 0) {
        root.textSizeRequested(action === "textSizeReset" ? 0 : (action === "textSizeUp" ? 1 : -1))
        return true
    }
    // The bar lives in the chrome above both views, so neither owns it; shell.qml holds the field.
    if (action === "pathBar") {
        root.pathBarRequested()
        return true
    }
    // These answer from the rail as well as the list, so they are taken before the rail's own keys.
    if (action === "openTerminal" || action === "settings" || action === "copydirpath") {
        root.act(action)
        return true
    }
    if (root.focusView === RAIL) {
        RailKeys.act(action, root, sidebar)
        return true
    }
    if (gridArrow(event, action, root)) return true
    if (action.length > 0 || Keymap.lookup(event.key, event.text, event.modifiers).length > 0) {
        if (action.length > 0) root.act(action)
        return true
    }
    // An unbound printable key used to jump to a name, which only half worked because most letters
    // are bound, and taught a habit that reached d and trashed the row. It names the filter instead.
    if (root.shown === null && event.text.length === 1 && event.text >= " ") {
        root.message("Press / to filter this listing by name.", false)
        return true
    }
    return false
}
