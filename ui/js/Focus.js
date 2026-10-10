.pragma library
.import "Input.js" as Input
.import "Eject.js" as Eject
.import "Filter.js" as Filter
.import "Grid.js" as Grid
.import "Format.js" as Format
.import "Keymap.js" as Keymap
.import "Marks.js" as Marks
.import "Mounts.js" as Mounts
.import "Ops.js" as Ops
.import "PreviewKeys.js" as PreviewKeys
.import "RailKeys.js" as RailKeys
.import "Reload.js" as Reload
.import "RecentMode.js" as RecentMode
.import "Status.js" as Status
.import "Search.js" as Search
.import "Sort.js" as Sort
.import "Swap.js" as Swap
.import "TextSize.js" as TextSize
.import "Trash.js" as Trash
.import "Tabs.js" as Tabs

var LIST = "list"
var RAIL = "rail"

// Refusal where gio has no Trash, from ui/js/Status.js, which recognises it to drop it when the pane leaves that place; a preset with no key reads bare.
var NO_TRASH = Status.NO_TRASH
function noTrashHint() { return Status.trashHint().replace(" · ", "") }
function noTrashLine() { return Status.noTrashLine() }

// Tab is the only thing that moves focus between views, so the rule lives in one function.
function next(current, sidebar) {
    // RailAdditions rule 4: a hidden rail is not a place the keyboard can go, so Tab stays in the list. Directive 77: one that auto-hide withdrew is, because arriving there is what reveals it.
    return current === LIST && sidebar ? RAIL : LIST
}

// Hiding the rail while it holds the keyboard strands focusView, so the listing takes the logical view and the actual focus together.
function railHidden(root) {
    // Both panes share the hidden state, so only the active one answers it; undefined stays compatible with fixtures that predate the flag.
    if (root.paneFocused === false || root.focusView !== RAIL) return
    root.focusView = LIST
    if (root.listArea) root.listArea.forceActiveFocus()
}

function shareBrowserHere(root) {
    return !!(root.shareBrowser && root.shareBrowser.active && (!root.shareBrowser.owner || root.shareBrowser.owner === root))
}

// The one gate the z key and the "z undoes" click share: no undo under a listing, rename, overlay or query line.
function canUndo(pane, sidebar) {
    if (!pane || Swap.swallows(pane.listInFlight, "undo"))
        return false
    if (pane.selectionBand !== null || pane.renameEditor() !== null)
        return false
    if (sidebar && sidebar.renameEditor() !== null)
        return false
    if (pane.searchMode === Search.TYPING || pane.filterTyping)
        return false
    if ((pane.preview && pane.preview.active) || shareBrowserHere(pane))
        return false
    if (pane.menuVisible || (pane.menuActions && pane.menuActions.opened))
        return false
    if ((pane.collide && (pane.collide.opened || pane.collide.pending !== null))
            || (pane.keymapSheet && pane.keymapSheet.opened)
            || (pane.settingsPanel && pane.settingsPanel.opened))
        return false
    return true
}

