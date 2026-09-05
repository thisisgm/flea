.pragma library

.import "Eject.js" as Eject
.import "Mounts.js" as Mounts

// What the rail does with a key, split out of Focus.js at its 300-line hard cap the same way
// ui/js/PreviewKeys.js was: Focus.js decides which surface owns a key, and this is the surface.

// The rail answers seven of the key table's action names and ignores the rest while it has focus.
function act(action, root, sidebar) {
    switch (action) {
    case "cursorDown": sidebar.cursorIndex = Math.min(sidebar.entries.length - 1, sidebar.cursorIndex + 1); return
    case "cursorUp": sidebar.cursorIndex = Math.max(0, sidebar.cursorIndex - 1); return
    // The sheet advertises g and G as first and last row, and the rail is a cursored list too, so
    // they answered nothing here while every other cursor key worked.
    case "cursorFirst": sidebar.cursorIndex = 0; return
    case "cursorLast": sidebar.cursorIndex = Math.max(0, sidebar.entries.length - 1); return
    // activate(), not a direct opened(path): a Network entry may need mounting first.
    case "open": if (sidebar.entries.length > 0) sidebar.activate(sidebar.cursorIndex); return
    // Focus.LIST's own value, written out because importing Focus.js back would be a cycle.
    case "escape": root.focusView = "list"; return
    case "addNetwork": sidebar.addRequested(); return
    // Favorites are not offered: Sidebar.startRename ignores an index outside the Network group.
    case "rename": sidebar.startRename(sidebar.cursorIndex); return
    // Eject and Unmount are menu rows, so this opens the menu rather than inventing a second route.
    case "menu": Mounts.raiseMenu(root, sidebar); return
    case "eject": Eject.release(root, sidebar, true); return
    }
}
