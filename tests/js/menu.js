.import "../../ui/js/Menu.js" as Menu
.import "../../ui/js/Icons.js" as Icons

function state(changes) {
    var value = { hasRow: true, selectionCount: 1, rowMode: 0o100644, clipboardAvailable: false,
        hiddenActions: ["delete", "openTerminal", "moveto", "copyto", "properties", "permissions", "copypath"],
        archiveFormats: ["zip"], canExtract: true, rowIsArchive: false, rowIsImage: true, canConvert: true,
        taildropInstalled: true, taildropPeers: [{ id: "box", label: "Box" }],
        dropboxInstalled: true, dropboxPath: "/tmp/Dropbox", rowInDropbox: false }
    for (var key in changes) value[key] = changes[key]
    return value
}
function actions(rows) { return rows.filter(function (r) { return !r.separator }).map(function (r) { return r.action }).join(",") }
function entry(rows, action) { return rows.filter(function (r) { return r.action === action })[0] || {} }
function separated(rows) {
    return !rows.length || !rows[0].separator && !rows[rows.length - 1].separator
        && !rows.some(function (r, i) { return r.separator && i > 0 && rows[i - 1].separator })
}
function run(check) {
    var file = Menu.listingEntries(state({}))
    check("Menus and Places inventory has 30 actions", Menu.INVENTORY.length, 30)
    check("Open with uses the authoritative cut geometry", Icons.pathFor("app-window"), "M3 4h18v16H3z M3 9h18 M6 6.5h.01 M9 6.5h.01")
    check("Restore all uses the authoritative undo geometry", Icons.pathFor("undo"), "M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8 M3 3v5h5")
    check("inventory storage ids are unique", Object.keys(Menu.INVENTORY.reduce(function (out, row) { out[row[0]] = true; return out }, {})).length, 30)
    check("default image menu matches Menus specimen", actions(file),
          "open,openWith,cut,copy,paste,duplicate,rename,compress,convert,taildrop,dropbox,trash,addFavourite,toggleHidden")
    check("empty clipboard leaves Paste visible and disabled", entry(file, "paste").disabled, true)
    check("populated clipboard enables Paste", entry(Menu.listingEntries(state({ clipboardAvailable: true })), "paste").disabled, false)
    check("folder omits conversion and extraction", actions(Menu.listingEntries(state({ rowMode: 0o040755, rowIsImage: false }))),
          "open,openWith,cut,copy,paste,duplicate,rename,compress,taildrop,dropbox,trash,addFavourite,toggleHidden")
    check("background menu includes real creation actions in order", actions(Menu.listingEntries(state({ hasRow: false }))),
          "newFolder,newFile,paste,selectAll,addFavourite,sort,toggleHidden,settings")
    check("selected file cannot be pinned as a folder", entry(file, "addFavourite").disabled, true)
    check("Favorites menu uses GM's displayed spelling", entry(file, "addFavourite").label, "Add to Favorites")
    check("selected directory can be pinned", entry(Menu.listingEntries(state({ rowMode: 0o040755 })), "addFavourite").disabled, false)
    check("multi-selection cannot pin a cursor sibling", entry(Menu.listingEntries(state({ rowMode: 0o040755, selectionCount: 2 })), "addFavourite").disabled, true)
    check("symlink metadata cannot pretend to be a directory", entry(Menu.listingEntries(state({ rowMode: 0o120777 })), "addFavourite").disabled, true)
    check("missing metadata cannot be pinned", entry(Menu.listingEntries(state({ rowMode: undefined })), "addFavourite").disabled, true)
    check("background pins the current folder without selected rows", entry(Menu.listingEntries(state({ hasRow: false, selectionCount: 0 })), "addFavourite").disabled, undefined)
    check("direct background builder ignores row eligibility", entry(Menu.backgroundEntries(state({})), "addFavourite").disabled, undefined)
    check("empty Trash retains both disabled actions", actions(Menu.trashEntries(0, false)), "open,restoreAll,emptyTrash")
    check("empty Trash disables restore", entry(Menu.trashEntries(0, false), "restoreAll").disabled, true)
    check("empty Trash disables empty", entry(Menu.trashEntries(0, false), "emptyTrash").disabled, true)
    check("busy Trash disables destructive reactivation", entry(Menu.trashEntries(4, true), "emptyTrash").disabled, true)
    check("full idle Trash enables restore", entry(Menu.trashEntries(4, false), "restoreAll").disabled, false)
    var all = Menu.listingEntries(state({ hiddenActions: [] }))
    check("stored delete id reaches permanent deletion action", entry(all, "deletePermanently").id, "delete")
    check("all optional file controls exist", ["openWith", "moveTo", "copyTo", "properties", "permissions", "copypath", "openTerminal"].every(function (a) { return !!entry(all, a).action }), true)
    check("permanent deletion carries danger role", entry(all, "deletePermanently").danger, true)
    check("single-item actions stay present but disabled on multi-selection", ["openWith", "properties", "rename", "duplicate", "permissions"].every(function (a) {
        return entry(Menu.listingEntries(state({ hiddenActions: [], selectionCount: 2 })), a).disabled === true
    }), true)
    check("missing converter removes Convert", entry(Menu.listingEntries(state({ canConvert: false })), "convert").action, undefined)
    check("missing archiver removes Compress", entry(Menu.listingEntries(state({ archiveFormats: [] })), "compress").action, undefined)
    check("archive needs actual extraction capability", entry(Menu.listingEntries(state({ rowIsArchive: true, canExtract: false })), "extract").action, undefined)
    check("supported archive offers Extract", entry(Menu.listingEntries(state({ rowIsArchive: true })), "extract").action, "extract")
    check("absent Taildrop provider removes its row", entry(Menu.listingEntries(state({ taildropInstalled: false })), "taildrop").action, undefined)
    var offline = entry(Menu.listingEntries(state({ taildropPeers: [] })), "taildrop")
    check("offline installed Taildrop remains disabled", offline.disabled, true)
    check("offline Taildrop reads as an error, with no sentence beside it", offline.errored + "|" + offline.hint, "true|undefined")
    check("offline Taildrop offers no stale peers", offline.submenu.length, 0)
    check("fresh status disables a cached Taildrop target", entry(Menu.listingEntries(state({providersRefreshing: true})), "taildrop").disabled, true)
    check("a provider still being read is dimmed, not errored", String(entry(Menu.listingEntries(state({providersRefreshing: true})), "taildrop").errored), "undefined")
    check("fresh status disables a cached Dropbox directory", entry(Menu.listingEntries(state({providersRefreshing: true})), "dropbox").disabled, true)
    check("installed signed-out provider reads as an error", entry(Menu.listingEntries(state({taildropPeers: [], taildropReason: "signed out"})), "taildrop").errored, true)
    var absent = Menu.listingEntries(state({taildropInstalled: false, dropboxInstalled: false}))
    var available = Menu.listingEntries(state({}))
    var trashCursor = absent.findIndex(function(row) { return row.action === "trash" })
    var refreshed = Menu.refreshedCursor(absent, available, trashCursor, -1, 0)
    check("new provider inventory inserts both actual providers", available.filter(function(row) { return row.action === "taildrop" || row.action === "dropbox" }).length, 2)
    check("new providers do not steal the current action", available[refreshed.cursor].action, "trash")
    refreshed = Menu.refreshedCursor(available, absent, refreshed.cursor, -1, 0)
    check("removed providers leave the existing action selected", absent[refreshed.cursor].action, "trash")
    check("removed providers are absent from fresh inventory", absent.filter(function(row) { return row.action === "taildrop" || row.action === "dropbox" }).length, 0)
    var oldPeers = [{action: "taildrop", submenu: [{id: "b"}]}]
    var newPeers = [{action: "taildrop", submenu: [{id: "a"}, {id: "b"}]}]
    refreshed = Menu.refreshedCursor(oldPeers, newPeers, 0, 0, 0)
    check("a submenu keeps its target id when peers reorder", refreshed.submenuCursor, 1)
    check("a retained submenu keeps its parent action", refreshed.submenuRow, 0)
    refreshed = Menu.refreshedCursor(oldPeers, [{action: "taildrop", submenu: [{id: "a"}]}], 0, 0, 0)
    check("a vanished peer closes rather than retargeting the submenu", refreshed.submenuRow, -1)
    refreshed = Menu.refreshedCursor([{action: "taildrop"}], [{separator: true}, {action: "copy", disabled: true}, {action: "open"}], 0, -1, 0)
    check("a removed action selects an eligible row rather than a separator", refreshed.cursor, 2)
    check("missing Dropbox removes its row", entry(Menu.listingEntries(state({ dropboxInstalled: false })), "dropbox").action, undefined)
    check("offline Dropbox remains disabled", entry(Menu.listingEntries(state({ dropboxPath: "" })), "dropbox").disabled, true)
    var inside = Menu.listingEntries(state({ rowInDropbox: true }))
    check("inside Dropbox replaces move with share link", entry(inside, "dropbox").action + "|" + entry(inside, "sharelink").action, "undefined|sharelink")
    check("hidden capability rows leave no doubled separators", separated(Menu.listingEntries(state({ hiddenActions: ["compress", "convert", "taildrop", "dropbox"] }))), true)
    check("Open cannot be hidden", Menu.isHidden(["open"], "open"), false)
    check("hidden toggle cannot be hidden", Menu.isHidden(["toggleHidden"], "toggleHidden"), false)
    check("stored lower-case id hides corresponding action", entry(Menu.listingEntries(state({ hiddenActions: ["openwith"] })), "openWith").action, undefined)
    check("hidden files toggle uses inverse wording", Menu.hiddenRow(true).label, "Hide hidden files")
    check("permissions accepts ordinary file", Menu.permissionsEntry(0o100644, 1).disabled, false)
    check("permissions accepts directory", Menu.permissionsEntry(0o040755, 1).disabled, false)
    check("permissions rejects symlink", Menu.permissionsEntry(0o120777, 1).hint, "Symlink target not changed")
    check("permissions rejects missing metadata", Menu.permissionsEntry(undefined, 1).disabled, true)
    check("permissions rejects fifo", Menu.permissionsEntry(0o010644, 1).disabled, true)
    check("permissions allows read-only inspection of special bits", Menu.permissionsEntry(0o104755, 1).disabled, false)
    check("menu fits at pointer", Menu.clamp(20, 80, 300), 20)
    check("menu flips before shifting", Menu.clamp(270, 80, 300), 190)
    check("oversized menu pins to near edge", Menu.clamp(30, 500, 300), 0)
    check("negative point clamps", Menu.clamp(-20, 80, 300), 0)
    check("submenu array is recognized even when empty", Menu.hasSubmenu({ submenu: [] }), true)
    check("ordinary entry has no submenu", Menu.hasSubmenu({ action: "open" }), false)
    check("missing entry has no submenu", Menu.hasSubmenu(undefined), false)
    check("header keeps required Name column outside toggles", actions(Menu.headerEntries([], false)), "col:mode,col:size,col:date,col:kind,toggleHidden")
    check("sort submenu uses real backend order ids", Menu.sortEntries().map(function (r) { return r.id }).join(","), "name,size,mtime,kind")
}
