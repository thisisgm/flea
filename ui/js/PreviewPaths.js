.pragma library

// Rows can arrive before pane.path is updated. Use the requested directory or home in that gap.
function rowPath(pane, row) {
    if (!pane || !row)
        return ""
    var base = pane.path || pane.listingPath || pane.home || "/"
    return pane.join(base, row.n)
}
