.pragma library

.import "Archive.js" as Archive
.import "Sort.js" as Sort

// The one definition of a submenu row, shared by ui/MenuRow.qml and ui/ContextMenu.qml. The two
// carried their own copies and drifted: the row drew no disclosure at all for want of this.

// A submenu row carries its flyout's own entries in this field, so the test is that the field is
// present, never that it is true.
function hasSubmenu(entry) {
    return entry !== undefined && entry !== null && entry.submenu !== undefined
}

// Where one edge of a menu frame sits when it opens at this point: far enough back that the whole
// frame stays inside its bounds, and never off the near edge. A frame larger than its bounds pins
// to the near edge and its far end is cut, which no menu built on this box reaches.
function clamp(point, size, bounds) {
    return Math.max(0, Math.min(bounds - size, point))
}

// The listing's rows, built from the pane's state in one object so the construction can live here
// and carry its own suite, tests/js/menu.js. Copy path sits beside Open because both answer
// "where is this and what runs on it".
function listingEntries(p) {
    // Empty space is its own menu and not a shortened row menu: Menus.html draws the background
    // column with its own rows in its own order, so the two are built apart and filtered alike.
    if (!p.hasRow)
        return applyHidden(backgroundEntries(p), p.hiddenActions)
    var out = []
    out.push({ label: "Open", action: "open", glyph: "folder-open" })
    out.push({ label: "Copy path", action: "copypath", glyph: "file-text" })
    out.push({ separator: true })
    // SettingsMenus.html's six basic rows, in its own order. Cut, Copy and Paste were keyboard
    // only until the Menus section grew a switch for each of them, and a switch over a row no
    // menu draws is a mock control. Paste answers with a sentence on an empty clipboard.
    out.push({ label: "Cut", action: "cut", glyph: "scissors" })
    out.push({ label: "Copy", action: "copy", glyph: "copy" })
    out.push({ label: "Paste", action: "paste", glyph: "clipboard" })
    out.push({ label: "Duplicate", action: "duplicate", glyph: "file-plus" })
    out.push({ label: "Rename", action: "rename", glyph: "rename" })
    var ops = []
    // The submenu is exactly the table the backend probed, so a box with no tool offers nothing.
    if (p.archiveFormats.length > 0)
        ops.push({ label: "Compress", action: "compress", glyph: "archive",
                   submenu: Archive.formatEntries(p.archiveFormats) })
    if (p.rowIsArchive)
        ops.push({ label: "Extract", action: "extract", glyph: "archive-out" })
    if (p.canConvert && p.rowIsImage)
        ops.push({ label: "Convert", action: "convert", glyph: "sliders" })
    if (ops.length > 0) {
        out.push({ separator: true })
        for (var i = 0; i < ops.length; i++) out.push(ops[i])
    }
    var share = []
    if (p.taildropPeers.length > 0)
        share.push({ label: "Send with Taildrop", action: "taildrop", mark: "tailscale",
                     submenu: p.taildropPeers })
    // Moving a file into the folder it already lives in is not an action, so the row hides there.
    if (p.dropboxPath.length > 0 && !p.rowInDropbox)
        share.push({ label: "Move to Dropbox", action: "dropbox", mark: "dropbox" })
    // A share link is inherently per file, so it appears only for a row already in Dropbox.
    if (p.rowInDropbox)
        share.push({ label: "Copy share link", action: "sharelink", glyph: "network" })
    if (share.length > 0) {
        out.push({ separator: true })
        for (var s = 0; s < share.length; s++) out.push(share[s])
    }
    out.push({ separator: true })
    // No confirm anywhere behind this row: the undo journal is the safety, see the operations design.
    out.push({ label: "Move to Trash", action: "trash", glyph: "trash", danger: true })
    out.push({ separator: true })
    // The tail is the rows that need no row under the cursor. Open in terminal opens the directory
    // being shown rather than the row, which is why it sits here and not above.
    out.push({ label: "Open in terminal", action: "openTerminal", glyph: "terminal" })
    out.push({ label: "New folder", action: "newFolder", glyph: "folder-plus" })
    out.push(hiddenRow(p.showHidden))
    return applyHidden(out, p.hiddenActions)
}

// Menus.html's background column, drawn on a right click that landed on no row: the directory's
// own actions, in the board's order and with its rules. Its New File row is not built, because
// this release's backend has mkdir and no create-empty-file of any kind, and a row that cannot
// work is not a row. The same hiddenActions set filters it, so a switch is never per menu.
function backgroundEntries(p) {
    var out = []
    out.push({ label: "New folder", action: "newFolder", glyph: "folder-plus" })
    out.push({ separator: true })
    out.push({ label: "Paste", action: "paste", glyph: "clipboard" })
    out.push({ label: "Select all", action: "selectAll", glyph: "check" })
    out.push({ separator: true })
    out.push({ label: "Sort by", action: "sort", glyph: "sort", submenu: sortEntries() })
    out.push({ label: "Open in terminal", action: "openTerminal", glyph: "terminal" })
    out.push(hiddenRow(p.showHidden))
    out.push({ separator: true })
    out.push({ label: "Settings", action: "settings", glyph: "sliders" })
    return out
}

// The Sort by flyout, built from ui/js/Sort.js's own ORDERS so it can only ever offer an order the
// backend really produces; a fourth key would earn a refusal instead of a listing.
var SORT_LABELS = { name: "Name", size: "Size", mtime: "Date Modified" }

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
        if (!isHidden(hiddenActions, entry.action))
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