// The one lookup Pane.qml's Keys.onPressed calls. "addNetwork" is a rail-only action (the
// dialog is reached from the rail's own "+" mark), so "a" does nothing in the list;
// filtering it here, not in Keymap.js, keeps the generated file a pure keys.toml mirror.
// seekBack/seekForward get the same treatment, scoped to an open MEDIA preview instead of the rail.
function lookup(event, root) {
    var context = (root.preview && root.preview.active) ? (root.preview.isPdf ? "pdf" : root.preview.isMedia ? "media" : "preview")
                  : shareBrowserHere(root) ? "menu" : root.focusView === RAIL ? "rail" : "listing"
    // Grid arrows address visual neighbours even when the preset uses them to open folders in List.
    // Issue 114, muellan: h and l are the arrows spelled as letters, so in the grid they mean what
    // the arrows mean and not the tree's own pair, which is what the eye reads off a row of tiles.
    if (context === "listing" && root.viewMode === "grid" && event.modifiers === Qt.NoModifier) {
        if (Grid.sideways(event) < 0) return "cursorLeft"
        if (Grid.sideways(event) > 0) return "cursorRight"
    }
    var action = Keymap.lookup(event.key, event.text, event.modifiers, context)
    // The share listing borrows the menu context for j/k/enter, but it has no submenu to step into.
    if (action === "menuRight" && shareBrowserHere(root)) return "open"
    // List and Grid filter held rows; search owns the header while its results are active.
    if (action === "filter")
        return (root.viewMode !== "columns" && root.searchMode.length === 0) ? action : ""
    // Left and Right seek inside a media preview and turn the page in a PDF one, which is the only
    // place the map binds either action now that the browsing pair is parent and browse-in; the grid
    // takes the bare arrows above, before the map is consulted.
    if (action === "seekBack" || action === "seekForward")
        return root.preview && root.preview.active && (root.preview.isMedia || root.preview.isPdf) ? action : ""
    // Minus, plus and e mean nothing outside a PDF. l is h's forward: page, else enter or preview.
    if (action === "zoomOut" || action === "zoomIn" || action === "expand")
        return (root.preview && root.preview.active && root.preview.isPdf) ? action : ""
    if (action === "pageForward") {
        if (root.preview && root.preview.active)
            return root.preview.isPdf ? action : ""
        if (shareBrowserHere(root) || root.focusView === RAIL)
            return "open"
        var row = root.rowFor(root.cursorIndex)
        return row && (row.d || (Format.isSymlink(row.p) && row.i === "folder")) ? "open" : (row ? "preview" : "")
    }
    // The key follows the row: where gio has no Trash the refusal names trashRefused, never arming a d that can only fail.
    if ((action === "trashArm" || action === "trash") && !Mounts.trashable(root.path, !root.backend || root.backend.dirWritable !== false))
        return "trashRefused"
    // reveal only means something on a search result or a recent row, so o is discarded everywhere else.
    if (action === "reveal" && root.searchMode !== Search.RESULTS && root.recentMode !== RecentMode.RESULTS)
        return ""
    // A sort ends the running walk in the backend and the search strip hides the mark that would
    // show it happening, so both sort keys go quiet for as long as a search owns the header.
    if (action === "sortNext" || action === "sortReverse")
        return root.searchMode.length === 0 ? action : ""
    // A reload re-lists the folder it is on, so it goes quiet where a walk owns the header.
    if (action === "reload" && root.searchMode.length > 0)
        return ""
    return action
}

