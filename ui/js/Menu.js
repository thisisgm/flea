.pragma library

.import "Archive.js" as Archive
.import "Sort.js" as Sort

// Submenus carry an entry array, including an empty array while a provider is unavailable.
function hasSubmenu(entry) {
    return entry !== undefined && entry !== null && entry.submenu !== undefined
}

// Flip at the far edge first, then shift only when neither side fits.
function clamp(point, size, bounds) {
    var start = point + size <= bounds ? point : point - size
    return Math.max(0, Math.min(Math.max(0, bounds - size), start))
}

// The flyout's tail row, which is the only way into the dialog; ui/PaneMenuActions.qml reads it.
var OPEN_WITH_OTHER = "__another__"

// SettingsMenus and SettingsPlaces share one order; F=file/folder, B=background, T=Trash rail.
var INVENTORY = [
    ["open", "Open", "folder-open", "FT", "open"],
    ["openwith", "Open with", "app-window", "F", "open", "openWith"],
    ["newFolder", "New Folder", "folder-plus", "B", "open"],
    ["newFile", "New File", "file-plus", "B", "open"],
    ["cut", "Cut", "scissors", "F", "basic"],
    ["copy", "Copy", "copy", "F", "basic"],
    ["paste", "Paste", "clipboard", "FB", "basic"],
    ["duplicate", "Duplicate", "file-plus", "F", "basic"],
    ["rename", "Rename", "rename", "F", "basic"],
    ["selectAll", "Select all", "check", "B", "basic"],
    ["compress", "Compress", "archive", "F", "archive"],
    ["extract", "Extract", "archive-out", "F", "archive"],
    ["convert", "Convert", "sliders", "F", "archive"],
    ["taildrop", "Send with Taildrop", "tailscale", "F", "share"],
    ["dropbox", "Move to Dropbox", "dropbox", "F", "share"],
    ["sharelink", "Copy Share Link", "network", "F", "share"],
    ["trash", "Move to Trash", "trash", "F", "trash"],
    ["delete", "Delete permanently", "trash", "F", "trash", "deletePermanently"],
    ["openTerminal", "Open in terminal", "terminal", "FB", "inspect"],
    ["moveto", "Move to", "folder-plus", "F", "inspect", "moveTo"],
    ["copyto", "Copy to", "copy", "F", "inspect", "copyTo"],
    ["properties", "Properties", "info", "F", "inspect"],
    ["permissions", "Permissions", "lock", "F", "inspect"],
    ["copypath", "Copy path", "file-text", "F", "inspect"],
    ["addFavourite", "Add to Favorites", "star", "FB", "inspect"],
    ["sort", "Sort by", "sort", "B", "view"],
    ["toggleHidden", "Show hidden files", "eye", "FB", "view"],
    ["settings", "Settings", "sliders", "B", "settings"],
    ["restoreAll", "Restore all", "undo", "T", "restore"],
    ["emptyTrash", "Empty Trash", "trash", "T", "empty"]
]

function listingEntries(p) { return buildEntries(p.hasRow ? "F" : "B", p) }
function backgroundEntries(p) { return buildEntries("B", p) }
function trashEntries(total, busy) { return buildEntries("T", { trashTotal: total, busy: busy }) }

// Preserve action and peer identity when refreshed capabilities change the inventory beneath the keyboard cursor.
function refreshedCursor(previous, next, cursor, submenuRow, submenuCursor) {
    var action = previous[cursor] ? previous[cursor].action : ""
    var selected = action ? next.findIndex(function(entry) { return entry.action === action }) : -1
    if (selected < 0) {
        selected = Math.min(Math.max(0, cursor), next.length - 1)
        while (selected < next.length && selected >= 0 && (next[selected].separator || next[selected].disabled)) selected++
        if (selected === next.length) {
            selected--
            while (selected >= 0 && (next[selected].separator || next[selected].disabled)) selected--
        }
    }
    var oldSubmenu = previous[submenuRow]
    var subRow = oldSubmenu ? next.findIndex(function(entry) { return entry.action === oldSubmenu.action }) : -1
    var sub = subRow >= 0 ? next[subRow] : null
    var oldTarget = oldSubmenu && oldSubmenu.submenu ? oldSubmenu.submenu[submenuCursor] : null
    var target = sub && !sub.disabled && oldTarget ? (sub.submenu || []).findIndex(function(entry) { return entry.id === oldTarget.id && !entry.disabled }) : -1
    return { cursor: selected, submenuRow: target >= 0 ? subRow : -1, submenuCursor: Math.max(0, target) }
}

