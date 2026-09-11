.pragma library
.import "Ops.js" as Ops

function request(pane) {
    if (pane.pathsPending || pane.clipPending != null) { pane.message("Still resolving the last selection; try again in a moment.", false); return }
    pane.pathsPending = { kind: "localsend" }
    pane.backend.askPaths(Ops.targetIndices(pane))
}

// Resolve through the backend: the selected rows can extend beyond the held window.
function resolved(pane, paths, send) {
    if (!pane.pathsPending || pane.pathsPending.kind !== "localsend")
        return false
    pane.pathsPending = null
    if (paths.length > 0)
        send(paths)
    return true
}
