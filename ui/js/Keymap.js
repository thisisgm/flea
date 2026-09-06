.pragma library

// Generated from keys.toml by tools/flea-keymap-gen. Do not edit.
// The selected Mac/Windows preset. A .pragma library holds one copy per QML engine, so
// ui/ViewState.qml sets it once and every caller of lookup() below follows without a
// second wire; an unknown name falls back to mac rather than leaving the map empty.
var preset = "mac"
function setPreset(name) { preset = name === "windows" ? "windows" : "mac" }

// The [[preset]] rows of keys.toml, for ui/SettingsPanel.qml's Keys section. code is the Qt
// name the overlay below matches on, so a row here and the binding are the same keys.toml row.
var PRESET_KEYS = [
    { preset: "mac", ctrl: true, shift: false, code: "Key_1", keys: "ctrl-1", action: "viewList", label: "list view" },
    { preset: "mac", ctrl: true, shift: false, code: "Key_2", keys: "ctrl-2", action: "viewColumns", label: "columns view" },
    { preset: "mac", ctrl: true, shift: false, code: "Key_3", keys: "ctrl-3", action: "viewGrid", label: "grid view" },
    { preset: "mac", ctrl: true, shift: false, code: "Key_Up", keys: "ctrl-up", action: "parent", label: "up one level" },
    { preset: "mac", ctrl: true, shift: false, code: "Key_Down", keys: "ctrl-down", action: "open", label: "open" },
    { preset: "mac", ctrl: true, shift: false, code: "Key_Delete", keys: "ctrl-delete", action: "trash", label: "move to trash" },
    { preset: "mac", ctrl: true, shift: false, code: "Key_K", keys: "ctrl-k", action: "addNetwork", label: "connect to server" },
    { preset: "windows", ctrl: true, shift: false, code: "Key_H", keys: "ctrl-h", action: "toggleHidden", label: "hidden files" },
    { preset: "windows", ctrl: true, shift: true, code: "Key_1", keys: "ctrl-shift-1", action: "viewList", label: "list view" },
    { preset: "windows", ctrl: true, shift: true, code: "Key_2", keys: "ctrl-shift-2", action: "viewColumns", label: "columns view" },
    { preset: "windows", ctrl: true, shift: true, code: "Key_3", keys: "ctrl-shift-3", action: "viewGrid", label: "grid view" },
]

// Checked before every shared table, so a preset can claim a chord the shared tables bind.
function lookupPreset(name, key, text, modifiers) {
    var ctrl = (modifiers & Qt.ControlModifier) !== 0
    var shift = (modifiers & Qt.ShiftModifier) !== 0
    if (name === "mac" && ctrl && !shift && key === Qt.Key_1) return "viewList"
    if (name === "mac" && ctrl && !shift && key === Qt.Key_2) return "viewColumns"
    if (name === "mac" && ctrl && !shift && key === Qt.Key_3) return "viewGrid"
    if (name === "mac" && ctrl && !shift && key === Qt.Key_Up) return "parent"
    if (name === "mac" && ctrl && !shift && key === Qt.Key_Down) return "open"
    if (name === "mac" && ctrl && !shift && key === Qt.Key_Delete) return "trash"
    if (name === "mac" && ctrl && !shift && key === Qt.Key_K) return "addNetwork"
    if (name === "windows" && ctrl && !shift && key === Qt.Key_H) return "toggleHidden"
    if (name === "windows" && ctrl && shift && key === Qt.Key_1) return "viewList"
    if (name === "windows" && ctrl && shift && key === Qt.Key_2) return "viewColumns"
    if (name === "windows" && ctrl && shift && key === Qt.Key_3) return "viewGrid"
    return ""
}

