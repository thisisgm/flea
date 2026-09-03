.pragma library

.import "Eject.js" as Eject
.import "Filter.js" as Filter
.import "Keymap.js" as Keymap
.import "Mounts.js" as Mounts
.import "Ops.js" as Ops
.import "Search.js" as Search
.import "Sort.js" as Sort

var LIST = "list"
var RAIL = "rail"

// Left/Right's seek step, Task 22's operator ruling; previewAct is the only reader.
var SEEK_MS = 5000

// Tab is the only thing that moves focus between views, so the rule lives in one function.
function next(current) {
    return current === LIST ? RAIL : LIST
}

// The one lookup Pane.qml's Keys.onPressed calls. "addNetwork" is a rail-only action (the
// dialog is reached from the rail's own "+" mark), so the list keeps "a" for type-ahead;
// filtering it here, not in Keymap.js, keeps the generated file a pure keys.toml mirror.
// seekBack/seekForward get the same treatment, scoped to an open MEDIA preview instead of the
// rail, so Left/Right stay silent everywhere else rather than reaching act()'s "not built yet".
function lookup(event, root) {
    var action = Keymap.lookup(event.key, event.text, event.modifiers)
    // Only the bare a is rail-only; Ctrl+K costs no type-ahead letter and opens the dialog anywhere.
    if (action === "addNetwork" && root.focusView !== RAIL && !(event.modifiers & Qt.ControlModifier))
        return ""
    // A filter narrows rows already on screen, which is the list view's own job: the GridView and
    // Columns boards draw no filter, so / is dropped there rather than narrowing a view nothing shows.
    // It goes quiet while a search owns the header too, the same way the two sort keys do below.
    if (action === "filter")
        return (root.viewMode === "list" && root.searchMode.length === 0) ? action : ""
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
    // The PDF viewer's own four. Minus, plus, e and l mean nothing anywhere else, so they stay
    // silent rather than reaching act()'s "not built yet" while browsing.
    if (action === "zoomOut" || action === "zoomIn" || action === "expand" || action === "pageForward")
        return (root.preview.active && root.preview.isPdf) ? action : ""
    // reveal only means something on a search result, so o stays a type-ahead key everywhere else.
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
function act(action, root) {
    switch (action) {
    // One row in the list, one row of tiles in the grid: a grid that stepped linearly on Down would
    // move the cursor sideways, which is not what the key looks like it does.
    // Every one of these moves through what is drawn, not through the listing: with a filter up the
    // two differ, and stepping the listing would land the cursor on a row nothing is showing.
    case "cursorDown": Filter.moveCursor(root, root.cursorStride); return
    case "cursorUp": Filter.moveCursor(root, -root.cursorStride); return
    case "cursorLeft": Filter.moveCursor(root, -1); return
    case "cursorRight": Filter.moveCursor(root, 1); return
    case "cursorFirst": Filter.setCursorView(root, 0); return
    case "cursorLast": Filter.setCursorView(root, root.shownTotal - 1); return
    case "pageDown": Filter.moveCursor(root, Math.max(1, Math.floor(root.visibleRows / 2))); return
    case "pageUp": Filter.moveCursor(root, -Math.max(1, Math.floor(root.visibleRows / 2))); return
    case "open": root.openCursor(); return
    case "parent": root.openParent(); return
    case "toggleHidden": root.toggleHidden(); return
    // Esc unwinds one thing at a time, and the least destructive first: a running walk, then the
    // search, then the filter (which loses nothing), then the selection, then the transient line.
    case "escape":
        if (root.searchMode.length > 0) Search.cancel(root)
        else if (root.filterTyping || root.filterQuery.length > 0) Filter.close(root)
        else root.escapePressed()
        return
    case "preview": openPreview(root); return
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
    case "duplicate": Ops.duplicate(root); return
    case "trash": Ops.trash(root); return
    case "copy": Ops.clip(root, false); return
    case "cut": Ops.clip(root, true); return
    case "paste": Ops.paste(root); return
    case "undo": Ops.undo(root); return
    case "rename": Ops.startRename(root); return
    // m. The rail's raiseMenu says why a favourite has no menu; here the pane says whether a row was
    // under the cursor at all, and an empty or fully filtered listing gets the sentence, not silence.
    case "menu":
        if (!root.openCursorMenu())
            root.message("No row under the cursor to open a menu on.", false)
        return
    case "extract": Ops.extract(root); return
    case "dropbox": root.moveToDropbox(); return
    case "sharelink": root.copyShareLink(); return
    // Convert opens the one popup this whole design has; every other operation answers without one.
    case "convert": root.openConvert(); return
    // The header answers the same two through ui/Pane.qml, so the key and the click share one route.
    case "sortNext": Sort.next(root); return
    case "sortReverse": Sort.reverse(root); return
    case "addNetwork": root.sidebar.addRequested(); return
    case "eject": Eject.release(root, root.sidebar, false); return
    // Finder's Cmd+1/2/3; the chrome's three buttons write the same property, so they follow.
    case "viewList": root.viewMode = "list"; return
    case "viewColumns": root.viewMode = "columns"; return
    case "viewGrid": root.viewMode = "grid"; return
    case "newFolder": Ops.newFolder(root); return
    }
    // A submenu row fires "<action>:<id>", which is how one signal covers Taildrop and Compress both.
    if (action.indexOf("compress:") === 0) {
        Ops.compress(root, action.substring("compress:".length))
        return
    }
    // Bound ahead of its feature, by the rule that a mapped key says why rather than doing nothing:
    // the Tui board draws tabs on t, w and the digits. The path bar that shared this line is built,
    // and handleKey below opens it before the pane's own views ever see the key.
    if (action.indexOf("tab") === 0) { root.message("Tabs are not built yet.", false); return }
    root.message(action + " is not built yet.", false)
}

// A directory has no preview kind of its own, so Space on one is a silent no-op rather than an error.
function openPreview(root) {
    var row = root.rowFor(root.cursorIndex)
    if (row && !row.d)
        root.preview.open(root.join(root.path, row.n), row.i, row.s)
}

// Preview open: j/k move the cursor and the preview follows; escape always closes. Space closes
// a text or unsupported preview as before, but toggles play/pause on a MEDIA one instead
// (Task 22's operator ruling: "the idea is our preview is as good or better than Showtime").
// Any key reveals the media strip, even one that does nothing else, matching "move the mouse or
// press anything" from the same ruling.
function previewAct(action, root) {
    root.preview.revealStrip()
    switch (action) {
    case "cursorDown": Filter.moveCursor(root, 1); followPreview(root); return
    case "cursorUp": Filter.moveCursor(root, -1); followPreview(root); return
    case "preview":
        if (root.preview.isMedia) root.preview.togglePlay()
        else root.preview.close()
        return
    case "escape": root.preview.close(); return
    case "seekBack":
        if (root.preview.isPdf) root.preview.turnPage(-1)
        else root.preview.seek(-SEEK_MS)
        return
    case "seekForward":
        if (root.preview.isPdf) root.preview.turnPage(1)
        else root.preview.seek(SEEK_MS)
        return
    // h keeps its own "parent" name from keys.toml; turnPage self-guards, so a media preview
    // ignores both of these rather than seeking on a key the strip never advertised.
    case "parent": root.preview.turnPage(-1); return
    case "pageForward": root.preview.turnPage(1); return
    case "zoomOut": root.preview.zoomBy(-1); return
    case "zoomIn": root.preview.zoomBy(1); return
    case "expand": root.preview.toggleExpand(); return
    }
}

// The row under the moved cursor, handed to Preview.follow so a held key settles before it reloads.
function followPreview(root) {
    var row = root.rowFor(root.cursorIndex)
    if (row && !row.d)
        root.preview.follow(root.join(root.path, row.n), row.i, row.s)
}

// ui/ShareBrowser.qml's own overlay, the same j/k/open/escape shape previewAct above uses.
function shareBrowserAct(action, root) {
    switch (action) {
    case "cursorDown": root.shareBrowser.moveCursor(1); return
    case "cursorUp": root.shareBrowser.moveCursor(-1); return
    case "open": root.shareBrowser.activateCursor(); return
    case "escape": root.shareBrowser.close(); return
    }
}

// Lifted whole from Pane.qml's Keys.onPressed, which had grown past its file's 400-line cap; returns whether the key was consumed.
function handleKey(event, root, sidebar) {
    // Guards a key that reaches the list before a rename field's own focus transfer lands, the OEM's
    // "blocked:" lesson; the row editor and the rail's own field both need it. The index alone is not
    // asked, because an editor released by a scroll or hidden by a view change left it set with
    // nothing to give the keys to, and every later key was swallowed for the life of the window.
    if (sidebar.renameEditor() !== null || root.renameEditor() !== null) {
        return true
    }
    root.inputAt = Date.now()
    root.rowsAt = 0
    // The query line owns every key while it has the caret, the same way the rename field does above.
    if (root.searchMode === Search.TYPING) {
        return Search.typeKey(event, root)
    }
    // The filter's query line owns every key while it has the caret, the same as the search's above.
    if (root.filterTyping) {
        return Filter.typeKey(event, root)
    }
    var action = lookup(event, root)
    if (root.preview.active) {
        previewAct(action, root)
        return true
    }
    if (root.shareBrowser.active) {
        shareBrowserAct(action, root)
        return true
    }
    if (action === "focusNext") {
        root.focusView = next(root.focusView)
        return true
    }
    // The sheet is global, unlike addNetwork, so it answers from the rail as well as the list. It
    // takes active focus itself, so nothing below has to route keys into it while it stands.
    if (action === "keymapSheet") {
        root.keymapSheet.open(root)
        return true
    }
    // The path bar is global for the same reason, and for one more: it lives in the chrome above
    // both views, so neither the list nor the rail is the view that owns it. The pane only asks for
    // it; ui/shell.qml owns the field and hands the keyboard back when it closes.
    if (action === "pathBar") {
        root.pathBarRequested()
        return true
    }
    if (root.focusView === RAIL) {
        root.railAct(action)
        return true
    }
    if (action.length > 0) {
        root.act(action)
        return true
    }
    // Type-ahead is the fallback, so any printable key that is not bound jumps. A standing filter is
    // already a name match over these rows, so the two never run at once and never disagree.
    if (root.shown === null && event.text.length === 1 && event.text >= " ") {
        root.typeAhead(event.text)
        return true
    }
    return false
}

// The keyboard's route into the rail menu, and where a rail row without one is answered: the sheet
// advertises m, so a key that silently did nothing on a favourite would read as a broken one.
function raiseMenu(root, sidebar) {
    var entry = sidebar.entries[sidebar.cursorIndex]
    if (!entry)
        return
    if (Mounts.railMenu(entry).length > 0)
        sidebar.openCursorMenu()
    else
        root.message(entry.label + " has nothing to eject or unmount.", false)
}

// The rail answers seven of the key table's action names and ignores the rest while it has focus.
function railAct(action, root, sidebar) {
    switch (action) {
    case "cursorDown": sidebar.cursorIndex = Math.min(sidebar.entries.length - 1, sidebar.cursorIndex + 1); return
    case "cursorUp": sidebar.cursorIndex = Math.max(0, sidebar.cursorIndex - 1); return
    // activate(), not a direct opened(path): a Network entry may need mounting first.
    case "open": if (sidebar.entries.length > 0) sidebar.activate(sidebar.cursorIndex); return
    case "escape": root.focusView = LIST; return
    case "addNetwork": sidebar.addRequested(); return
    // Favorites are not offered: Sidebar.startRename ignores an index outside the Network group.
    case "rename": sidebar.startRename(sidebar.cursorIndex); return
    // Eject and Unmount are menu rows, so this opens the menu rather than inventing a second route.
    case "menu": raiseMenu(root, sidebar); return
    case "eject": Eject.release(root, sidebar, true); return
    }
}
