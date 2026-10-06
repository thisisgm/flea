.pragma library

.import "Archive.js" as Archive
.import "ExtThumbs.js" as ExtThumbs
.import "Format.js" as Format
.import "Sort.js" as Sort

// Submenus carry an entry array, including an empty array while a provider is unavailable.
function hasSubmenu(entry) {
    return entry !== undefined && entry !== null && entry.submenu !== undefined
}

// A separator or a disabled row is never the cursor, so a step skips over one and an end keeps it where it is.
function stepRow(rows, from, delta) {
    for (var i = from + delta; i >= 0 && i < rows.length; i += delta)
        if (rows[i].separator !== true && rows[i].disabled !== true) return i
    return from
}

// Flip at the far edge first, then shift only when neither side fits.
function clamp(point, size, bounds) {
    var start = point + size <= bounds ? point : point - size
    return Math.max(0, Math.min(Math.max(0, bounds - size), start))
}

// The flyout's tail row, which is the only way into the dialog; ui/PaneMenuActions.qml reads it.
var OPEN_WITH_OTHER = "__another__"

// One order for every menu: F file, B background, T Trash, P Places, R rail rows (never hideable, never in Settings > Menus).
var INVENTORY = [
    ["open", "Open", "folder-open", "FTPR", "open"],
    // Show original reveals a symlink's target in its own folder.
    ["showOriginal", "Show original", "symlink", "F", "open", "showOriginal"],
    // MenuAdditions rule 3: a Places or Favorites row opens this menu for its own path, so the rows
    // it carries are the ones that take a path and not the clipboard, archive, send or destroy ones.
    ["openTab", "New tab", "plus", "P", "open"],
    ["openwith", "Open with", "app-window", "F", "open", "openWith"],
    // Mount and Unmount are one toggle on the drive mark, as Show and Hide share the eye; Eject keeps its own.
    ["mountVolume", "Mount", "drive", "R", "open"],
    ["mountPhone", "Mount", "drive", "R", "open"],
    ["unmountVolume", "Unmount", "drive", "R", "rrelease"],
    ["unmountPhone", "Unmount", "drive", "R", "rrelease"],
    ["unmount", "Unmount", "drive", "R", "rrelease"],
    ["eject", "Eject", "eject", "R", "rrelease"],
    ["newFolder", "New Folder", "folder-plus", "B", "open"],
    ["newFile", "New File", "file-plus", "B", "open"],
    ["cut", "Cut", "scissors", "F", "basic"],
    ["copy", "Copy", "copy", "F", "basic"],
    ["paste", "Paste", "clipboard", "FB", "basic"],
    // MenuAdditions040: Paste as links, undoable, through the collision
    // card; hidden, and holding only the three link rows.
    ["pasteAs", "Paste as", "symlink", "FB", "basic", "pasteAs"],
    ["duplicate", "Duplicate", "file-plus", "F", "basic"],
    ["rename", "Rename", "rename", "FR", "basic"],
    // A saved place's own rows beside Rename; Edit address and Remove from Network are rail-only.
    ["editPlace", "Edit address", "sliders", "R", "basic"],
    ["remove", "Remove from Network", "minus", "R", "rremove"],
    ["selectAll", "Select all", "check", "B", "basic"],
    // MenuAdditions040: Invert selection flips the marks over the rows the
    // listing draws; hidden, present only while something is selected.
    ["invertSelection", "Invert selection", "contrast", "B", "basic", "invertSelection"],
    ["compress", "Compress", "archive", "F", "archive"],
    ["extract", "Extract", "archive-out", "F", "archive"],
    ["convert", "Convert", "sliders", "F", "archive"],
    // Flea's own spiral drawn static: FleaMark paints in over two seconds, which a menu row never waits for.
    ["shelf", "Add to shelf", "shelf", "F", "share", "addToShelf"],
    ["taildrop", "Send with Taildrop", "tailscale", "F", "share"],
    // MenuAdditions rule 1: between Taildrop and Dropbox, present only while a localsend binary is
    // on PATH, and carrying its own reproduced mark rather than a cut glyph.
    ["localsend", "Send with LocalSend", "localsend", "F", "share"],
    ["dropbox", "Move to Dropbox", "dropbox", "F", "share"],
    ["sharelink", "Copy Share Link", "network", "F", "share"],
    ["trash", "Move to Trash", "trash", "F", "trash"],
    ["delete", "Delete permanently", "trash", "F", "trash", "deletePermanently"],
    ["openTerminal", "Open in terminal", "terminal", "FBP", "inspect"],
    ["moveto", "Move to", "folder-plus", "F", "inspect", "moveTo"],
    ["copyto", "Copy to", "copy", "F", "inspect", "copyTo"],
    ["properties", "Properties", "info", "F", "inspect"],
    ["permissions", "Permissions", "lock", "F", "inspect"],
    // Both boards place Make executable in inspect, without a key, only on scripts missing every execute bit.
    ["makeExecutable", "Make executable", "play", "F", "inspect", "makeExecutable"],
    // MenuAdditions040: Copy as replaces the file menu's hidden Copy path row; a place keeps the flat Copy path behind the same switch.
    ["copyAs", "Copy as", "file-text", "FP", "inspect", "copyAs"],
    // MenuAdditions rule 2: after Copy as, one row per executable in ~/.config/flea/scripts, and
    // absent rather than greyed when that directory is missing or holds none.
    ["runScript", "Run script", "terminal", "F", "inspect"],
    ["addFavourite", "Add to Favorites", "star", "FBP", "inspect"],
    ["removeFavourite", "Remove from Favorites", "minus", "PR", "inspect"],
    ["sort", "Sort by", "sort", "B", "view"],
    ["toggleHidden", "Show hidden files", "eye", "FB", "view"],
    ["extThumbs", "Show thumbnails", "image", "B", "view"],
    ["settings", "Settings", "sliders", "B", "settings"],
    ["updateFlea", "Update Flea", "download", "B", "settings"],
    ["restoreAll", "Restore all", "undo", "T", "restore"],
    ["emptyTrash", "Empty Trash", "trash", "T", "empty"]
]

