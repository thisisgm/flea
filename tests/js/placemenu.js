.import "../../ui/js/Menu.js" as Menu
.import "../../ui/js/PlaceMenu.js" as PlaceMenu

// MenuAdditions rule 3: a Places or Favorites row opens the folder menu for its own path, with the
// clipboard, archive, send and destroy groups left out, and the last row adds or removes the
// favourite. Behind its own Extras switch, which ships off, so the rail keeps today's menu.

function paneStub(hidden) {
    return { uiState: { menu: { hidden: hidden } }, path: "/home/gm", opened: [], tabbed: "", terminal: "", copied: [],
             open: function (p) { this.opened.push(p) },
             openTerminal: function (p) { this.terminal = p },
             performMenu: function (action, id, paths) { this.copied.push(action + ":" + paths[0]) },
             home: "/home/gm", tabs: { items: [], index: 0 } }
}

function sidebarStub(hidden) {
    var pane = paneStub(hidden)
    return { navigationPane: pane, said: "", message: function (text) { this.said = text } }
}

function labels(rows) {
    return rows.filter(function (row) { return !row.separator }).map(function (row) { return row.label }).join("|")
}

function run(check) {
    var placeRow = { path: "/home/gm/Downloads", kind: "home", label: "Downloads" }
    var favouriteRow = { path: "/home/gm/Work", kind: "favourite", label: "Work", favouriteIndex: 2 }

    check("with the switch off the rail's own menu is the one it has today",
          PlaceMenu.entries(placeRow, -1, ["placeMenu"]).length, 0)
    // A fresh home has no state file yet, and the rail asked before ui/ViewState.qml had a list.
    check("and a list it could not read switches nothing on",
          PlaceMenu.entries(placeRow, -1, undefined).length, 0)

    check("a Places row offers the path rows and nothing that cuts, sends or destroys",
          labels(PlaceMenu.entries(placeRow, -1, [])),
          "Open|New tab|Open in terminal|Copy path|Add to Favorites")
    check("and a Favorites row ends on Remove rather than Add, so issue 138's duplicate is impossible",
          labels(PlaceMenu.entries(favouriteRow, 2, [])),
          "Open|New tab|Open in terminal|Copy path|Remove from Favorites")
    check("the key carries the path, because the rail rebuilds under an open menu",
          PlaceMenu.key(favouriteRow, 2), "place:2:/home/gm/Work")
    check("and a Places row carries no favourite index", PlaceMenu.key(placeRow, -1), "place:-1:/home/gm/Downloads")

    // Every row acts on the path the key carries and not on the listing's cursor.
    var acting = sidebarStub([])
    PlaceMenu.perform("open", "place:-1:/home/gm/Downloads", acting, null)
    check("Open opens the path the row named", acting.navigationPane.opened.join(","), "/home/gm/Downloads")
    PlaceMenu.perform("openTerminal", "place:-1:/home/gm/Downloads", acting, null)
    check("and so does Open in terminal", acting.navigationPane.terminal, "/home/gm/Downloads")
    PlaceMenu.perform("copypath", "place:-1:/home/gm/Downloads", acting, null)
    check("Copy path copies that path", acting.navigationPane.copied.join(","), "copypath:/home/gm/Downloads")

    var favourites = { records: [{ label: "Work", path: "/home/gm/Work" }], added: [], removed: [],
                       add: function (path, label) { this.added.push(path + " as " + label) },
                       remove: function (index) { this.removed.push(index) } }
    PlaceMenu.perform("addFavourite", "place:-1:/home/gm/Downloads", acting, favourites)
    check("Add to Favorites saves the path under its own leaf", favourites.added.join(","), "/home/gm/Downloads as Downloads")
    PlaceMenu.perform("removeFavourite", "place:0:/home/gm/Work", acting, favourites)
    check("Remove takes the row whose path the menu was opened on", favourites.removed.join(","), "0")
    PlaceMenu.perform("removeFavourite", "place:0:/home/gm/Elsewhere", acting, favourites)
    check("and a row that moved under the menu is said out loud rather than removed",
          favourites.removed.join(",") + "|" + acting.said,
          "0|Favorites changed; reopen the menu before removing this row.")

    // The older key is still the one a switched-off rail opens, identity check and all.
    var legacy = sidebarStub(["placeMenu"])
    PlaceMenu.perform("removeFavourite", 'favourite:0:{"label":"Work","path":"/home/gm/Work"}', legacy, favourites)
    check("the rail's own Remove still removes by index under its identity check",
          favourites.removed.join(","), "0,0")
    PlaceMenu.perform("removeFavourite", 'favourite:0:{"label":"Moved","path":"/home/gm/Work"}', legacy, favourites)
    check("and refuses when the record it was opened on is not there any more",
          favourites.removed.join(",") + "|" + legacy.said,
          "0,0|Favorites changed; reopen the menu before removing this row.")

    // A middle click opens a Places or Favorites row in a new tab; the rows with no folder of their
    // own until they are opened answer nothing, and a remote favourite says why.
    var middle = sidebarStub([])
    middle.entries = [{ kind: "trash", label: "Trash" }, { kind: "favourite", label: "NAS", path: "smb://nas/data",
                       original: { label: "NAS", path: "smb://nas/data" } }]
    PlaceMenu.openTabAt(middle, 0)
    check("a middle click on the Trash opens nothing", middle.said + middle.navigationPane.tabs.items.length, "0")
    PlaceMenu.openTabAt(middle, 1)
    check("and a remote favourite says it opens in this tab only", middle.said, "NAS opens in this tab only.")
}
