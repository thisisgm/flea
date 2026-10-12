.import "menuhunt.js" as MenuHunt
.import "../../ui/js/Menu.js" as Menu
.import "../../ui/js/MenuRefresh.js" as MenuRefresh
.import "../../ui/js/LockedMenu.js" as LockedMenu
.import "../../ui/js/Nav.js" as Nav
.import "../../ui/js/Icons.js" as Icons
.import "../../ui/js/Mounts.js" as Mounts
.import "sourcefixture.js" as Source

function state(changes) {
    var value = { hasRow: true, selectionCount: 1, rowMode: 0o100644, clipboardAvailable: false,
        rowIsSymlink: false,
        hiddenActions: ["delete", "openTerminal", "moveto", "copyto", "properties", "permissions",
            "copyAs", "pasteAs", "invertSelection"],
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
    check("Menus and Places inventory has 49 actions", Menu.INVENTORY.length, 49)
    check("Open with uses the authoritative cut geometry", Icons.pathFor("app-window"), "M3 4h18v16H3z M3 9h18 M6 6.5h.01 M9 6.5h.01")
    check("Restore all uses the authoritative undo geometry", Icons.pathFor("undo"), "M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8 M3 3v5h5")
    check("inventory storage ids are unique", Object.keys(Menu.INVENTORY.reduce(function (out, row) { out[row[0]] = true; return out }, {})).length, 49)
    check("default image menu matches Menus specimen", actions(file),
          "open,openWith,cut,copy,paste,duplicate,rename,compress,convert,addToShelf,taildrop,dropbox,trash,addFavourite,toggleHidden")
    check("empty clipboard keeps Paste disabled", entry(file, "paste").disabled, true)
    check("populated clipboard enables Paste", entry(Menu.listingEntries(state({ clipboardAvailable: true })), "paste").disabled, false)
    check("folder omits conversion and extraction", actions(Menu.listingEntries(state({ rowMode: 0o040755, rowIsImage: false }))),
          "open,openWith,cut,copy,paste,duplicate,rename,compress,addToShelf,taildrop,dropbox,trash,addFavourite,toggleHidden")
    check("background menu includes real creation actions in order", actions(Menu.listingEntries(state({ hasRow: false }))),
          "newFolder,newFile,paste,selectAll,openTerminal,addFavourite,sort,toggleHidden,settings")
    var background = Menu.listingEntries(state({ hasRow: false, updateVersion: "0.3.4" }))
    check("a known newer build adds Update Flea under Settings, in the same group, with the download mark",
          background.slice(-2).map(function (r) { return (r.separator ? "|" : r.action) + ":" + r.glyph }).join(","),
          "settings:sliders,updateFlea:download")
    check("and its hint slot carries that version beside the status square",
          entry(background, "updateFlea").hint + "|" + entry(background, "updateFlea").hintSquare, "0.3.4|true")
    check("the Menus switch takes it away like any other row",
          entry(Menu.listingEntries(state({ hasRow: false, updateVersion: "0.3.4", hiddenActions: ["updateFlea"] })), "updateFlea").action, undefined)
    check("a file row's menu never offers it", entry(Menu.listingEntries(state({ updateVersion: "0.3.4" })), "updateFlea").action, undefined)
    check("and neither does an empty version, which is what every state but available gives",
          entry(Menu.listingEntries(state({ hasRow: false, updateVersion: "" })), "updateFlea").action, undefined)
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
    // Railmenus2: rail rows live in INVENTORY with kinds R and draw the listing menu's own separator on a group change.
    function railLabels(rows) { return rows.filter(function (r) { return !r.separator }).map(function (r) { return r.label }).join("|") }
    function railGlyphs(rows) { return rows.filter(function (r) { return !r.separator }).map(function (r) { return r.glyph }).join("|") }
    var mountedShare = { group: "network", kind: "share", uri: "smb://h/data/", mounted: true }
    check("a mounted saved place opens like every other Flea menu",
          railLabels(Menu.railEntries(mountedShare)), "Open|Unmount|Rename|Edit address|Remove from Network")
    check("and its marks are the main menu's own",
          railGlyphs(Menu.railEntries(mountedShare)), "folder-open|drive|rename|sliders|minus")
    check("a group change draws a separator", Menu.railEntries(mountedShare).filter(function (r) { return r.separator === true }).length, 3)
    var stick = { group: "device", kind: "volume", device: "/dev/sda1", mounted: true, removable: true, volumeMenu: true }
    check("a mounted volume offers Open, Unmount and Eject", railLabels(Menu.railEntries(stick)), "Open|Unmount|Eject")
    check("Mount and Unmount share the drive mark", railGlyphs(Menu.railEntries(stick)), "folder-open|drive|eject")
    var idleVolume = { group: "device", kind: "volume", device: "/dev/sda1", mounted: false, removable: false, volumeMenu: true }
    check("an unmounted volume offers the mount its activation does", railLabels(Menu.railEntries(idleVolume)), "Mount")
    var savedOnly = { group: "network", kind: "share", uri: "smb://h/data/", mounted: false }
    check("an unmounted saved place keeps Rename, Edit address and Remove from Network",
          railLabels(Menu.railEntries(savedOnly)), "Rename|Edit address|Remove from Network")
    var legacyFav = { group: "favourite", kind: "favourite" }
    check("a favourite with the Places menu off is INVENTORY's own row, never a bare Remove",
          railLabels(Menu.railEntries(legacyFav)), "Remove from Favorites")
    var openTabSpec = Menu.INVENTORY.filter(function (r) { return r[0] === "openTab" })[0]
    var shelfSpec = Menu.INVENTORY.filter(function (r) { return r[0] === "shelf" })[0]
    check("New tab keeps its own label", openTabSpec[1], "New tab")
    check("Add to shelf keeps its label and the shelf's own glyph", shelfSpec[1] + "|" + shelfSpec[2], "Add to shelf|shelf")
    check("New tab's mark is real, not the silent file fallback",
          Icons.pathFor(openTabSpec[2]) === Icons.pathFor("file"), false)
    check("Add to shelf draws the shelf's own cut glyph, not the file fallback",
          Icons.pathFor(shelfSpec[2]) === Icons.pathFor("file"), false)
    var all = Menu.listingEntries(state({ hiddenActions: [], clipboardAvailable: true, rowIsSymlink: true, hasShebang: true, cursorIsTarget: true }))
    check("stored delete id reaches permanent deletion action", entry(all, "deletePermanently").id, "delete")
    check("all optional file controls exist", ["openWith", "moveTo", "copyTo", "properties", "permissions", "makeExecutable", "copyAs", "showOriginal", "pasteAs", "openTerminal"].every(function (a) { return !!entry(all, a).action }), true)
    // Invert selection lives on the background menu beside Select all, never on a file row.
    check("while Invert selection lives on the background menu",
        entry(Menu.listingEntries(state({ hasRow: false, hiddenActions: [] })), "invertSelection").action, "invertSelection")
    function shown(a) { return entry(Menu.listingEntries(state({ hiddenActions: [], rowIsSymlink: true })), a) }
    check("Show original draws the link mark in the open group", shown("showOriginal").glyph, "symlink")
    check("permanent deletion carries danger role", entry(all, "deletePermanently").danger, true)
    // Issue 133: ui/Pane.qml hands the menu Mounts.trashable of the folder, and on a share the row that would fail is absent.
    var onShare = Menu.listingEntries(state({ hiddenActions: [], canTrash: Mounts.trashable("/run/user/1000/gvfs/smb-share:server=192.168.21.25,share=data") }))
    check("a file on an SMB share is not offered Move to Trash", entry(onShare, "trash").action, undefined)
    check("while Delete permanently, which the share can do, stays and stays enabled",
          entry(onShare, "deletePermanently").action + "|" + entry(onShare, "deletePermanently").disabled, "deletePermanently|undefined")
    check("and a local file keeps Move to Trash", entry(Menu.listingEntries(state({ canTrash: Mounts.trashable("/home/gm/Downloads") })), "trash").action, "trash")
    check("single-item actions stay present but disabled on multi-selection", ["openWith", "properties", "rename", "duplicate"].every(function (a) {
        return entry(Menu.listingEntries(state({ hiddenActions: [], selectionCount: 2 })), a).disabled === true
    }), true)
    // Permissions040: Permissions takes the whole selection, so it stays
    // enabled where the single-item rows above grey out.
    check("multi-selection keeps Permissions enabled on files",
        entry(Menu.listingEntries(state({ hiddenActions: [], selectionCount: 2, selectionModes: [0o100644, 0o100644] })), "permissions").disabled, false)
    // MenuAdditions040: Copy as holds seven leaves, Paste as three, each with
    // the board's own mark; the letters ride the menu keymap, not a hint.
    var copyLeaves = entry(Menu.listingEntries(state({ hiddenActions: [] })), "copyAs").submenu
    check("Copy as holds Path, Relative path, Name, Stem, Folder path, URI and Shell-quoted",
        copyLeaves.map(function (r) { return r.id }).join(","), "copyPath,copyRelative,copyName,copyStem,copydirpath,copyUri,copyQuoted")
    check("and each leaf wears the board's mark",
        copyLeaves.map(function (r) { return r.glyph }).join(","), "file-text,file,type,type,folder,globe,terminal")
    var pasteLeaves = entry(Menu.listingEntries(state({ hiddenActions: [], clipboardAvailable: true })), "pasteAs").submenu
    check("Paste as holds Link, Absolute link and Hard link",
        pasteLeaves.map(function (r) { return r.id }).join(","), "pasteLink,pasteAbsoluteLink,pasteHardLink")
    check("and Link shares Paste's mark rather than repeating it",
        pasteLeaves.map(function (r) { return r.glyph }).join(","), "symlink,symlink,copy")
    check("a linkless filesystem offers no Paste as rows at all",
        entry(Menu.listingEntries(state({ hiddenActions: [], clipboardAvailable: true, canLink: false })), "pasteAs").action, undefined)
    check("a read-only folder turns its background write rows off",
        ["newFolder", "newFile", "paste"].map(function (a) {
            return entry(Menu.listingEntries(state({ hasRow: false, hiddenActions: [], clipboardAvailable: true, dirWritable: false })), a).disabled
        }).join(","), "true,true,true")
    check("and its row write rows too",
        ["duplicate", "rename", "trash"].map(function (a) {
            return entry(Menu.listingEntries(state({ hiddenActions: [], dirWritable: false })), a).disabled
        }).join(","), "true,true,true")
    check("while a writable folder leaves them enabled",
        ["newFolder", "duplicate", "trash"].map(function (a) {
            var rows = a === "newFolder" ? Menu.listingEntries(state({ hasRow: false, hiddenActions: [], dirWritable: true }))
                                         : Menu.listingEntries(state({ hiddenActions: [], dirWritable: true }))
            return entry(rows, a).disabled === true
        }).join(","), "false,false,false")
    check("Show original is absent except on a symlink",
        entry(Menu.listingEntries(state({ hiddenActions: [] })), "showOriginal").action, undefined)
    check("and present on one",
        entry(Menu.listingEntries(state({ hiddenActions: [], rowIsSymlink: true })), "showOriginal").action, "showOriginal")
    check("Invert selection is absent with nothing selected",
        entry(Menu.listingEntries(state({ hasRow: false, selectionCount: 0, hiddenActions: [] })), "invertSelection").action, undefined)
    check("and present once something is",
        entry(Menu.listingEntries(state({ hasRow: false, hiddenActions: [] })), "invertSelection").action, "invertSelection")
    // MenuAdditions040 callout 9: the background shows Open in terminal at defaults, while file
    // and place keep it behind the switch.
    check("the background shows Open in terminal while it stays hidden",
        entry(Menu.listingEntries(state({ hasRow: false })), "openTerminal").action, "openTerminal")
    check("and the file menu hides it behind that same switch",
        entry(Menu.listingEntries(state({})), "openTerminal").action, undefined)
    check("with the switch off the file menu shows it too",
        entry(Menu.listingEntries(state({ hiddenActions: ["delete"] })), "openTerminal").action, "openTerminal")
    MenuHunt.executable(check, state, entry, actions)
    check("missing converter removes Convert", entry(Menu.listingEntries(state({ canConvert: false })), "convert").action, undefined)
    check("missing archiver removes Compress", entry(Menu.listingEntries(state({ archiveFormats: [] })), "compress").action, undefined)
    var noReader = entry(Menu.listingEntries(state({ rowIsArchive: true, canExtract: false, archiveFormats: ["zip", "tar"] })), "extract")
    check("a recognized archive with no reader keeps its Extract row", noReader.action, "extract")
    check("and the disabled row carries the missing reader as its reason",
          noReader.disabled + "|" + noReader.hint + "|" + noReader.hintWrap, "true|7-Zip is not installed|true")
    check("supported archive offers Extract", entry(Menu.listingEntries(state({ rowIsArchive: true })), "extract").action, "extract")
    check("absent Taildrop provider removes its row", entry(Menu.listingEntries(state({ taildropInstalled: false })), "taildrop").action, undefined)
    var offline = entry(Menu.listingEntries(state({ taildropPeers: [] })), "taildrop")
    check("offline installed Taildrop remains disabled", offline.disabled, true)
    check("offline Taildrop reads as an error, with no sentence beside it", offline.errored + "|" + offline.hint, "true|undefined")
    check("offline Taildrop offers no stale peers", offline.submenu.length, 0)
    check("fresh status disables a cached Taildrop target", entry(Menu.listingEntries(state({taildropRefreshing: true})), "taildrop").disabled, true)
    check("a provider still being read is dimmed, not errored", String(entry(Menu.listingEntries(state({taildropRefreshing: true})), "taildrop").errored), "undefined")
    check("fresh status disables a cached Dropbox directory", entry(Menu.listingEntries(state({dropboxRefreshing: true})), "dropbox").disabled, true)
    check("a Taildrop still being read leaves Dropbox as it was answered", entry(Menu.listingEntries(state({taildropRefreshing: true})), "dropbox").disabled, false)
    check("installed signed-out provider reads as an error", entry(Menu.listingEntries(state({taildropPeers: [], taildropReason: "signed out"})), "taildrop").errored, true)
    var absent = Menu.listingEntries(state({taildropInstalled: false, dropboxInstalled: false}))
    var available = Menu.listingEntries(state({}))
    var trashCursor = absent.findIndex(function(row) { return row.action === "trash" })
    var refreshed = MenuRefresh.refreshedCursor(absent, available, trashCursor, -1, 0)
    check("new provider inventory inserts both actual providers", available.filter(function(row) { return row.action === "taildrop" || row.action === "dropbox" }).length, 2)
    check("new providers do not steal the current action", available[refreshed.cursor].action, "trash")
    refreshed = MenuRefresh.refreshedCursor(available, absent, refreshed.cursor, -1, 0)
    check("removed providers leave the existing action selected", absent[refreshed.cursor].action, "trash")
    check("removed providers are absent from fresh inventory", absent.filter(function(row) { return row.action === "taildrop" || row.action === "dropbox" }).length, 0)
    var oldPeers = [{action: "taildrop", submenu: [{id: "b"}]}]
    var newPeers = [{action: "taildrop", submenu: [{id: "a"}, {id: "b"}]}]
    refreshed = MenuRefresh.refreshedCursor(oldPeers, newPeers, 0, 0, 0)
    check("a submenu keeps its target id when peers reorder", refreshed.submenuCursor, 1)
    check("a retained submenu keeps its parent action", refreshed.submenuRow, 0)
    refreshed = MenuRefresh.refreshedCursor(oldPeers, [{action: "taildrop", submenu: [{id: "a"}]}], 0, 0, 0)
    check("a vanished peer closes rather than retargeting the submenu", refreshed.submenuRow, -1)
    refreshed = MenuRefresh.refreshedCursor([{action: "taildrop"}], [{separator: true}, {action: "copy", disabled: true}, {action: "open"}], 0, -1, 0)
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
    // Issue 193: a row that cannot act reads red with no sentence beside it, never "Unavailable" (GM, 2026-09-24).
    function refusal(e) { return e.disabled + "|" + e.errored + "|" + e.hint }
    check("permissions on a symlink reads as an error with no sentence", refusal(Menu.permissionsEntry(0o120777, 1)), "true|true|undefined")
    check("permissions on a row whose mode could not be read does too", refusal(Menu.permissionsEntry(0, 1)), "true|true|undefined")
    check("and a file's row can act, so it is not red", refusal(Menu.permissionsEntry(0o100644, 1)), "false|false|undefined")
    var perms = function (changes) { return entry(Menu.listingEntries(state(changes)), "permissions") }
    check("the menu row under a parent without execute reads red with no sentence",
          refusal(perms({ hiddenActions: [], rowMode: 0 })), "true|true|undefined")
    check("the menu row for a multi-selection reads plain on files",
        refusal(perms({ hiddenActions: [], selectionCount: 2, selectionModes: [0o100644, 0o100644] })), "false|undefined|undefined")
    check("and red when one of them is a link",
        refusal(perms({ hiddenActions: [], selectionCount: 2, selectionModes: [0o100644, 0o120777] })), "true|true|undefined")
    check("the menu row for a symlink reads the same way", refusal(perms({ hiddenActions: [], rowMode: 0o120777 })), "true|true|undefined")
    check("the menu row for a folder it can change is plain", refusal(perms({ hiddenActions: [], rowMode: 0o040755 })), "false|undefined|undefined")
    check("permissions rejects missing metadata", Menu.permissionsEntry(undefined, 1).disabled, true)
    check("permissions rejects fifo", Menu.permissionsEntry(0o010644, 1).disabled, true)
    check("permissions allows read-only inspection of special bits", Menu.permissionsEntry(0o104755, 1).disabled, false)
    // Issue 193 follow-up (GM, 2026-09-24): a right click on the Locked tile opens the
    // locked folder's own menu, never the parent's background one.
    var locked = LockedMenu.lockedEntries({ lockedMode: 0o040000, hiddenActions: [] })
    check("the Locked tile offers only rows that act without listing the folder",
          actions(locked), "openTerminal,permissions,copypath")
    // One check per locked row: each offered action is the dispatch ui/Pane.qml performLocked
    // switches on, so a row renamed on either side strands the other. The signal carry itself
    // (ContextMenu.lockedChosen) is QML-only and is covered by the controller's live check.
    check("its Open in Terminal row carries the terminal dispatch",
          entry(locked, "openTerminal").action, "openTerminal")
    check("its Permissions row carries the permissions dispatch",
          entry(locked, "permissions").action, "permissions")
    check("its Copy path row carries the copy dispatch",
          entry(locked, "copypath").action, "copypath")
    check("and no row that would create in, paste into or sort the parent",
          ["newFolder", "newFile", "paste", "selectAll", "sort", "toggleHidden", "settings"].every(function (a) {
              return entry(locked, a).action === undefined
          }), true)
    check("Permissions stays plain while its owner may still be the operator, the way back in",
          refusal(entry(locked, "permissions")), "false|undefined|undefined")
    var stranger = LockedMenu.lockedEntries({ lockedMode: 0o040755, hiddenActions: [] })
    check("Permissions for a folder owned by somebody else reads red with no sentence",
          refusal(entry(stranger, "permissions")), "true|true|undefined")
    var unreadable = LockedMenu.lockedEntries({ lockedMode: 0, hiddenActions: [] })
    check("Permissions with no mode to read reads red with no sentence too",
          refusal(entry(unreadable, "permissions")), "true|true|undefined")
    check("hiding Open in terminal takes it off the Locked tile as well",
          entry(LockedMenu.lockedEntries({ lockedMode: 0o040000, hiddenActions: ["openTerminal"] }), "openTerminal").action, undefined)
    var shippedHidden = ["delete", "openTerminal", "placeMenu", "runScript", "moveto", "copyto", "properties", "permissions", "copyAs", "extThumbs"]
    check("the shipped hidden set leaves the Locked tile with no row",
          LockedMenu.lockedEntries({ lockedMode: 0o040000, hiddenActions: shippedHidden }).length, 0)
    check("and it refuses with the Menus switch sentence",
          LockedMenu.lockedRefusal({ lockedMode: 0o040000, hiddenActions: shippedHidden }),
          "Open in terminal, Permissions and Copy as are hidden in Settings > Menus.")
    check("one row shown yields that row alone",
          actions(LockedMenu.lockedEntries({ lockedMode: 0o040000, hiddenActions: ["openTerminal", "permissions"] })), "copypath")
    check("and it refuses nothing then",
          LockedMenu.lockedRefusal({ lockedMode: 0o040000, hiddenActions: ["openTerminal", "permissions"] }), "")
    check("menu fits at pointer", Menu.clamp(20, 80, 300), 20)
    check("menu flips before shifting", Menu.clamp(270, 80, 300), 190)
    check("oversized menu pins to near edge", Menu.clamp(30, 500, 300), 0)
    check("negative point clamps", Menu.clamp(-20, 80, 300), 0)
    check("submenu array is recognized even when empty", Menu.hasSubmenu({ submenu: [] }), true)
    check("ordinary entry has no submenu", Menu.hasSubmenu({ action: "open" }), false)
    check("missing entry has no submenu", Menu.hasSubmenu(undefined), false)
    check("header keeps required Name column outside toggles", actions(Menu.headerEntries([], false)), "col:mode,col:size,col:date,col:kind,toggleHidden")
    check("sort submenu uses real backend order ids", Menu.sortEntries().map(function (r) { return r.id }).join(","), "name,size,mtime,kind")
    providerRefresh(check)
}

function merged(base, changes) {
    var out = {}
    for (var key in base) out[key] = base[key]
    for (key in changes) out[key] = changes[key]
    return out
}

// The rows a file menu draws from one set of provider inputs, through ui/js/MenuRefresh.js the way ui/ContextMenu.qml builds them.
function drawn(known, live) {
    var view = MenuRefresh.providerView(known, live)
    return Menu.listingEntries(state({ taildropPeers: view.taildropPeers, taildropRefreshing: view.taildropRefreshing,
        dropboxPath: view.dropboxPath, dropboxRefreshing: view.dropboxRefreshing, localSendInstalled: true,
        localSendPeers: [{ id: "Phone", label: "Phone" }], localSendChecking: view.localSendChecking }))
}

// An open refreshes its providers behind the menu; the last answer stands in, so only a changed answer rebuilds rows.
function providerRefresh(check) {
    check("the menu keeps the last provider answer", typeof MenuRefresh.settle + "|" + typeof MenuRefresh.providerView, "function|function")
    if (typeof MenuRefresh.settle !== "function") return
    var answer = { refreshing: false, taildropInstalled: true, taildropPeers: [{ id: "box", label: "Box" }],
        dropboxInstalled: true, dropboxPath: "/tmp/Dropbox", localSendChecking: false, localSendAnswered: true }
    var known = MenuRefresh.settle(answer)
    var before = drawn(known, answer)
    // While the status helpers run, Taildrop has cleared its peers, Dropbox is not ready and LocalSend is looking again.
    var reading = merged(answer, { refreshing: true, taildropPeers: [], dropboxPath: "", localSendChecking: true })
    var during = drawn(known, reading)
    check("a refresh behind the open menu builds exactly the rows drawn", MenuRefresh.unchanged(before, during), true)
    check("so Taildrop keeps its peer and stays live", entry(during, "taildrop").disabled + "|" + entry(during, "taildrop").submenu.length, "false|1")
    check("Dropbox keeps its folder", entry(during, "dropbox").disabled, false)
    check("and LocalSend stays live while it looks again", entry(during, "localsend").disabled, false)
    known = MenuRefresh.settle(answer)
    check("the same answer landing rebuilds nothing", MenuRefresh.unchanged(before, drawn(known, answer)), true)
    var more = merged(answer, { taildropPeers: [{ id: "box", label: "Box" }, { id: "laptop", label: "Laptop" }] })
    known = MenuRefresh.settle(more)
    var after = drawn(known, more)
    check("a changed answer is a change", MenuRefresh.unchanged(before, after), false)
    check("and it moves only the Taildrop row", before.map(function (row, i) {
        return JSON.stringify(row) === JSON.stringify(after[i]) ? "" : row.action }).filter(String).join(","), "taildrop")
    check("which now offers both peers", entry(after, "taildrop").submenu.map(function (peer) { return peer.id }).join(","), "box,laptop")
    var gone = merged(answer, { dropboxPath: "" })
    after = drawn(MenuRefresh.settle(gone), gone)
    check("a Dropbox that stopped answering reads red, and only its row moves", before.map(function (row, i) {
        return JSON.stringify(row) === JSON.stringify(after[i]) ? "" : row.action }).filter(String).join(",") + "|" + entry(after, "dropbox").errored, "dropbox|true")
    var first = drawn(null, merged(reading, { localSendAnswered: false }))
    check("with nothing answered yet the first refresh dims Taildrop, as it always did", entry(first, "taildrop").disabled + "|" + String(entry(first, "taildrop").errored), "true|undefined")
    check("and Dropbox", entry(first, "dropbox").disabled + "|" + String(entry(first, "dropbox").errored), "true|undefined")
    check("and LocalSend", entry(first, "localsend").disabled + "|" + String(entry(first, "localsend").errored), "true|undefined")
    var reinstalled = drawn(MenuRefresh.settle(merged(answer, { taildropInstalled: false, taildropPeers: [] })), reading)
    check("a provider installed since its last answer is read afresh, dimmed rather than red",
          entry(reinstalled, "taildrop").disabled + "|" + String(entry(reinstalled, "taildrop").errored), "true|undefined")
    // The one key step both the menu and its flyout take: separators and disabled rows are skipped, an end holds.
    var steps = [{ action: "open" }, { separator: true }, { action: "cut", disabled: true }, { action: "copy" }]
    check("a step skips a separator and a disabled row", Menu.stepRow(steps, 0, 1), 3)
    check("and back again", Menu.stepRow(steps, 3, -1), 0)
    check("an end keeps the cursor where it is", Menu.stepRow(steps, 3, 1) + "|" + Menu.stepRow(steps, 0, -1), "3|0")
    check("the opening cursor is the first row a step from before the top reaches", Menu.stepRow(steps.slice(1), -1, 1), 2)

    // Issue 193: a Locked tile names the refused folder for its menu; any other state names nothing.
    function lockedAs(path, asked, state) { return Nav.lockedTarget({ path: path, listingPath: asked, listingState: state }) }
    check("refused hop names the ask, re-read names itself, ready and error name nothing", lockedAs("/d", "/d/locked", "locked") + "|" + lockedAs("/d", "/d", "locked") + "|" + lockedAs("/d", "/d", "ready") + "|" + lockedAs("/d", "/d", "error"), "/d/locked|/d||")
    check("a refused bookmark with a trailing slash names the folder without it", lockedAs("/d", "/root/", "locked"), "/root")

    // A hidden row still opens its flyout, built from the action.
    var hiddenRows = Menu.listingEntries(state({ clipboardAvailable: true }))
    check("the shipped defaults hide the Copy as row", entry(hiddenRows, "copyAs").action || "absent", "absent")
    check("and the Paste as row", entry(hiddenRows, "pasteAs").action || "absent", "absent")
    check("yet Copy as still builds its flyout from the action",
          Menu.flyoutEntries("copyAs").map(function (l) { return l.id }).join(","),
          "copyPath,copyRelative,copyName,copyStem,copydirpath,copyUri,copyQuoted")
    check("and Paste as builds its own",
          Menu.flyoutEntries("pasteAs").map(function (l) { return l.id }).join(","),
          "pasteLink,pasteAbsoluteLink,pasteHardLink")
    check("while any other action builds no flyout", Menu.flyoutEntries("trash").length, 0)
    // Lone flyout opens only with no row and an available action; otherwise it refuses.
    check("Menu decides lone vs refuse in one place", typeof Menu.submenuFor === "function", true)
    var loneFor = Menu.submenuFor || function () { return { kind: "none" } }
    var loneHidden = Menu.listingEntries(state({ clipboardAvailable: true }))
    check("hidden Copy as with clipboard opens lone", loneFor("copyAs", loneHidden, true).kind, "lone")
    check("hidden Paste as with clipboard opens lone", loneFor("pasteAs", loneHidden, true).kind, "lone")
    var loneEmpty = Menu.listingEntries(state({ clipboardAvailable: false }))
    check("hidden Paste as with empty clipboard refuses", loneFor("pasteAs", loneEmpty, false).kind, "refuse")
    check("and names the empty-clipboard sentence", Menu.EMPTY_CLIPBOARD || "", "There is nothing to paste; y copies and x cuts.")
    var shownEmpty = Menu.listingEntries(state({ clipboardAvailable: false, hiddenActions: [] }))
    check("shown Paste as with empty clipboard refuses", loneFor("pasteAs", shownEmpty, false).kind, "refuse")
    var shownFull = Menu.listingEntries(state({ clipboardAvailable: true, hiddenActions: [] }))
    check("shown Paste as with clipboard opens its row", loneFor("pasteAs", shownFull, true).kind, "row")
    check("shown Copy as opens its row", loneFor("copyAs", shownFull, true).kind, "row")
    check("an action with no flyout opens nothing", loneFor("trash", shownFull, true).kind, "none")
    check("Menu decides a lone leaf in one place", typeof Menu.loneChoice === "function", true)
    var lonePick = Menu.loneChoice || function () { return { kind: "none" } }
    check("a known leaf with no move fires", lonePick("copyAs", "copyPath", false, false, true, "a", "a").kind, "fire")
    check("and names the fired action with its leaf", lonePick("copyAs", "copyPath", false, false, true, "a", "a").fired, "copyAs:copyPath")
    check("an unknown leaf refuses", lonePick("copyAs", "bogus", false, false, true, "a", "a").kind, "unknown")
    check("a moved selection refuses", lonePick("copyAs", "copyPath", false, false, true, "a", "b").kind, "moved")
    // A moved selection outranks an unknown leaf, the order chooseSub carried.
    check("a moved selection outranks an unknown leaf", lonePick("copyAs", "bogus", false, false, true, "a", "b").kind, "moved")
    check("a rail move never counts as moved", lonePick("copyAs", "copyPath", true, false, true, "a", "b").kind, "fire")
    // Pane root carries the cursor sequence Filter and Marks bump, so QML must declare it.
    var paneSrc = Source.source("ui/Pane.qml")
    check("Pane declares cursorSeq beside cursorIndex", paneSrc.indexOf("property int cursorSeq: 0") >= 0, true)
    check("Pane declares recentSortBy", paneSrc.indexOf('property string recentSortBy: ""') >= 0, true)
    check("Pane declares recentSortDesc", paneSrc.indexOf("property bool recentSortDesc: false") >= 0, true)
    // QML routes through the same decision, so a revert goes red here.
    var contextSrc = Source.source("ui/ContextMenu.qml")
    check("ContextMenu opens through Menu.submenuFor", contextSrc.indexOf("Menu.submenuFor(action, root.entries, root.clipboardAvailable)") >= 0, true)
    check("ContextMenu chooses through Menu.loneChoice", contextSrc.indexOf("Menu.loneChoice(root.loneFlyoutAction, id,") >= 0, true)
    check("its refusal names the empty-clipboard sentence", contextSrc.indexOf("root.refused(Menu.EMPTY_CLIPBOARD)") >= 0, true)
    var pasteBody = Source.slice(paneSrc, "function openPasteAs()", "function invertSelection")
    check("Pane.openPasteAs closes and says empty on refuse", pasteBody.indexOf("menu.close()") >= 0 && pasteBody.indexOf("There is nothing to paste; y copies and x cuts.") >= 0, true)
    // Show original answers only its own pending id, so a late reply never yanks a navigation.
    // One brace scan serves every shipped body below; an inline copy beside it is the defect this pins.
    // Sample input: functionBody("head function f(a) { return a } tail", "function f(") answers " return a ".
    function functionBody(sourceText, mark) {
        var at = sourceText.indexOf(mark)
        if (at < 0)
            throw new Error("sourcefixture: missing " + mark)
        var brace = sourceText.indexOf("{", at)
        var scan = brace + 1, depth = 1, quote = "", comment = false
        while (depth > 0 && scan < sourceText.length) {
            var ch = sourceText.charAt(scan)
            if (comment) {
                if (ch === "\n") comment = false
            } else if (quote.length > 0) {
                if (ch === quote) quote = ""
            } else if (ch === "/" && sourceText.charAt(scan + 1) === "/") comment = true
            else if (ch === '"' || ch === "'") quote = ch
            else if (ch === "{") depth += 1
            else if (ch === "}") depth -= 1
            scan += 1
        }
        if (depth > 0)
            throw new Error("sourcefixture: unterminated " + mark)
        return sourceText.substring(brace + 1, scan - 1)
    }
    var selfText = Source.source("tests/js/menu.js")
    var chooseSub = eval("(function (root, id) {" + functionBody(contextSrc, "function chooseSub(") + "})")
    var refusalBody = contextSrc.indexOf("function refuseLone(") >= 0
        ? functionBody(contextSrc, "function refuseLone(") : ""
    var refuseLone = eval("(function (root, kind) {" + refusalBody + "})")
    var validateChoice = eval("(function (root, action, subId, includeHidden) {" + functionBody(contextSrc, "function validateChoice(") + "})")
    var refusalCases = [
        { id: "copyPath", identity: "changed", reason: "Selected items changed; reopen the menu." },
        { id: "unknown", identity: "original", reason: "That action is no longer available; reopen the menu." }
    ]
    refusalCases.forEach(function (test) {
        var menu = { entries: [], openSubmenuRow: -1, loneFlyoutAction: "copyAs", forRail: false,
            forHeader: false, hasRow: true, openedIdentity: "original", selectionIdentity: test.identity,
            opened: true, validations: 0, reasons: [], fired: [] }
        menu.close = function () { menu.opened = false }
        menu.refused = function (reason) { menu.reasons.push(reason) }
        menu.chosen = function (action) { menu.fired.push(action) }
        menu.refuseLone = function (kind) { refuseLone(menu, kind) }
        menu.validateChoice = function () {
            menu.validations += 1
            return true
        }
        chooseSub(menu, test.id)
        check("lone " + test.id + " closes without validator side effects", menu.opened, false)
        check("lone " + test.id + " refuses with its named reason", menu.reasons.join("|"), test.reason)
        check("lone " + test.id + " never validates an already refused choice", menu.validations, 0)
        check("lone " + test.id + " fires nothing", menu.fired.length, 0)
        menu.opened = true
        menu.reasons = []
        menu.buildEntries = function () { return [] }
        check("normal " + test.id + " validation refuses", validateChoice(menu, "copyAs", test.id), false)
        check("normal " + test.id + " shares the lone refusal sentence", menu.reasons.join("|"), test.reason)
        check("normal " + test.id + " validation closes", menu.opened, false)
    })
    var buildArgs = []
    var sheetMenu = { entries: [], forRail: false, forHeader: false, hasRow: true, openedIdentity: "a", selectionIdentity: "a",
        close: function () {}, refused: function () {}, refuseLone: function () {},
        buildEntries: function (flyout, include) { buildArgs.push(include); return [{ action: "permissions" }] } }
    check("a sheet choice validates against the rows it listed", validateChoice(sheetMenu, "permissions", "", true), true)
    validateChoice(sheetMenu, "permissions", "")
    check("only the sheet asks the build for hidden rows", buildArgs.join("|"), "true|")
    check("the brace scan lives in one helper, not three inline loops", selfText.split("Depth +=" + " 1").length - 1, 0)
    var linkText = Source.source("ui/PaneWire.qml")
    var onLinkTarget = eval("(function (pane, path, directory, name, id) {"
        + functionBody(linkText, "function onLinkTarget(") + "})")
    function linkPane(pending) {
        var p = {linkTargetPendingId: pending, pendingSelect: "", said: [], opened: []}
        p.message = function (text) { p.said.push(text) }
        p.open = function (target) { p.opened.push(target) }
        return p
    }
    var live = linkPane(7)
    onLinkTarget(live, "/a/l", "/b", "f.txt", 7)
    check("its own id reveals the target folder", live.opened.join("|"), "/b")
    check("and selects the target row there", live.pendingSelect, "/b/f.txt")
    check("and spends the pending id", live.linkTargetPendingId, 0)
    var foreign = linkPane(7)
    onLinkTarget(foreign, "/a/l", "/b", "f.txt", 8)
    check("a foreign id opens nothing", foreign.opened.length, 0)
    check("and keeps the pending id for the real reply", foreign.linkTargetPendingId, 7)
    var idle = linkPane(0)
    onLinkTarget(idle, "/a/l", "/b", "f.txt", 0)
    check("with nothing pending even id 0 opens nothing", idle.opened.length, 0)
    var paneText = Source.source("ui/Pane.qml")
    check("Show original mints its pending id from the backend counter", paneText.indexOf("root.backend.nextLinkTargetId()") >= 0, true)
    check("and the request carries that id", paneText.indexOf("id: root.linkTargetPendingId") >= 0, true)
    check("and no pane mints an id of its own", paneText.indexOf("linkTargetPendingId += 1") < 0, true)
    // The counter never resets, so an id is never handed out twice in one process.
    var backText = Source.source("ui/Backend.qml")
    check("the backend owns the linktarget counter", backText.indexOf("linkTargetSeq") >= 0, true)
    check("and nothing ever resets it", backText.indexOf("linkTargetSeq = 0") < 0, true)
    var nextLinkTargetId = eval("(function (root) {" + functionBody(backText, "function nextLinkTargetId(") + "})")
    var sharedBackend = { linkTargetSeq: 0 }
    check("the counter rises forever", nextLinkTargetId(sharedBackend) + "|" + nextLinkTargetId(sharedBackend), "1|2")
    // Both entrances send through here, so the test runs the shipped body, not a copy.
    var requestLinkTarget = eval("(function (root, path) {" + functionBody(paneText, "function requestLinkTarget(") + "})")
    function linkRequester() {
        var stub = { linkTargetPendingId: 0, pendingSelect: "", said: [], opened: [], sent: [] }
        stub.message = function (text) { stub.said.push(text) }
        stub.open = function (target) { stub.opened.push(target) }
        stub.backend = { send: function (line) { stub.sent.push(line) } }
        stub.backend.nextLinkTargetId = function () { return nextLinkTargetId(sharedBackend) }
        return stub
    }
    var firstPane = linkRequester()
    var secondPane = linkRequester()
    requestLinkTarget(firstPane, "/a/first")
    requestLinkTarget(secondPane, "/b/first")
    check("two panes sharing one backend get different ids", firstPane.linkTargetPendingId === secondPane.linkTargetPendingId, false)
    check("and each request carries its pending id", firstPane.sent[0].id === firstPane.linkTargetPendingId && secondPane.sent[0].id === secondPane.linkTargetPendingId, true)
    // Exactly one request stands when the navigation lands, so a per-pane counter would hand its id out again.
    var navPane = linkRequester()
    requestLinkTarget(navPane, "/a/second")
    var staleId = navPane.sent[0].id
    // A navigation drops the wait, which is what ui/js/Nav.js openWithoutHistory writes.
    navPane.linkTargetPendingId = 0
    requestLinkTarget(navPane, "/a/third")
    var freshId = navPane.linkTargetPendingId
    check("a navigation and a new request never reuse an id", staleId === freshId, false)
    onLinkTarget(navPane, "/a/second", "/b", "f.txt", staleId)
    check("a reply for the older id opens nothing", navPane.opened.length, 0)
    check("and keeps waiting for the new one", navPane.linkTargetPendingId, freshId)
    onLinkTarget(navPane, "/a/third", "/c", "g.txt", freshId)
    check("the new reply still reveals its folder", navPane.opened.join("|"), "/c")
    // Show original has one route out, so a raw send cannot bypass the pending id.
    var pmaText = Source.source("ui/PaneMenuActions.qml")
    check("Show original sends through requestLinkTarget", pmaText.indexOf("requestLinkTarget") >= 0, true)
    check("and sends no raw linktarget", pmaText.indexOf('"linktarget"') < 0, true)
    // Source pin: a menu leaf copies through Ops.copyAs, so Recent's refusal covers it too.
    check("Copy as leaves copy through Ops.copyAs", pmaText.indexOf("Ops.copyAs(root.pane, kind, taken)") >= 0, true)
    check("and never format their own text", pmaText.indexOf("CopyAs.lines") < 0, true)
}