function lookup(key, text, modifiers) {
    var chosen = lookupPreset(preset, key, text, modifiers)
    if (chosen.length > 0)
        return chosen
    if (modifiers & Qt.ControlModifier) {
        if (modifiers & Qt.ShiftModifier) {
            if (key === Qt.Key_N) return "newFolder"
            if (key === Qt.Key_Plus) return "textSizeUp"
            if (key === Qt.Key_Equal) return "textSizeUp"
            if (key === Qt.Key_Minus) return "textSizeDown"
            if (key === Qt.Key_Underscore) return "textSizeDown"
            if (key === Qt.Key_0) return "textSizeReset"
            if (key === Qt.Key_Greater) return "toggleHidden"
            if (key === Qt.Key_Period) return "toggleHidden"
        }
        if (key === Qt.Key_D) return "pageDown"
        if (key === Qt.Key_U) return "pageUp"
        if (key === Qt.Key_A) return "selectAll"
        if (key === Qt.Key_C) return "copy"
        if (key === Qt.Key_V) return "paste"
        if (key === Qt.Key_X) return "cut"
        if (key === Qt.Key_Z) return "undo"
        if (key === Qt.Key_F) return "search"
        if (key === Qt.Key_E) return "eject"
        if (key === Qt.Key_L) return "pathBar"
        if (key === Qt.Key_T) return "openTerminal"
        if (key === Qt.Key_Comma) return "settings"
        return ""
    }

    if (modifiers & Qt.ShiftModifier) {
        if (key === Qt.Key_Down) return "extendDown"
        if (key === Qt.Key_Up) return "extendUp"
    }

    switch (key) {
    case Qt.Key_Down: return "cursorDown"
    case Qt.Key_Up: return "cursorUp"
    case Qt.Key_Home: return "cursorFirst"
    case Qt.Key_End: return "cursorLast"
    case Qt.Key_PageUp: return "pageUp"
    case Qt.Key_PageDown: return "pageDown"
    case Qt.Key_Return: return "open"
    case Qt.Key_Enter: return "open"
    case Qt.Key_Backspace: return "parent"
    case Qt.Key_Delete: return "trash"
    case Qt.Key_Escape: return "escape"
    case Qt.Key_Tab: return "focusNext"
    case Qt.Key_Space: return "preview"
    case Qt.Key_F2: return "rename"
    case Qt.Key_Left: return "seekBack"
    case Qt.Key_Right: return "seekForward"
    }

    switch (text) {
    case "j": return "cursorDown"
    case "k": return "cursorUp"
    case "g": return "cursorFirst"
    case "G": return "cursorLast"
    case "v": return "toggleSelect"
    case "J": return "extendDown"
    case "K": return "extendUp"
    case "h": return "parent"
    case "A": return "openAgent"
    case "O": return "openVideoEditor"
    case "/": return "filter"
    case "f": return "search"
    case "o": return "reveal"
    case ":": return "pathBar"
    case "t": return "tabNew"
    case "w": return "tabClose"
    case "y": return "copy"
    case "Y": return "copydirpath"
    case "x": return "cut"
    case "p": return "paste"
    case "P": return "openAgentPicker"
    case "d": return "trashArm"
    case "r": return "rename"
    case "z": return "undo"
    case "s": return "sortNext"
    case "S": return "sortReverse"
    case ".": return "toggleHidden"
    case "a": return "addNetwork"
    case "m": return "menu"
    case ",": return "settings"
    case "?": return "keymapSheet"
    case "-": return "zoomOut"
    case "+": return "zoomIn"
    case "e": return "expand"
    case "l": return "pageForward"
    }

    if (text >= "1" && text <= "9") {
        return "tab" + text
    }
    return ""
}

// The key ui/MenuRow.qml prints beside a menu row, keyed on the row's own action. Only bare
// keys are here: an action reachable by a chord alone leaves its row's hint slot empty, which
// is how Menus.html draws New Folder. Derived from keys.toml, so a hint cannot advertise a key
// nothing is bound to.
var HINTS = {
    "addNetwork": "a",
    "copy": "y",
    "copydirpath": "Y",
    "cursorDown": "j",
    "cursorFirst": "g",
    "cursorLast": "G",
    "cursorUp": "k",
    "cut": "x",
    "escape": "escape",
    "expand": "e",
    "extendDown": "J",
    "extendUp": "K",
    "filter": "/",
    "focusNext": "tab",
    "keymapSheet": "?",
    "menu": "m",
    "open": "enter",
    "openAgent": "A",
    "openAgentPicker": "P",
    "openVideoEditor": "O",
    "pageDown": "pagedown",
    "pageForward": "l",
    "pageUp": "pageup",
    "parent": "h",
    "paste": "p",
    "pathBar": ":",
    "preview": "space",
    "rename": "r",
    "reveal": "o",
    "search": "f",
    "seekBack": "left",
    "seekForward": "right",
    "settings": ",",
    "sortNext": "s",
    "sortReverse": "S",
    "tabClose": "w",
    "tabNew": "t",
    "toggleHidden": ".",
    "toggleSelect": "v",
    "trash": "d",
    "trashArm": "d",
    "undo": "z",
    "zoomIn": "+",
    "zoomOut": "-",
}

