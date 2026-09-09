.pragma library

.import "Keymap.js" as Keymap

// What the chooser does with a key. Two tables, in order: the picker's own verbs, which were an
// if-chain in ui/PickerList.qml, and then keys.toml through ui/js/Keymap.js for the operations the
// picker shares with the browser window, so a preset's rebinding (the windows preset's Ctrl+H)
// applies here too. Any other key answers "" and the event stays unaccepted. Pure, so
// tests/js/pickerkeys.js drives lookup, act and handle.

// The shared actions the picker takes from the key table, each with the name the footer prints
// while its verb is not built yet. An action outside this table never reaches act.
var SHARED = {
    copy: "Copy", cut: "Cut", paste: "Paste", selectAll: "Select all", trash: "Move to Trash",
    rename: "Rename", newFolder: "New folder", openTerminal: "Open in terminal",
    copydirpath: "Copy folder path", undo: "Undo", toggleHidden: "Hidden files",
    filter: "Filter", focusNext: "Focus"
}

function lookup(event, state) {
    var own = ownVerb(event, state)
    if (own.length > 0)
        return own
    var shared = Keymap.lookup(event.key, event.text, event.modifiers)
    return shared in SHARED ? shared : ""
}

// The picker's own keys. A Ctrl chord belongs to the key table except Ctrl+L, so the windows
// preset's Ctrl+H reaches toggleHidden instead of the bare h below. Escape while a fetch is in
// flight stops the download and not the dialog, ui/picker.qml's own rule for the same key.
function ownVerb(event, state) {
    var key = event.key
    var mods = event.modifiers
    if (mods & Qt.ControlModifier)
        return key === Qt.Key_L ? "location" : ""
    if (key === Qt.Key_Left)
        return (mods & Qt.AltModifier) ? "back" : ""
    switch (key) {
    case Qt.Key_Escape: return state.fetching ? "stopFetch" : "cancel"
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

// The dispatcher. state is ui/PickerState.qml; ops is ui/PickerList.qml, for the cursor moves
// that need the viewport and the field ":" hands the keyboard to. A shared action with no verb
// here yet says so in the footer, so a key never falls silent.
function act(action, state, ops) {
    switch (action) {
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
    }
    if (action in SHARED)
        state.message(SHARED[action] + " is not built in the chooser yet.", false)
}

// ui/PickerList.qml's Keys.onPressed: a key that resolves is accepted and acted on, and any other
// stays unaccepted so the window above still sees it. Answers the action for the caller's log.
function handle(event, state, ops) {
    var action = lookup(event, state)
    if (action.length === 0)
        return ""
    event.accepted = true
    act(action, state, ops)
    return action
}
