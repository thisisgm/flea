.pragma library

.import "Filter.js" as Filter
.import "Keymap.js" as Keymap
.import "Ops.js" as Ops
.import "PickerOps.js" as PickerOps
.import "RailKeys.js" as RailKeys

// What the chooser does with a key. Two tables, in order: the picker's own verbs, which were an
// if-chain in ui/PickerList.qml, and then keys.toml through ui/js/Keymap.js for the operations the
// picker shares with the browser window, so a preset's rebinding (the windows preset's Ctrl+H)
// applies here too. Any other key answers "" and the event stays unaccepted. Tab swaps the surface
// the keys reach, and while it is the rail a third table below answers instead of the two. Pure,
// so tests/js/pickerkeys.js drives lookup, act and handle.

// ui/js/Focus.js's two surface names, written out because importing it would pull the browser's
// whole key dispatch, ui/js/Ops.js included, into a chooser that carries none of it.
var LIST = "list"
var RAIL = "rail"

// The shared actions the picker takes from the key table, each with the name the footer prints
// while its verb is not built yet; toggleHidden and selectAll have one. An action outside this
// table never reaches act.
var SHARED = {
    copy: "Copy", cut: "Cut", paste: "Paste", selectAll: "Select all", trash: "Move to Trash",
    rename: "Rename", newFolder: "New folder", openTerminal: "Open in terminal",
    copydirpath: "Copy folder path", undo: "Undo", toggleHidden: "Hidden files",
    filter: "Filter", focusNext: "Focus"
}

function lookup(event, state) {
    if (state.focusView === RAIL)
        return railVerb(event)
    var own = ownVerb(event, state)
    if (own.length > 0)
        return own
    var shared = Keymap.lookup(event.key, event.text, event.modifiers)
    if ((shared === "copy" || shared === "cut" || shared === "paste")
            && event.modifiers !== Qt.ControlModifier)
        return ""
    return shared in SHARED ? shared : ""
}

// The picker's own keys. A Ctrl chord belongs to the key table except Ctrl+L, so the windows
// preset's Ctrl+H reaches toggleHidden instead of the bare h below. Escape unwinds one thing at a
// time, the least destructive first: a fetch in flight, then a standing filter (which loses
// nothing), then the dialog, ui/picker.qml's fetch-then-field-then-window order.
function ownVerb(event, state) {
    var key = event.key
    var mods = event.modifiers
    // Both platform delete chords are fixed picker verbs. Ctrl+Delete otherwise exists only in
    // the mac preset, while an OS picker must accept it under every preset.
    if (key === Qt.Key_Delete && (mods === Qt.NoModifier || mods === Qt.ControlModifier))
        return "trash"
    if (mods & Qt.ControlModifier)
        return key === Qt.Key_L ? "location" : ""
    if (key === Qt.Key_Left)
        return (mods & Qt.AltModifier) ? "back" : ""
    switch (key) {
    case Qt.Key_Escape:
        return state.fetching ? "stopFetch" : state.filterQuery.length > 0 ? "filterClose" : "cancel"
    case Qt.Key_Down: case Qt.Key_J: return "cursorDown"
    case Qt.Key_Up: case Qt.Key_K: return "cursorUp"
    case Qt.Key_PageDown: return "pageDown"
    case Qt.Key_PageUp: return "pageUp"
    case Qt.Key_Home: return "cursorFirst"
    case Qt.Key_End: return "cursorLast"
    case Qt.Key_Space: return "mark"
    case Qt.Key_Return: case Qt.Key_Enter: case Qt.Key_L: return "activate"
    case Qt.Key_Backspace: case Qt.Key_H: return "parent"
    }
    return event.text === ":" ? "location" : ""
}

// The rail's keys, the six of ui/js/RailKeys.js's ten actions a chooser's rail can answer: the
// other four rename, mount and eject through members only ui/Sidebar.qml has. Tab still swaps
// back through the key table, and anything else is nobody's while the rail has the keyboard, so
// Space cannot mark a list row the person is not looking at. Home and End are named as the list
// names them, and j and k walk the rail as they walk the list.
function railVerb(event) {
    switch (event.key) {
    case Qt.Key_Down: case Qt.Key_J: return "cursorDown"
    case Qt.Key_Up: case Qt.Key_K: return "cursorUp"
    case Qt.Key_Home: return "cursorFirst"
    case Qt.Key_End: return "cursorLast"
    case Qt.Key_Return: case Qt.Key_Enter: return "open"
    case Qt.Key_Escape: return "escape"
    }
    var action = Keymap.lookup(event.key, event.text, event.modifiers)
    return action === "focusNext" || action === "openTerminal" || action === "copydirpath" ? action : ""
}

