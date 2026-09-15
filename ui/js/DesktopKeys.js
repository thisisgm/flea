.pragma library
.import "Filter.js" as Filter
.import "Format.js" as Format
.import "Search.js" as Search
.import "Tabs.js" as Tabs

// Desktop aliases use the pane's existing operations; the QML host owns window and singleton access.
function act(action, pane, host) {
    switch (action) {
    case "refresh": pane.refresh(); return true
    case "home": pane.open(pane.home); return true
    case "bookmark": host.bookmark(); return true
    case "locationRoot": pane.pathBarRequested("/"); return true
    case "locationHome": pane.pathBarRequested(pane.home + "/"); return true
    case "closeWindow": host.closeWindow(); return true
    case "closeTabOrWindow":
        if (Tabs.count(pane) <= 1) host.closeWindow()
        else Tabs.act("tabClose", pane)
        return true
    case "searchCurrent":
    case "searchEverywhere":
        Filter.close(pane)
        pane.searchHere = action === "searchCurrent"
        Search.start(pane)
        return true
    case "invertSelection":
        for (var i = 0; i < pane.shownTotal; i++) pane.selection.toggle(Filter.at(pane.shown, i))
        pane.selectionVersion++
        return true
    case "openTab":
    case "openWindow":
        var row = pane.rowFor(pane.cursorIndex)
        if (!row) return true
        if (!row.d && !(Format.isSymlink(row.p) && row.i === "folder")) {
            pane.openCursor()
            return true
        }
        var path = pane.join(pane.path, row.n)
        if (action === "openTab") Tabs.openNew(pane, path)
        else pane.newWindow(path)
        return true
    }
    return false
}

function typeAhead(event, pane) {
    if (pane.focusView !== "list" || pane.searchMode.length > 0 || !event.text || event.text.length !== 1
            || event.text < " " || (event.modifiers & ~Qt.ShiftModifier) !== 0) return false
    Filter.close(pane)
    pane.searchHere = true
    pane.searchQuery = event.text
    Search.start(pane)
    return true
}
