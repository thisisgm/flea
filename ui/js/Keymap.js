.pragma library

// Generated from keys.toml by tools/flea-keymap-gen. Do not edit.
function lookup(key, text, modifiers) {
    if (modifiers & Qt.ControlModifier) {
        if (modifiers & Qt.ShiftModifier) {
            if (key === Qt.Key_N) return "newFolder"
            if (key === Qt.Key_Plus) return "scaleUp"
            if (key === Qt.Key_Equal) return "scaleUp"
            if (key === Qt.Key_Minus) return "scaleDown"
            if (key === Qt.Key_Underscore) return "scaleDown"
            if (key === Qt.Key_0) return "scaleReset"
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
        if (key === Qt.Key_K) return "addNetwork"
        if (key === Qt.Key_L) return "pathBar"
        if (key === Qt.Key_Delete) return "trash"
        if (key === Qt.Key_Up) return "parent"
        if (key === Qt.Key_Down) return "open"
        if (key === Qt.Key_1) return "viewList"
        if (key === Qt.Key_2) return "viewColumns"
        if (key === Qt.Key_3) return "viewGrid"
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
    case "/": return "filter"
    case "f": return "search"
    case "o": return "reveal"
    case ":": return "pathBar"
    case "t": return "tabNew"
    case "w": return "tabClose"
    case "y": return "copy"
    case "x": return "cut"
    case "p": return "paste"
    case "d": return "trashArm"
    case "r": return "rename"
    case "z": return "undo"
    case "s": return "sortNext"
    case "S": return "sortReverse"
    case ".": return "toggleHidden"
    case "a": return "addNetwork"
    case "m": return "menu"
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
    { keys: "^+", action: "scaleUp", label: "scale up" },
    { keys: "^-", action: "scaleDown", label: "scale down" },
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