function listingEntries(p) { return buildEntries(p.hasRow ? "F" : "B", p) }
// The Places rail's own, behind its Extras toggle: off, the rail keeps the menu it has today.
function placeEntries(p) { return isHidden(p.hiddenActions, "placeMenu") ? [] : buildEntries("P", p) }
function backgroundEntries(p) { return buildEntries("B", p) }
function trashEntries(total, busy) { return buildEntries("T", { trashTotal: total, busy: busy }) }

function buildEntries(kind, p) {
    var out = [], group = ""
    for (var i = 0; i < INVENTORY.length; i++) {
        var spec = INVENTORY[i]
        // Rail rows are never user-hideable: no switch governs them, so the stored set is not read here.
        if (spec[3].indexOf(kind) < 0) continue
        // The background menu shows Open in terminal without reading the hidden set.
        if (kind !== "R" && isHidden(p.hiddenActions, spec[0])) {
            if (!(kind === "B" && spec[0] === "openTerminal")) continue
        }
        var entry = { id: spec[0], action: spec[5] || spec[0], label: spec[1], glyph: spec[2] }
        if (!availableEntry(entry, p, kind)) continue
        if (out.length && group !== spec[4]) out.push({ separator: true })
        group = spec[4]
        out.push(entry)
    }
    return out
}

// The rail menu: the rows above with kind R, built with the listing menu's separator rule.
function railEntries(entry) { return buildEntries("R", { entry: entry }) }