function hintFor(action) {
    var k = HINTS[String(action)]
    return k ? k : ""
}

// The keymap sheet ui/KeymapSheet.qml draws, from the [[sheet]] table in keys.toml.
var SHEET = [
    { keys: "j k", action: "cursorDown", label: "move" },
    { keys: "enter", action: "open", label: "open" },
    { keys: "l", action: "pageForward", label: "browse forward" },
    { keys: "space", action: "preview", label: "preview" },
    { keys: "/", action: "filter", label: "filter" },
    { keys: "f", action: "search", label: "find in subtree" },
    { keys: "o", action: "reveal", label: "reveal result" },
    { keys: "tab", action: "focusNext", label: "search scope, or focus" },
    { keys: ": ^l", action: "pathBar", label: "go to path" },
    { keys: "y ^c", action: "copy", label: "copy" },
    { keys: "Y", action: "copydirpath", label: "copy folder path" },
    { keys: "x ^x", action: "cut", label: "cut" },
    { keys: "p ^v", action: "paste", label: "paste" },
    { keys: "r", action: "rename", label: "rename" },
    { keys: "dd", action: "trashArm", label: "trash" },
    { keys: "z ^z", action: "undo", label: "undo" },
    { keys: "^N", action: "newFolder", label: "new folder" },
    { keys: "v", action: "toggleSelect", label: "select" },
    { keys: "s", action: "sortNext", label: "sort column" },
    { keys: "S", action: "sortReverse", label: "reverse sort" },
    { keys: ". ^>", action: "toggleHidden", label: "hidden files" },
    { keys: "a", action: "addNetwork", label: "add network place" },
    { keys: "m", action: "menu", label: "context menu" },
    { keys: "^e", action: "eject", label: "eject" },
    { keys: "^t", action: "openTerminal", label: "open terminal" },
    { keys: "O", action: "openVideoEditor", label: "open in omacut" },
    { keys: "A", action: "openAgent", label: "open agent" },
    { keys: "P", action: "openAgentPicker", label: "agent picker" },
    { keys: "^+", action: "textSizeUp", label: "text size up" },
    { keys: "^-", action: "textSizeDown", label: "text size down" },
    { keys: ",", action: "settings", label: "settings" },
    { keys: "?", action: "keymapSheet", label: "this sheet" },
]

// The pointer contract, from the [[pointer]] table in keys.toml. ui/js/Tap.js is what makes
// it true and tests/js/tap.js is what holds the two together.
var POINTER = [
    { where: "listing", press: "left", row: "any", does: "selectOnly", label: "put the cursor on the row and drop any other selection" },
    { where: "listing", press: "left x2", row: "any", does: "open", label: "open the row" },
    { where: "listing", press: "left x2", row: "result", does: "reveal", label: "go to the file in its own directory, selected" },
    { where: "listing", press: "ctrl left", row: "any", does: "toggleSelect", label: "add the row to the selection" },
    { where: "listing", press: "shift left", row: "any", does: "extendSelect", label: "extend the selection to the row" },
    { where: "listing", press: "ctrl left x2", row: "any", does: "toggleSelect", label: "still only selects" },
    { where: "listing", press: "shift left x2", row: "any", does: "extendSelect", label: "still only selects" },
    { where: "listing", press: "left", row: "renaming", does: "commitRename", label: "commit the open rename, then select the row" },
    { where: "listing", press: "right", row: "any", does: "menu", label: "open the context menu at the pointer, on the selection the row is in" },
    { where: "neighbour", press: "left", row: "dir", does: "reveal", label: "show that directory in the middle column" },
    { where: "neighbour", press: "left", row: "file", does: "nothing", label: "a file has no contents to reveal" },
    { where: "neighbour", press: "left x2", row: "file", does: "open", label: "open the file" },
    { where: "neighbour", press: "right", row: "any", does: "nothing", label: "a peeked row has no menu" },
    { where: "chrome", press: "left", row: "parent", does: "goToCrumb", label: "open the directory that segment of the path names" },
    { where: "chrome", press: "left x2", row: "any", does: "pathBar", label: "type the path instead of clicking it" },
    { where: "window", press: "back", row: "any", does: "backOrParent", label: "go back through the history, or up a directory when there is none" },
    { where: "rail", press: "left", row: "any", does: "open", label: "open the place" },
    { where: "rail", press: "right", row: "any", does: "menu", label: "eject and unmount" },
]