// The dispatcher. state is ui/PickerState.qml; ops is ui/PickerList.qml, for the cursor moves
// that need the viewport and the field ":" hands the keyboard to. A shared action with no verb
// here yet says so in the footer, so a key never falls silent.
function act(action, state, ops) {
    // Tab, ui/js/Focus.js's own rule: the only key that moves between the two surfaces.
    if (action === "focusNext") {
        state.focusView = state.focusView === LIST ? RAIL : LIST
        return
    }
    // These belong to the directory the window shows, so the list and the rail both answer them.
    // Recent is a history token and must never be handed to an external process or the clipboard.
    if (action === "openTerminal") {
        state.openTerminal()
        return
    }
    if (action === "copydirpath") {
        if (state.recent)
            state.message("Recent has no folder path to copy.", false)
        else
            state.copyText(state.path)
        return
    }
    // The rail answers through RailKeys unmodified, on the members ui/PickerPlaces.qml shares with
    // ui/Sidebar.qml; escape writes focusView back to the list and open goes through the rail's own
    // activate, whose chosen handler in ui/picker.qml returns the keyboard to the list too.
    if (state.focusView === RAIL) {
        RailKeys.act(action, state, state.places)
        return
    }
    switch (action) {
    case "filter": Filter.start(state); return
    case "filterClose": Filter.close(state); return
    case "cancel": state.cancel(); return
    case "stopFetch": state.fetcher.cancel(); return
    case "cursorDown": ops.moveCursor(1); return
    case "cursorUp": ops.moveCursor(-1); return
    case "pageDown": ops.moveCursor(ops.visibleRows); return
    case "pageUp": ops.moveCursor(-ops.visibleRows); return
    case "cursorFirst": ops.moveCursor(-state.shownTotal); return
    case "cursorLast": ops.moveCursor(state.shownTotal); return
    case "mark": state.toggleMark(state.cursorIndex); return
    case "location": if (ops.entry) ops.entry.takeFocus(); return
    case "activate": state.activate(state.cursorIndex); return
    case "parent": state.goUp(); return
    case "back": state.goBack(); return
    case "toggleHidden": state.toggleHidden(); return
    // Ctrl+A and the menu's Select all row, ui/js/PickerOps.js selectAll through the state.
    case "selectAll": state.selectAll(); return
    case "copy": state.clip(false); return
    case "cut": state.clip(true); return
    case "paste": state.paste(); return
    // One source path owns the async reply. A second editor cannot replace it while that write runs.
    case "rename": if (state.renameFromPath.length === 0) state.startRename(); return
    case "newFolder": state.newFolder(); return
    case "trash": PickerOps.trash(state); return
    case "undo": Ops.undo(state); return
    }
    if (action in SHARED)
        state.message(SHARED[action] + " is not built in the chooser yet.", false)
}

// ui/js/Focus.js's LEAVES_LINE, resolved through the picker's own table: a cursor key commits the
// query line the way Enter does and then goes on to mean what it means everywhere else. A printable
// character is excluded before the lookup, or j and k would leave the line instead of being typed.
var LEAVES_LINE = ["cursorDown", "cursorUp", "cursorFirst", "cursorLast", "pageDown", "pageUp"]

function leavesLine(event, state) {
    if (event.text.length === 1 && event.text >= " ")
        return false
    return LEAVES_LINE.indexOf(ownVerb(event, state)) >= 0
}

// Filter.apply seats a cursor the query hid on the query's first match, which the chip can still
// hide: the chooser's shown is the two met, so the cursor lands on the first row both leave standing.
function settle(state) {
    var list = state.shown
    if (list !== null && list.length > 0 && Filter.viewOf(list, state.cursorIndex) < 0)
        state.setCursor(list[0])
}

// ui/PickerList.qml's Keys.onPressed: a key that resolves is accepted and acted on, and any other
// stays unaccepted so the window above still sees it. Answers the action for the caller's log.
// The filter's query line owns every key while it has the caret, ui/js/Focus.js's rule, so nothing
// below it, Escape included, sees a key until Enter or a cursor key hands the keyboard back.
function handle(event, state, ops) {
    // A live field owns every key. The index alone is not enough because a released delegate can
    // leave it set with no editor able to return the keyboard to the list.
    if (ops.renameEditor() !== null) {
        event.accepted = true
        return "rename"
    }
    if (state.filterTyping) {
        if (leavesLine(event, state)) {
            Filter.commit(state)
        } else {
            event.accepted = true
            Filter.typeKey(event, state)
            settle(state)
            return "filter"
        }
    }
    var action = lookup(event, state)
    if (action.length === 0)
        return ""
    event.accepted = true
    act(action, state, ops)
    return action
}