function availableEntry(e, p, kind) {
    // Rail availability reads the rail entry alone; nothing here is disabled, red or hideable.
    if (kind === "R") return availableRail(e, p.entry)
    var count = p.selectionCount === undefined ? 1 : p.selectionCount
    // Rule 3: the last row adds the favourite or removes it, and the duplicate case is absent rather
    // than grey, which is what keeps issue 138's second row impossible from the rail as well.
    if (kind === "P" && (e.id === "addFavourite" || e.id === "removeFavourite"))
        return (e.id === "removeFavourite") === (p.placeFavourite === true)
    if (e.action === "addFavourite" && kind === "F")
        e.disabled = count !== 1 || ((Number(p.rowMode) || 0) & 0o170000) !== 0o040000
    if (e.action === "paste") e.disabled = p.clipboardAvailable !== true && p.clipboardWatchFailed !== true
    // MenuAdditions040: Show original is visible but only on a symlink.
    if (e.action === "showOriginal" && p.rowIsSymlink !== true) return false
    // Paste as keeps its disabled row without a chevron until files are known.
    if (e.action === "pasteAs") {
        // A filesystem that holds no links offers no link rows at all.
        if (p.canLink === false) return false
        e.disabled = p.clipboardAvailable !== true
        if (p.clipboardAvailable === true)
            e.submenu = pasteAsEntries()
    }
    // Every Copy as variant covers the whole selection, one path per line; no board draws a place menu with it, so a place keeps 0.3.7's flat Copy path.
    if (e.action === "copyAs") {
        if (kind === "P") { e.id = e.action = "copypath"; e.label = "Copy path" }
        else e.submenu = copyAsEntries()
    }
    // MenuAdditions040: Invert selection flips over the rows the listing
    // draws, so with nothing selected there is nothing to flip from.
    if (e.action === "invertSelection" && count === 0) return false
    if (["duplicate", "rename", "properties"].indexOf(e.action) >= 0)
        e.disabled = count !== 1
    if (e.action === "permissions") {
        var permission = permissionsEntry(p.rowMode, count, p.selectionModes)
        e.disabled = permission.disabled
        if (permission.errored) e.errored = true
    }
    // Only a cursor-row script without any execute bit qualifies; its two-byte probe runs once at menu open.
    if (e.action === "makeExecutable" && !canMakeExecutable(p.rowMode, count, p.hasShebang, p.cursorIsTarget)) return false
    if (e.action === "runScript") {
        if (!(p.scripts || []).length) return false
        e.submenu = p.scripts.map(function (script) { return { id: script.id, label: script.label } })
    }
    if (e.action === "compress") {
        if (!(p.archiveFormats || []).length) return false
        e.submenu = Archive.formatEntries(p.archiveFormats)
    }
    // Issue 133: a mount with no trash directory never offers the row, rather than offering one that
    // fails; ui/js/Mounts.js trashable is the one reader of what the path says about that.
    if (e.action === "trash" && p.canTrash === false) return false
    // A read-only folder turns its write rows off; the listing's own w flag decides, never the mode.
    if (p.dirWritable === false && ["newFolder", "newFile", "paste", "pasteAs", "duplicate",
            "rename", "trash", "deletePermanently"].indexOf(e.action) >= 0) e.disabled = true
    if (e.action === "extract" && !Archive.extractEntry(e, p, count)) return false
    if (e.action === "convert" && !(p.rowIsImage && p.canConvert && count === 1)) return false
    // OpenWith.html: the desktop's current default is first and carries the muted caption "default"
    // in the hint slot, the registry order follows it, and the tail row sits under its own
    // separator with the app-window glyph. The row self-hides when the registry names nothing.
    if (e.action === "openWith") {
        if (p.openWithLoaded === true && !(p.openWithApps || []).length) return false
        var apps = p.openWithApps || []
        var rows = apps.map(function (app) {
            return { id: app.id, label: app.label, icon: app.icon || "",
                     hint: app.default === true ? "default" : "" }
        })
        if (rows.length) rows.push({ separator: true })
        rows.push({ id: OPEN_WITH_OTHER, label: "Another application\u2026", glyph: "app-window" })
        e.submenu = rows
        e.disabled = count !== 1
    }
    if (e.action === "taildrop") {
        if (!p.taildropInstalled) return false
        e.mark = "tailscale"
        delete e.glyph
        e.submenu = p.taildropPeers || []
        e.disabled = p.taildropRefreshing === true || !e.submenu.length
        // The row says it cannot answer by being red, not by carrying the reason: a sentence here
        // widened the menu past its own frame while the providers were still being read.
        if (e.disabled && p.taildropRefreshing !== true) e.errored = true
    }
    // Absent rather than greyed when nothing is installed, the Menu board's rule for a row whose
    // whole destination is missing; the row is a plain send, so it has no submenu and no reason.
    // Directive 71: the row is Taildrop's twin, so it opens the same flyout and reads the same way.
    if (e.action === "localsend") {
        if (!p.localSendInstalled) return false
        e.mark = "localsend"
        delete e.glyph
        e.submenu = p.localSendPeers || []
        e.disabled = p.localSendChecking === true || !e.submenu.length
        if (e.disabled && p.localSendChecking !== true) e.errored = true
    }
    if (e.action === "dropbox" || e.action === "sharelink") {
        if (!p.dropboxInstalled || (e.action === "dropbox" ? p.rowInDropbox : !p.rowInDropbox)) return false
        e.disabled = p.dropboxRefreshing === true || !p.dropboxPath
        if (e.action === "dropbox") { e.mark = "dropbox"; delete e.glyph }
        if (e.disabled && p.dropboxRefreshing !== true) e.errored = true
    }
    if (e.action === "sort") e.submenu = sortEntries(p.hasFolderSort === true)
    // Present only while a check has found a newer build, whose version rides the hint slot beside the status square.
    if (e.action === "updateFlea") {
        if (!p.updateVersion) return false
        e.hint = p.updateVersion
        e.hintSquare = true
    }
    if (e.action === "toggleHidden") {
        var hidden = hiddenRow(p.showHidden)
        e.label = hidden.label
        e.glyph = hidden.glyph
    }
    // The class switch ships hidden; switched on, it is present only on network,
    // phone or USB storage and reads Show while that class is off, Hide while on.
    if (e.action === "extThumbs") return ExtThumbs.entry(e, p)
    if (e.action === "restoreAll" || e.action === "emptyTrash") e.disabled = !(p.trashTotal > 0) || p.busy === true
    if (["trash", "deletePermanently", "emptyTrash"].indexOf(e.action) >= 0) e.danger = true
    return true
}

