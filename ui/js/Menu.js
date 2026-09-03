.pragma library

.import "Archive.js" as Archive

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
    var out = []
    if (p.hasRow) {
        out.push({ label: "Open", action: "open", glyph: "folder-open" })
        out.push({ label: "Copy path", action: "copypath", glyph: "file-text" })
        out.push({ separator: true })
        out.push({ label: "Rename", action: "rename", glyph: "rename" })
        out.push({ label: "Duplicate", action: "duplicate", glyph: "file-plus" })
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
    }
    // The last group is the rows that need no row under the cursor, which is also the whole menu
    // on a listing's empty space.
    out.push({ label: "New folder", action: "newFolder", glyph: "folder-plus" })
    out.push(hiddenRow(p.showHidden))
    return out
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