// Lifted from Pane.qml's Keys.onPressed: the map holds the keys, this holds the behaviour.
// Takes the Pane root because every case is a method call or a property read on it.
function act(action, root, menuId, paths) {
    switch (action) {
    // List steps follow item order; in the grid ui/js/Grid.js takes j/k and the arrows as visual cells.
    case "cursorDown": step(root, 1); Marks.follow(root); return
    case "cursorUp": step(root, -1); Marks.follow(root); return
    case "cursorLeft": step(root, -1); Marks.follow(root); return
    case "cursorRight": step(root, 1); Marks.follow(root); return
    case "cursorFirst": Filter.setCursorView(root, 0); Marks.follow(root); return
    case "cursorLast": Filter.setCursorView(root, root.shownTotal - 1); Marks.follow(root); return
    case "pageDown": step(root, Math.max(1, Math.floor(root.visibleRows / 2))); Marks.follow(root); return
    case "pageUp": step(root, -Math.max(1, Math.floor(root.visibleRows / 2))); Marks.follow(root); return
    case "open": root.openCursor(paths); return
    case "openTab": Tabs.openCursorTab(root); return
    case "parent": root.openParent(); return
    case "historyBack": root.goBack(); return
    case "historyForward": root.goForward(); return
    case "togglePreview": root.togglePreviewColumn(); return
    case "loadPreview": root.loadSelectionPreview(); return
    case "focusPreview": root.focusPreviewColumn(); return
    case "windowNew": root.newWindow(); return
    case "toggleHidden": root.toggleHidden(); return
    // ExtThumbs: the background menu's class row and any future chord land here.
    case "extThumbs": root.toggleExtThumbs(); return
    case "sidebar": root.toggleRail(); return
    // Popups handle Escape first; the focused listing then unwinds filter, search, status and marks.
    case "escape":
        if (root.filterTyping || root.filterQuery.length > 0) Filter.close(root)
        else if (root.searchMode.length > 0 && root.focusView === LIST) Search.cancel(root)
        else if (root.recentMode.length > 0 && root.focusView === LIST) { if (root.listInFlight) root.message("A directory is already loading.", false); else RecentMode.close(root) }
        else if (root.statusBar && root.statusBar.escapePressed()) return
        else if (escapeUp(root)) root.openParent()
        else root.escapePressed()
        return
    case "reload": Reload.begin(root, root.wire); return
    case "preview": PreviewKeys.open(root); return
    case "toggleSelect": root.toggleSelect(); return
    case "extendDown": root.extendSelection(1); return
    case "extendUp": root.extendSelection(-1); return
    case "selectAll": root.selectAll(); return
    // A walk replaces the listing the filter was narrowing, so the filter goes before the query line
    // does: leaving it up would hide every result that did not happen to match it.
    case "search": Filter.close(root); Search.start(root); return
    case "filter": Filter.start(root); return
    case "reveal": if (root.recentMode.length > 0) { if (root.listInFlight) root.message("A directory is already loading.", false); else RecentMode.reveal(root) } else Search.reveal(root); return
    // The write operations; every one of them is reversible with undo, so none of them confirms.
    case "duplicate": Ops.duplicate(root, menuId); return
    case "trash": Ops.trash(root, menuId); return
    case "trashArm": Trash.arm(root); return
    // A trash key where there is no Trash refuses in the error role, hinting the key that still works.
    case "trashRefused": root.message(noTrashLine(), true); return
    case "copy": Ops.clip(root, false, paths); return
    case "copydirpath": root.copyDirPath(); return
    // MenuAdditions040: c copies as, P pastes as, V flips the selection.
    case "copyAs": root.openCopyAs(); return
    case "pasteAs":
        if (root.recentMode.length > 0) { root.message("This listing is a history, and cannot take a paste.", false); return }
        root.openPasteAs(); return
    case "invertSelection": root.invertSelection(); return
    case "showOriginal": root.showOriginal(); return
    case "makeExecutable": root.makeExecutable(paths); return
    case "copyPath": Ops.copyAs(root, "path", paths); return
    case "copyName": Ops.copyAs(root, "name", paths); return
    case "copyStem": Ops.copyAs(root, "stem", paths); return
    case "copyUri": Ops.copyAs(root, "uri", paths); return
    case "copyQuoted": Ops.copyAs(root, "quoted", paths); return
    case "pasteLink":
    case "pasteAbsoluteLink":
    case "pasteHardLink":
        if (RecentMode.refusePaste(root)) return
        root.pasteLink(action === "pasteAbsoluteLink" ? "absolute" : action === "pasteHardLink" ? "hard" : "relative"); return
    case "cut": Ops.clip(root, true, paths); return
    // Recent is a history, not a directory: pasting or creating there would land in the root it stands on.
    case "paste": if (root.recentMode.length > 0) { root.message("This listing is a history, and cannot take a paste.", false); return } Ops.paste(root); return
    case "movePaste":
        if (root.recentMode.length > 0) { root.message("This listing is a history, and cannot take a paste.", false); return }
        Ops.paste(root, true)
        return
    case "undo": Ops.undo(root); return
    case "redo": root.backend.send({c: "redo"}); return
    // m opens the row menu, or says why no row was under the cursor.
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
    // F4 fits every drawn list column; anywhere but the list view it says where that lives.
    case "autofitColumns":
        if (root.viewMode === "list") root.header.autofitAll()
        else root.message("Autofit needs the list view.", false)
        return
    case "addNetwork": if (root.sidebar) root.sidebar.addRequested(); return
    case "eject": Eject.release(root, root.sidebar, false); return
    // Finder's Cmd+1/2/3; the chrome's three buttons write the same property, so they follow.
    case "viewList": root.chooseView("list"); return
    case "viewColumns": root.chooseView("columns"); return
    case "viewGrid": root.chooseView("grid"); return
    case "newFolder": if (root.recentMode.length > 0) { root.message("This listing is a history, and cannot take a new folder.", false); return } Ops.newFolder(root); return
    // The directory being shown, not the row: the menu row and the chord both land here.
    case "openTerminal": root.openTerminal(); return
    // The background menu's Update Flea row, drawn only while an update is known, opens Omarchy's updater through the pane's opener.
    case "updateFlea": root.opener.updateFlea(root); return
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
    // Tabs run here and handleKey opens the path bar, so neither answers unbuilt any more.
    if (action.indexOf("tab") === 0) { Tabs.act(action, root); return }
    root.message(action + " is not built yet.", false)
}

// Issue 29: Escape climbs only when nothing else owns it, so each unwound state keeps the key.
function escapeUp(root) {
    return root.escapeUp === true && root.searchMode.length === 0
        && root.filterQuery.length === 0 && !root.filterTyping && !root.menuVisible
        && !(root.collide && root.collide.opened) && !hasDeliberateMarks(root) && !root.listInFlight
}

// Only the row a navigation landed on never counts as a selection, so Escape keeps climbing.
function hasDeliberateMarks(root) {
    if (root.selectionCount() === 0)
        return false
    if (root.selection && root.selection.isLanded && root.selection.isLanded())
        return false
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
    if (Input.isPrintable(event.text))
        return false
    return LEAVES_LINE.indexOf(Keymap.lookup(event.key, event.text, event.modifiers)) >= 0
}

// One stamp for a vim pair's seat, so Escape and the pair read the same identity.
function stampOf(root) {
    return JSON.stringify([root.path, root.cursorIndex, root.selectionVersion, root.viewMode])
}
// Only the second press of the same pair on the same selection fires.
var ARMED_PAIRS = { copyArm: true, cutArm: true, pasteArm: true, cursorFirstArm: true }
// Vim pairs are consecutive inputs on the same selection; pointer or navigation changes disarm them.
function sequenceAction(action, root) {
    var pairs = { copyArm: "copy", cutArm: "cut", pasteArm: "paste", cursorFirstArm: "cursorFirst" }
    var stamp = stampOf(root)
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
    // A live rename editor owns keys; an index left behind by a hidden or recycled editor does not.
    if ((sidebar && sidebar.renameEditor() !== null) || root.renameEditor() !== null) {
        return true
    }
    root.inputAt = Date.now()
    root.rowsAt = 0
    // Cursor keys commit a query before acting on its results; search walks once per commit.
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
    // A key that means an action ends a pending slow click, so its rename never lands after that key's result; a bare modifier means none.
    if (action.length > 0 && root.cancelSlowClick) root.cancelSlowClick()
    // With escape-up on Escape cancels an armed trash or a live vim pair and stops, with it off Escape disarms through the sequence and runs its own action.
    var escapeCancelsArm = action === "escape" && root.escapeUp === true
        && (root.trashArmedAt > 0 || (ARMED_PAIRS[root.keySequence] === true && root.keySequenceIdentity === stampOf(root)))
    action = sequenceAction(action, root)
    // Anything that is not the second d of the pair disarms it, so an arm never outlives the key
    // after it; ui/js/Trash.js re-stamps on its own, which is why it reads the stamp before writing.
    if (action !== "trashArm") {
        root.trashArmedAt = 0
    }
    if (root.preview && root.preview.active) {
        PreviewKeys.act(action, root)
        return true
    }
    if (shareBrowserHere(root)) {
        shareBrowserAct(action, root)
        return true
    }
    if (windowAction(action, root, sidebar)) return true
    if (root.focusView === RAIL && sidebar) {
        RailKeys.act(action, root, sidebar)
        return true
    }
    // An armed pair keeps Escape to cancel the arm instead of climbing.
    if (action === "escape" && escapeCancelsArm) {
        root.trashArmedAt = 0
        root.keySequence = ""
        root.keySequenceIdentity = ""
        root.message("", false)
        return true
    }
    // No row action runs against a listing still in flight.
    if (Swap.swallows(root.listInFlight, action)) { root.message(Swap.LOADING, false); return true }
    if (action === "undo" && !canUndo(root, sidebar)) return true
    if (Grid.arrow(event, action, root)) return true
    if (action.length > 0 || Keymap.lookup(event.key, event.text, event.modifiers).length > 0) {
        if (action.length > 0) dispatchAction(action, root)
        return true
    }
    // Unbound printable keys name the filter instead of jumping into destructive bindings.
    if (root.shown === null && Input.isPrintable(event.text)) {
        root.message("Press / to filter this listing by name.", false)
        return true
    }
    return false
}

// Keys and query actions share the window's dispatcher before reaching pane actions.
function dispatchAction(action, root) {
    if (!windowAction(action, root)) root.act(action)
}

function windowAction(action, root, sidebar) {
    if (action === "focusNext" || action === "focusPrevious") {
        if (root.dualMode && root.focusView === LIST) root.switchPane()
        else root.focusView = next(root.focusView, sidebar || root.sidebar || root.railAvailable)
        return true
    }
    if (action === "focusPreview") {
        if (!root.dualMode) root.focusPreviewColumn()
        return true
    }
    if (action === "keymapSheet") {
        root.keymapSheet.open(root)
        return true
    }
    // Tabs answer from either view.
    if (action.indexOf("tab") === 0) {
        root.act(action)
        return true
    }
    // Issue 9: the text size belongs to the window, so it answers from either view.
    if (action.indexOf("textSize") === 0) {
        root.textSizeRequested(TextSize.direction(action))
        return true
    }
    // The chrome owns the path field.
    if (action === "pathBar") {
        root.pathBarRequested()
        return true
    }
    // These answer from the rail as well as the list, so they are taken before the rail's own keys.
    if (action === "windowNew" || action === "reload" || action === "sidebar" || action === "openTerminal" || action === "settings" || action === "copydirpath") {
        root.act(action)
        return true
    }
    return false
}
