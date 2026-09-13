.pragma library

.import "Ops.js" as Ops

function parent(path) {
    return path.substring(0, path.lastIndexOf("/")) || "/"
}

// These actions otherwise infer their target from the listing. The menu snapshot owns this path.
function perform(pane, action, id, path) {
    if (action === "open") pane.open(path)
    else if (action === "openTerminal") pane.openTerminal(path)
    else if (action === "permissions") pane.permissionsRequested(path)
    else if (action === "duplicate") pane.backend.duplicate(path, id)
    else if (action === "trash") pane.backend.send({c: "trash", paths: [path], menuId: id})
    else if (action === "paste") Ops.paste(pane, path)
    else if (action.indexOf("compress:") === 0) Ops.compressResolved(pane, [path], action.substring(9), id, parent(path))
    else return false
    return true
}
