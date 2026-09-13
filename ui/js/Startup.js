.pragma library

// Where a window opens, and where a new tab opens. Both were fixed before 0.2.1: a window always
// opened on $HOME unless the command line named a path, and a new tab always cloned the folder the
// pane was resting on. Settings > View > Opening now decides, and this is the only place that reads
// those keys, so the panel, ui/shell.qml and ui/js/Tabs.js can never disagree about the answer.

// Sample state, as ui/ViewState.qml holds it:
// {startIn: "folder", startFolder: "/home/gm/Work", lastPath: "/home/gm/Pictures", newTab: "home"}
// argvPath is FLEA_PATH, which ui/shell.qml reads off the command line.
function startPath(state, home, argvPath) {
    // A path the caller named outranks every setting: a file manager asked to open somewhere opens
    // there, and this is the same precedence the --select flag already has over the remembered pair.
    if (argvPath && String(argvPath).length > 0)
        return String(argvPath)
    var data = state || {}
    var mode = data.startIn || "home"
    if (mode === "last")
        return orHome(data.lastPath, home)
    if (mode === "folder")
        return orHome(data.startFolder, home)
    return home
}

// here is the folder the pane is resting on, which is what a new tab used to clone unconditionally.
function newTabPath(state, here, home) {
    var mode = (state || {}).newTab || "current"
    if (mode === "home")
        return home
    if (mode === "start")
        return startPath(state, home, "")
    return here
}

// A chosen folder can be deleted or renamed between two runs, and a remembered one can be a stick
// that is no longer plugged in. Nothing here can stat a path, so an empty setting falls back to home
// and a path that no longer exists is left to the listing's own error, which names what is wrong.
function orHome(path, home) {
    return path && String(path).length > 0 ? String(path) : home
}