function buildEntries(kind, p) {
    var out = [], group = ""
    for (var i = 0; i < INVENTORY.length; i++) {
        var spec = INVENTORY[i]
        if (spec[3].indexOf(kind) < 0 || isHidden(p.hiddenActions, spec[0])) continue
        var entry = { id: spec[0], action: spec[5] || spec[0], label: spec[1], glyph: spec[2] }
        if (!availableEntry(entry, p, kind)) continue
        if (out.length && group !== spec[4]) out.push({ separator: true })
        group = spec[4]
        out.push(entry)
    }
    return out
}

function availableEntry(e, p, kind) {
    var count = p.selectionCount === undefined ? 1 : p.selectionCount
    if (e.action === "addFavourite" && kind === "F")
        e.disabled = count !== 1 || ((Number(p.rowMode) || 0) & 0o170000) !== 0o040000
    if (e.action === "paste") e.disabled = p.clipboardAvailable !== true
    if (["duplicate", "rename", "properties"].indexOf(e.action) >= 0)
        e.disabled = count !== 1
    if (e.action === "permissions") {
        var permission = permissionsEntry(p.rowMode, count)
        e.disabled = permission.disabled
        e.hint = permission.hint
    }
    if (e.action === "compress") {
        if (!(p.archiveFormats || []).length) return false
        e.submenu = Archive.formatEntries(p.archiveFormats)
    }
    if (e.action === "extract" && !(p.rowIsArchive && p.canExtract === true && count === 1)) return false
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
        e.disabled = p.providersRefreshing === true || !e.submenu.length
        // The row says it cannot answer by being red, not by carrying the reason: a sentence here
        // widened the menu past its own frame while the providers were still being read.
        if (e.disabled && p.providersRefreshing !== true) e.errored = true
    }
    if (e.action === "dropbox" || e.action === "sharelink") {
        if (!p.dropboxInstalled || (e.action === "dropbox" ? p.rowInDropbox : !p.rowInDropbox)) return false
        e.disabled = p.providersRefreshing === true || !p.dropboxPath
        if (e.action === "dropbox") { e.mark = "dropbox"; delete e.glyph }
        if (e.disabled && p.providersRefreshing !== true) e.errored = true
    }
    if (e.action === "sort") e.submenu = sortEntries()
    if (e.action === "toggleHidden") {
        var hidden = hiddenRow(p.showHidden)
        e.label = hidden.label
        e.glyph = hidden.glyph
    }
    if (e.action === "restoreAll" || e.action === "emptyTrash") e.disabled = !(p.trashTotal > 0) || p.busy === true
    if (["trash", "deletePermanently", "emptyTrash"].indexOf(e.action) >= 0) e.danger = true
    return true
}

// The mode describes the selected object itself, so a symlink never grants access to its unseen target.
function permissionsEntry(mode, count) {
    var kind = (Number(mode) || 0) & 0o170000
    var single = count === 1
    var allowed = single && (kind === 0o100000 || kind === 0o040000)
    return { label: "Permissions", action: "permissions", glyph: "lock", disabled: !allowed,
             hint: !single ? "Unavailable" : kind === 0o120000 ? "Symlink target not changed" : allowed ? "" : "Unavailable",
             hintWrap: true }
}


// The Sort by flyout, built from ui/js/Sort.js's own ORDERS so it can only ever offer an order the
// backend really produces; a fourth key would earn a refusal instead of a listing.
var SORT_LABELS = { name: "Name", size: "Size", mtime: "Date Modified", kind: "Kind" }

function sortEntries() {
    var out = []
    for (var i = 0; i < Sort.ORDERS.length; i++)
        out.push({ id: Sort.ORDERS[i], label: SORT_LABELS[Sort.ORDERS[i]] })
    return out
}

// The one mark every row of an open flyout draws, keyed on the row that opened it: a Taildrop peer
// is a machine, a sort order is the row above it, and an archive format is a file about to exist.
function submenuGlyph(action) {
    if (action === "taildrop")
        return "server"
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
    var columns = [["mode", "Mode"], ["size", "Size"], ["date", "Date Modified"], ["kind", "Kind"]]
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
