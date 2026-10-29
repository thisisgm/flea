.import "../../ui/js/SheetQuery.js" as SheetQuery
.import "sourcefixture.js" as Source
.import "../../ui/js/Keymap.js" as Keymap
.import "../../ui/js/Menu.js" as Menu

function run(check) {
    // Candidates from stub menu, rail and recent models, plus the Enter dispatch.
    function hint(action) {
        if (action === "deletePermanently") {
            return "shift-delete"
        }
        if (action === "trash") {
            return "dd"
        }
        return ""
    }
    var entries = [
        { id: "trash", action: "trash", label: "Move to Trash" },
        { id: "deletePermanently", action: "deletePermanently", label: "Delete permanently", danger: true },
        { id: "permissions", action: "permissions", label: "Permissions" },
        { id: "paste", action: "paste", label: "Paste", disabled: true },
        { id: "compress", action: "compress", label: "Compress",
          submenu: [{ id: ".zip", label: "Compress to .zip" }, { separator: true },
                    { id: ".7z", label: "Compress to .7z", disabled: true }] }
    ]
    var menus = SheetQuery.menuCandidates(entries, hint)
    // Five top-level entries plus two flyout leaves, the flyout separator skipped.
    check("every non-separator entry becomes a candidate", menus.length, 7)
    // The sheet keeps hidden rows findable by asking the menu for its hidden rows too.
    var sheet = Source.source("ui/KeymapSheet.qml")
    var menuSlice = Source.slice(sheet, "function menuModel()", "function railModel()")
    var codeLines = menuSlice.split("\n").filter(function (line) { return line.trim().indexOf("//") !== 0 })
    var codeSlice = codeLines.join("\n")
    var hiddenCount = codeSlice.split('listingContext("", true)').length - 1
    check("the sheet asks for the hidden rows once", hiddenCount, 1)
    check("and never trims the set itself", codeSlice.indexOf("context.hiddenActions") >= 0, false)
    check("a row with a key keeps its cap",
          menus.filter(function (row) { return row.label === "Delete permanently"; })[0].keys, "shift-delete")
    check("a keyless row draws no cap",
          menus.filter(function (row) { return row.label === "Permissions"; })[0].keys, "")
    check("a leaf reads its flyout for the muted suffix",
          menus.filter(function (row) { return row.label === "Compress to .zip"; })[0].where, "Compress")
    check("a leaf carries no cap",
          menus.filter(function (row) { return row.label === "Compress to .zip"; })[0].keys, "")
    check("a flyout with a separator yields its two leaves",
          menus.filter(function (row) { return row.where === "Compress"; }).length, 2)
    check("and no row has an empty label",
          menus.some(function (row) { return row.label.length === 0; }), false)
    check("a disabled row is flagged",
          menus.filter(function (row) { return row.label === "Paste"; })[0].disabled, true)
    check("a disabled leaf is flagged too",
          menus.filter(function (row) { return row.label === "Compress to .7z"; })[0].disabled, true)
    // The rail's places read "Open <name>" with the group name the rail shows.
    var rail = [
        { label: "flea", group: "favourite", kind: "favourite", path: "/home/gm/flea" },
        { label: "Trash", group: "trash", kind: "trash", path: "trash:///" },
        { label: "NAS", group: "network", kind: "share", uri: "smb://host/data" },
        { label: "Recent", group: "recent", kind: "recent", path: "flea:recent" }
    ]
    var places = SheetQuery.placeCandidates(rail)
    check("a favourite says Favorites",
          places.filter(function (row) { return row.name === "flea"; })[0].where, "Favorites")
    check("trash says Places",
          places.filter(function (row) { return row.name === "Trash"; })[0].where, "Places")
    check("a share keeps its group name",
          places.filter(function (row) { return row.name === "NAS"; })[0].where, "Network")
    check("the Recent row is no place",
          places.some(function (row) { return row.name === "Recent"; }), false)
    check("a place reads Open plus its name",
          places.filter(function (row) { return row.name === "Trash"; })[0].label, "Open Trash")
    // Recent files read "Open <file>" with the home-abbreviated parent.
    var recents = SheetQuery.recentCandidates(["/home/gm/Documents/claude/mix.flac", "/etc/hosts"], "/home/gm")
    check("a recent file reads Open plus its leaf", recents[0].label, "Open mix.flac")
    check("its parent is home abbreviated", recents[0].where, "~/Documents/claude")
    check("a root outside home is left whole", recents[1].where, "/etc")
    // Enter runs the highlighted row as its own surface would.
    var actionRow = SheetQuery.actionCandidates([{ label: "trash", keys: "dd", action: "trash", context: "listing" }])[0]
    check("an action dispatches as its key", SheetQuery.dispatch(actionRow).kind, "action")
    var menuRow = menus.filter(function (row) { return row.label === "Move to Trash"; })[0]
    check("a menu row dispatches as the menu", SheetQuery.dispatch(menuRow).kind, "menu")
    var confirmRow = menus.filter(function (row) { return row.label === "Delete permanently"; })[0]
    var confirm = SheetQuery.dispatch(confirmRow)
    check("a destructive row keeps its confirm", confirm.kind, "confirm")
    check("through the row the menu would run", confirm.menuAction, "deletePermanently")
    var placeRow = places.filter(function (row) { return row.name === "Trash"; })[0]
    check("a place dispatches as a rail click", SheetQuery.dispatch(placeRow).kind, "place")
    check("a recent file dispatches as its row", SheetQuery.dispatch(recents[0]).kind, "recent")
    var disabledRow = menus.filter(function (row) { return row.label === "Paste"; })[0]
    check("a disabled row never runs", SheetQuery.dispatch(disabledRow).kind, "disabled")
    // One action, one row (CommandPalette "After typing perm"): an action that is both a key and a menu row lists once, as its key.
    var cursorOnFile = { hasRow: true, hiddenActions: [], selectionCount: 1, rowMode: 0o100644, selectionModes: [0o100644],
        cursorIsTarget: true, hasShebang: true, rowIsFile: true, rowIsSymlink: true, rowIsArchive: true, rowIsImage: true,
        clipboardAvailable: true, canTrash: true, canConvert: true, canExtract: true, archiveFormats: ["zip"],
        storageClass: "network", dropboxInstalled: true, taildropInstalled: true, openWithLoaded: true, openWithApps: [] }
    var realActions = SheetQuery.actionCandidates(Keymap.sheetFor("default", "gui", false))
    var realMenus = SheetQuery.menuCandidates(Menu.listingEntries(cursorOnFile), function (a) { return Keymap.hintFor(a) })
    var keyed = {}
    realActions.forEach(function (row) { keyed[row.action] = true })
    var both = realMenus.filter(function (row) { return keyed[row.menuAction] === true }).map(function (row) { return row.menuAction })
    // The overlap on the default preset: open cut copy paste pasteAs rename trash deletePermanently openTerminal copyAs toggleHidden.
    check("the shipped menu and key table overlap", both.indexOf("deletePermanently") >= 0 && both.indexOf("trash") >= 0, true)
    check("SheetQuery.unique exists", typeof SheetQuery.unique, "function")
    var merged = typeof SheetQuery.unique === "function" ? SheetQuery.unique(realActions.concat(realMenus)) : realActions.concat(realMenus)
    both.forEach(function (name) {
        var runs = merged.filter(function (row) { return row.action === name || row.menuAction === name })
        check("one row runs " + name, runs.length, 1)
        check(name + " keeps the key row the board draws", runs.length === 1 ? runs[0].section : -1, 0)
    })
    var perm = SheetQuery.rank(realActions.concat(realMenus), "perm").map(function (row) { return row.keys + " " + row.label })
    check("perm lists delete permanently once, then Permissions", perm.join("|"), "shift-delete Delete permanently| Permissions")
    var trash = SheetQuery.rank(realActions.concat(realMenus), "trash").filter(function (row) {
        return row.action === "trash" || row.menuAction === "trash" })
    check("trash lists the file menu's Move to Trash once", trash.length, 1)
    check("and keeps the key's own wording when both match", trash[0].label, "Move to Trash")
    // One row per action, but a word only the menu's wording holds still finds it, under its key and in the wording that matched.
    var trashKey = realActions.filter(function (row) { return row.action === "trash" })[0]
    var viaMenu = SheetQuery.rank(realActions.concat(realMenus), "move to trash")
    check("a word only the menu wording holds finds the action once", viaMenu.length, 1)
    check("under the key row's cap", viaMenu.length === 1 ? viaMenu[0].keys : "none", trashKey.keys)
    check("as the key row, so Enter runs the key", viaMenu.length === 1 ? viaMenu[0].action : "none", "trash")
    check("in the menu's wording, the one that matched", viaMenu.length === 1 ? viaMenu[0].label : "none", "Move to Trash")
    var viaKey = SheetQuery.rank(realActions.concat(realMenus), "trash")
    check("a word both wordings hold lists the action once", viaKey.filter(function (row) { return row.action === "trash" }).length, 1)
    // A leaf's run is its flyout action plus its own id (SheetQuery.actionWithSub), so two formats and two apps stay four rows.
    var leafEntries = [
        { id: "compress", action: "compress", label: "Compress",
          submenu: [{ id: "zip", label: ".zip" }, { id: "tar", label: ".tar" }] },
        { id: "openWith", action: "openWith", label: "Open with",
          submenu: [{ id: "a.desktop", label: "Alpha" }, { id: "b.desktop", label: "Beta" }] }
    ]
    var leaves = SheetQuery.unique(SheetQuery.actionCandidates(Keymap.sheetFor("default", "gui", false)).concat(
        SheetQuery.menuCandidates(leafEntries, function (a) { return Keymap.hintFor(a) })))
    var leafRuns = leaves.filter(function (row) { return row.where === "Compress" || row.where === "Open with" })
        .map(function (row) { return row.menuAction })
    check("two formats and two apps survive the dedupe as four distinct runs", leafRuns.join("|"),
          "compress:zip|compress:tar|openWith:a.desktop|openWith:b.desktop")
    check("flyout leaves survive the dedupe", merged.filter(function (row) { return row.where === "Compress" }).length,
          realMenus.filter(function (row) { return row.where === "Compress" }).length)
    // CommandPalette callout 3: a key that works in the media preview only says so, in the name the UI shows for it.
    var muteRow = SheetQuery.rank(realActions, "mute").filter(function (row) { return row.action === "mute" })[0]
    check("mute is listed with its own key", muteRow ? muteRow.keys : "none", "m")
    check("and says where it works", muteRow ? muteRow.where : "none", "Preview")
    check("a listing key says nothing", realActions.filter(function (row) { return row.action === "rename" })[0].where, "")
    check("no row carries a raw context id", realActions.some(function (row) { return row.where === "media" || row.where === "all" }), false)
    // CommandPalette "After typing comp": the Compress flyout's leaves list alone, in archive.rs's order, with no muted suffix and no parent row.
    var archiveOrder = ["zip", "tar", "tar.gz", "tar.bz2", "tar.xz", "tar.zst", "7z"]
    var compressContext = {}
    for (var field in cursorOnFile) compressContext[field] = cursorOnFile[field]
    compressContext.archiveFormats = archiveOrder
    compressContext.openWithApps = [{ id: "alpha.desktop", label: "Alpha" }]
    compressContext.scripts = [{ id: "run.sh", label: "Run me" }]
    compressContext.taildropPeers = [{ id: "phone", label: "Phone" }]
    compressContext.hasFolderSort = true
    var compressEntries = Menu.listingEntries(compressContext)
    var compressRows = SheetQuery.menuCandidates(compressEntries, function (a) { return Keymap.hintFor(a) })
    var everyRow = SheetQuery.actionCandidates(Keymap.sheetFor("default", "gui", false)).concat(compressRows)
    var comp = SheetQuery.rank(everyRow, "comp")
    var compressWanted = archiveOrder.map(function (fmt) { return "Compress to ." + fmt })
    check("comp lists every leaf in archive.rs's order and no parent row",
          comp.map(function (row) { return row.label }).join("|"), compressWanted.join("|"))
    check("a Compress leaf carries no muted suffix", comp.length > 0 && comp.every(function (row) { return row.where === "" }), true)
    check("and no cap", comp.every(function (row) { return row.keys === "" }), true)
    check("a Compress leaf still runs its format", comp.length > 0 ? comp[0].menuAction : "none", "compress:zip")
    // Every flyout parent the menu builds: a query that lists one of its leaves lists no row of the parent, while a query only the parent matches keeps it.
    var parents = compressEntries.filter(function (entry) { return Menu.hasSubmenu(entry) })
    check("the menu builds several flyout parents", parents.length >= 5, true)
    parents.forEach(function (parent) {
        var action = String(parent.action || parent.id)
        var leaf = (parent.submenu || []).filter(function (item) { return item.separator !== true && String(item.label || "").length > 0 })[0]
        if (leaf === undefined) return
        var leafLabel = String(leaf.sheetLabel || leaf.label)
        var listed = SheetQuery.rank(everyRow, leafLabel)
        check(parent.label + ": its leaf " + leafLabel + " is listed", listed.some(function (row) { return row.menuAction === action + ":" + String(leaf.id) }), true)
        check(parent.label + ": no pure menu row of the parent stands beside its leaf",
              listed.filter(function (row) { return row.section === 1 && row.menuAction === action }).length, 0)
        // Whatever wording a leaf carries, one that matches beside its parent's name leaves the parent out.
        var twin = SheetQuery.menuCandidates([{ id: action, action: action, label: parent.label,
            submenu: [{ id: "more", label: String(parent.label) + " more" }] }], function () { return "" })
        var twinRows = SheetQuery.rank(twin, String(parent.label).toLowerCase())
        check(parent.label + ": a leaf matching beside its parent leaves the parent out", twinRows.map(function (row) { return row.menuAction }).join("|"), action + ":more")
        var alone = SheetQuery.rank(everyRow, String(parent.label).toLowerCase())
        var aloneLeaves = alone.filter(function (row) { return row.parentAction === action }).length
        check(parent.label + ": the parent's own name lists the parent or its leaves, never neither",
              alone.some(function (row) { return row.menuAction === action }) || aloneLeaves > 0, true)
    })
    // Sort by lives in the background menu; its leaves list without it the same way.
    var sortParent = Menu.listingEntries({ hasRow: false, hiddenActions: [], selectionCount: 0, hasFolderSort: true }).filter(function (entry) { return entry.action === "sort" })[0]
    check("the background menu builds the Sort by flyout", sortParent !== undefined && Menu.hasSubmenu(sortParent), true)
    var sortRows = SheetQuery.rank(SheetQuery.menuCandidates([sortParent], function () { return "" }), "name")
    check("a sort leaf lists without a Sort by row", sortRows.length > 0 && sortRows.every(function (row) { return row.menuAction !== "sort" }), true)
    // A parent with no leaves is its own row.
    var bare = SheetQuery.menuCandidates([{ id: "compress", action: "compress", label: "Compress", submenu: [] }], function () { return "" })
    check("a flyout with no leaves keeps its own row", SheetQuery.rank(bare, "comp").map(function (row) { return row.label }).join("|"), "Compress")
    var openWithLeaf = compressRows.filter(function (row) { return row.label === "Alpha" })[0]
    check("an Open with leaf keeps its own label", openWithLeaf !== undefined, true)
    check("and its muted flyout suffix", openWithLeaf ? openWithLeaf.where : "none", "Open with")
    // Availability is the menu's own answer for the pane's real cursor: on a regular file Permissions runs, on nothing it is refused.
    function permissionsRow(context) {
        var rows = SheetQuery.menuCandidates(Menu.listingEntries(context), function (a) { return Keymap.hintFor(a) })
        return rows.filter(function (row) { return row.label === "Permissions" })[0]
    }
    var onFile = permissionsRow(cursorOnFile)
    check("Permissions is available with the cursor on a regular file", onFile.disabled, false)
    check("and Enter runs it through the menu", SheetQuery.dispatch(onFile).kind + ":" + SheetQuery.dispatch(onFile).menuAction, "menu:permissions")
    var onNothing = permissionsRow({ hasRow: true, hiddenActions: [], selectionCount: 0, rowMode: 0, selectionModes: [] })
    check("Permissions is drawn disabled with nothing to act on", onNothing.disabled, true)
    check("and Enter is refused", SheetQuery.dispatch(onNothing).kind, "disabled")
    // The values the context above stands for are live in Pane only while the menu or the sheet reads them.
    var paneSource = Source.source("ui/Pane.qml")
    check("the pane keeps the menu's values live while the keymap sheet is open",
          /readonly property bool menuValuesLive:[^\n]*sheetReadsMenu/.test(paneSource), true)
    check("and the sheet's open state is what makes them so",
          /readonly property bool sheetReadsMenu:[^\n]*keymapSheet\.opened/.test(paneSource), true)
    // The menu's verdict rides on the key row, and several rows open the bulk card instead of refusing.
    var multi = SheetQuery.unique(SheetQuery.actionCandidates(Keymap.sheetFor("default", "gui", false)).concat(
        SheetQuery.menuCandidates(Menu.listingEntries({ hasRow: true, hiddenActions: [], selectionCount: 2, rowMode: 0o100644, selectionModes: [0o100644, 0o100644] }),
            function (a) { return Keymap.hintFor(a) }))).filter(function (row) { return row.action === "rename" || row.menuAction === "rename" })
    check("a multi-row Rename is still one row", multi.length, 1)
    check("it keeps the key row's wording", multi[0].label, "Rename")
    check("it follows the menu: enabled over several rows", multi[0].disabled, false)
    check("and Enter runs it as its key row", SheetQuery.dispatch(multi[0]).kind, "action")
}