// Rail rows read the rail entry alone; Mount and Unmount never meet, and Open is the row's own click.
function availableRail(e, entry) {
    if (!entry) return false
    var device = entry.group === "device", network = entry.group === "network"
    var volume = device && entry.kind === "volume", phone = device && entry.kind === "phone"
    var share = network && entry.kind === "share"
    var mounted = entry.mounted === true
    switch (e.id) {
    case "mountVolume": return volume && !mounted && entry.volumeMenu === true
    case "mountPhone": return phone && !mounted
    case "open": return (volume && mounted && (entry.volumeMenu === true || entry.removable === true || entry.loop === true))
        || (phone && mounted) || (share && mounted)
    case "unmountVolume": return volume && mounted && entry.volumeMenu === true
    case "unmountPhone": return phone && mounted
    case "unmount": return share && mounted
    case "eject": return volume && mounted && (entry.removable === true || entry.loop === true)
    case "rename": return share && entry.editable !== false
    case "editPlace": return share && entry.editable !== false
    case "remove": return share && entry.editable !== false
    case "removeFavourite": return entry.kind === "favourite"
    default: return false
    }
}

// The mode describes the selected object itself, so a symlink never grants access to its unseen target.
// Issue 193, GM's ruling of 2026-09-24: a row that cannot act reads red with no sentence, as a provider does.
// MenuAdditions040: Permissions takes the whole selection, so one bad row
// among modes refuses the row; the dialog inspects each file in turn.
function permissionsEntry(mode, count, modes) {
    if (modes !== undefined && modes !== null && modes.length > 0) {
        for (var i = 0; i < modes.length; i++) {
            var kind = (Number(modes[i]) || 0) & Format.S_IFMT
            if (!(kind === Format.S_IFREG || kind === 0o040000))
                return { label: "Permissions", action: "permissions", glyph: "lock", disabled: true, errored: true }
        }
        return { label: "Permissions", action: "permissions", glyph: "lock", disabled: false, errored: false }
    }
    var kind = (Number(mode) || 0) & Format.S_IFMT
    var allowed = count >= 1 && (kind === Format.S_IFREG || kind === 0o040000)
    return { label: "Permissions", action: "permissions", glyph: "lock", disabled: !allowed, errored: !allowed }
}

