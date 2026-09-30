.pragma library

.import "Menu.js" as Menu
.import "Mounts.js" as Mounts
.import "Places.js" as Places
.import "Tabs.js" as Tabs

// MenuAdditions rule 3: the folder menu, opened from a Places or Favorites row and acting on that
// row's own path. The key carries the path because the rail rebuilds on a five second poll and an
// index taken when the menu opened can name another row by the time a row inside it is chosen, the
// same reason ui/js/Mounts.js keys a share's menu by its uri.
function key(entry, favouriteIndex) {
    return "place:" + favouriteIndex + ":" + entry.path
}

// The hidden list is ui/ViewState.qml "menuHidden", handed in by the caller: it is the one reader
// that falls back to the shipped defaults, and a list derived any other way is empty before the
// state file lands, which switched this menu on in a fresh home. Anything else fails closed.
function entries(entry, favouriteIndex, hidden) {
    if (!Array.isArray(hidden))
        return []
    return Menu.placeEntries({ hiddenActions: hidden, hasRow: true, placeFavourite: favouriteIndex >= 0 })
}

// Every row here takes the path the key carries. Properties is not among them: the dialog snapshots
// row indices through the backend's own menuaction op, which a place has none of, so the board's
// sixth row waits for that protocol rather than being faked from the listing's cursor.
function perform(action, key, sidebar, favourites) {
    // The rail's older key, which only ever carried Remove: kept exactly, identity check included,
    // because it is what a favourite's menu still is while the Places row menu is switched off.
    if (key.indexOf("favourite:") === 0) {
        var end = key.indexOf(":", 10)
        var at = Number(key.substring(10, end))
        if (action !== "removeFavourite") return
        if (JSON.stringify(favourites.records[at]) === key.substring(end + 1)) favourites.remove(at)
        else sidebar.message("Favorites changed; reopen the menu before removing this row.", true)
        return
    }
    var rest = key.substring("place:".length)
    var cut = rest.indexOf(":")
    var index = Number(rest.substring(0, cut))
    var path = rest.substring(cut + 1)
    var pane = sidebar.navigationPane
    if (!pane || path.length === 0)
        return
    if (action === "open") pane.open(path)
    else if (action === "openTab") Tabs.openNew(pane, path)
    else if (action === "openTerminal") pane.openTerminal(path)
    else if (action === "copypath") pane.performMenu("copypath", 0, [path])
    else if (action === "addFavourite") favourites.add(path, leaf(path))
    else if (action === "removeFavourite") removeAt(sidebar, favourites, index, path)
}

// A favourite is removed by index, so the index is checked against the path the menu was opened on:
// the rail rebuilds on a five second poll and Favourites can be rewritten by the other front end.
function removeAt(sidebar, favourites, index, path) {
    var record = favourites.records[index]
    if (record && String(record.path) === path) favourites.remove(index)
    else sidebar.message("Favorites changed; reopen the menu before removing this row.", true)
}

function leaf(path) {
    var cut = String(path).replace(/\/+$/, "").lastIndexOf("/")
    return cut < 0 ? path : path.substring(cut + 1)
}

// A middle click on a Places or Favorites row: its folder in a new tab, the listing's own middle click
// (ui/js/Tap.js tappedTab). A favourite resolves as ui/Sidebar.qml openFavourite does, and a remote
// one, which has to mount before it has a path, says so rather than opening somewhere else.
function openTabAt(sidebar, index) {
    var entry = sidebar.entries[index]
    var pane = sidebar.navigationPane
    if (!entry || !pane || (entry.kind !== "home" && entry.kind !== "favourite"))
        return
    var error = entry.kind === "favourite" ? Places.recordError(entry.original) : ""
    if (error) { sidebar.message("Could not open " + entry.label + " · " + error, true); return }
    var path = String(entry.path)
    if (path.indexOf("file://") === 0) path = Mounts.decodePath(path.substring(7))
    else if (path.indexOf("://") >= 0) { sidebar.message(entry.label + " opens in this tab only.", false); return }
    Tabs.openNew(pane, path)
}