// Only a single cursor-row regular file with a shebang and no execute bit.
function canMakeExecutable(mode, count, hasShebang, cursorIsTarget) {
    if (count !== 1 || hasShebang !== true || cursorIsTarget !== true) return false
    var bits = Number(mode) || 0
    if ((bits & Format.S_IFMT) !== Format.S_IFREG) return false
    return (bits & Format.ANY_EXECUTE_BIT) === 0
}

// The Copy as flyout: six leaves in board order, letters on keyHint.
function copyAsEntries() {
    return [
        { id: "copyPath", label: "Path", glyph: "file-text", keyHint: "p" },
        { id: "copyName", label: "Name", glyph: "type", keyHint: "n" },
        { id: "copyStem", label: "Name without extension", glyph: "type", keyHint: "e" },
        { id: "copydirpath", label: "Folder path", glyph: "folder", keyHint: "f" },
        { id: "copyUri", label: "File URI", glyph: "globe", keyHint: "u" },
        { id: "copyQuoted", label: "Shell-quoted", glyph: "terminal", keyHint: "s" }
    ]
}

// MenuAdditions040: the Paste as flyout. Link is relative from the
// destination, Absolute link stores the full path; l and h open and close a
// flyout, so the letters are L, a and H.
function pasteAsEntries() {
    return [
        { id: "pasteLink", label: "Link", glyph: "symlink", keyHint: "L" },
        { id: "pasteAbsoluteLink", label: "Absolute link", glyph: "symlink", keyHint: "a" },
        { id: "pasteHardLink", label: "Hard link", glyph: "copy", keyHint: "H" }
    ]
}

// A hidden row still opens its flyout, built from the action.
function flyoutEntries(action) {
    if (action === "copyAs")
        return copyAsEntries()
    if (action === "pasteAs")
        return pasteAsEntries()
    return []
}
// The sentence an empty Paste as refuses with, the same one ui/Pane.qml shows.
var EMPTY_CLIPBOARD = "There is nothing to paste; y copies and x cuts."
// Sample input: submenuFor("pasteAs", [], false) answers refuse.
function submenuFor(action, entries, clipboardAvailable) {
    if (flyoutEntries(action).length === 0)
        return { kind: "none" }
    for (var i = 0; i < entries.length; i++) {
        if (entries[i].action === action) {
            if (entries[i].disabled === true || !hasSubmenu(entries[i]))
                return { kind: "refuse" }
            return { kind: "row", index: i }
        }
    }
    if (action === "pasteAs" && clipboardAvailable !== true)
        return { kind: "refuse" }
    return { kind: "lone" }
}
// Sample input: loneChoice("copyAs", "copyPath", false, false, true, "a", "a") fires "copyAs:copyPath".
function loneChoice(action, id, forRail, forHeader, hasRow, openedIdentity, selectionIdentity) {
    var leaves = flyoutEntries(action)
    var known = false
    for (var i = 0; i < leaves.length; i++)
        if (leaves[i].separator !== true && leaves[i].id === id) known = true
    var moved = !forRail && !forHeader && hasRow && openedIdentity !== selectionIdentity
    if (moved) return { kind: "moved" }
    if (!known) return { kind: "unknown" }
    return { kind: "fire", fired: action + ":" + id }
}


// The Sort by flyout, built from ui/js/Sort.js's own ORDERS so it can only ever offer an order the
// backend really produces; a fourth key would earn a refusal instead of a listing.
var SORT_LABELS = { name: "Name", size: "Size", mtime: "Modified", kind: "Kind" }

function sortEntries(hasOwn) {
    var out = []
    for (var i = 0; i < Sort.ORDERS.length; i++)
        out.push({ id: Sort.ORDERS[i], label: SORT_LABELS[Sort.ORDERS[i]] })
    // The id is the spelling ui/js/Sort.js's forget branch reads, so the two must agree.
    if (hasOwn === true) { out.push({ separator: true }); out.push({ id: "__default__", label: "Use the default sort" }) }
    return out
}

// The one mark every row of an open flyout draws, keyed on the row that opened it: a Taildrop peer
// is a machine, a sort order is the row above it, and an archive format is a file about to exist.
function submenuGlyph(action) {
    if (action === "taildrop")
        return "server"
    if (action === "runScript")
        return "terminal"
    if (action === "sort")
        return "sort"
    return "archive"
}

// The visibility consumer SettingsMenus.html specifies, and the only reader of the stored hidden
// set: a switched-off action's row leaves, and a rule that has lost every row it divided leaves
// with it, so a menu never grows a doubled, leading or trailing separator. Order never changes,
// because the board offers visibility and deliberately not ordering.
function applyHidden(entries, hiddenActions) {
    var kept = []
    for (var i = 0; i < entries.length; i++) {
        var entry = entries[i]
        if (entry.separator === true) {
            if (kept.length > 0 && kept[kept.length - 1].separator !== true)
                kept.push(entry)
            continue
        }
        if (!isHidden(hiddenActions, entry.id || entry.action))
            kept.push(entry)
    }
    while (kept.length > 0 && kept[kept.length - 1].separator === true)
        kept.pop()
    return kept
}

// Open and the hidden toggle are never in the stored set: ui/js/Settings.js draws them locked, and
// this is the second half of that lock, so a hand-edited state file cannot empty the menu either.
function isHidden(hiddenActions, action) {
    if (action === "open" || action === "toggleHidden")
        return false
    for (var i = 0; hiddenActions && i < hiddenActions.length; i++) {
        if (hiddenActions[i] === action)
            return true
    }
    return false
}

// The one state toggle either menu draws. The label flips with the state, the house pattern
// (ui/MenuRow.qml draws no checkmark), and the verb is the key's own.
function hiddenRow(showHidden) {
    return {
        label: showHidden ? "Hide hidden files" : "Show hidden files",
        action: "toggleHidden",
        glyph: showHidden ? "eye-off" : "eye"
    }
}

// ui/Header.qml's own rows, on a right click over the column titles. Name is not among them: it is
// the one column a file manager cannot do without (see ui/js/Columns.js), so it is never hidden and
// never offered. A hidden column reads "Show", a drawn one "Hide", the flip the state rows use.
function headerEntries(hiddenCols, showHidden) {
    var out = []
    var hidden = {}
    for (var h = 0; h < hiddenCols.length; h++)
        hidden[hiddenCols[h]] = true
    var columns = [["mode", "Mode"], ["size", "Size"], ["date", "Modified"], ["kind", "Kind"]]
    var glyphs = { mode: "lock", size: "drive", date: "download", kind: "type" }
    for (var i = 0; i < columns.length; i++) {
        var key = columns[i][0]
        var shown = !hidden[key]
        out.push({
            label: shown ? "Hide " + columns[i][1] : "Show " + columns[i][1],
            action: "col:" + key,
            glyph: glyphs[key]
        })
    }
    out.push({ separator: true })
    out.push(hiddenRow(showHidden))
    return out
}

// The keyboard's own entrance to the row menu, under the cursor row the way the rail's opens under
// its own; setCursor first, because a wheel scroll in the grid can leave the cursor off screen.
// paddingX is ui/Theme.qml's rowPaddingX, passed in because a singleton has no name in a library.
function openAtCursor(root, menu, paddingX) {
    root.setCursor(root.cursorIndex)
    var row = root.visibleItemFor(root.cursorIndex)
    if (row)
        menu.openAt(row.mapToItem(null, paddingX, row.height))
    return row !== null
}
