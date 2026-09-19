.pragma library

.import "Format.js" as Format
.import "Icons.js" as Icons
.import "Kinds.js" as Kinds

// The preview column's twelve states and the facts each one states, taken row by row from the
// canvas's Preview column board. One anatomy for all twelve: a frame, an optional transport, the
// name, and this caption-type table under it.

// The state names live with the classification that produces them; these aliases keep every
// Facts.STATE call site working with one definition behind them.
var IMAGE = Kinds.IMAGE
var VIDEO = Kinds.VIDEO
var AUDIO = Kinds.AUDIO
var PDF = Kinds.PDF
var TEXT = Kinds.TEXT
var CODE = Kinds.CODE
var ARCHIVE = Kinds.ARCHIVE
var SYMLINK = Kinds.SYMLINK
var MULTI = Kinds.MULTI
var LOADING = Kinds.LOADING
var ERROR = Kinds.ERROR
var UNSUPPORTED = Kinds.UNSUPPORTED
var UNKNOWN_KIND = Kinds.UNKNOWN_KIND

// The three that outrank what the row is: a multi-selection has no single row to describe, an error
// has nothing to read, and a preview still arriving has nothing yet.
function state(row, selectionCount, loading, errorText, kindName) {
    if (selectionCount > 1) {
        return MULTI
    }
    if (!row) {
        return UNSUPPORTED
    }
    if (errorText && errorText.length > 0) {
        return ERROR
    }
    if (loading) {
        return LOADING
    }
    if (Format.isSymlink(row.p)) {
        return SYMLINK
    }
    // The backend's icon ladder falls a name it cannot identify all the way to text-x-generic, but
    // its Kind says the truth: UNKNOWN_KIND, which is "Data". Such a row is not text, it is a type
    // nothing recognised, and that is the canvas's Unsupported tile (its own example is core.dump).
    if (kindName === UNKNOWN_KIND) {
        return UNSUPPORTED
    }
    // A PDF shares the x-office-document icon with every other office type, so it is told apart by
    // its own extension, exactly the way an archive is: the client classifies, the wire does not.
    if (Kinds.isPdf(row.n)) {
        return PDF
    }
    // A markdown file shares the office icon with a PDF and a .doc, and the canvas's Text tile is a
    // .md, so the suffix is the only thing that can route it to the text frame.
    if (Kinds.isMarkdown(row.n)) {
        return TEXT
    }
    var byIcon = Kinds.kindState(row.i)
    // Only a row the icon already called text can be promoted to code; an image named .css is not code.
    if (byIcon === TEXT && Kinds.isCode(row.n)) {
        return CODE
    }
    return byIcon
}

// The backend's icon for a source file is inconsistent (text-x-script for .js and .sh, plain
// text-x-generic for .rs, .py, .c and .go), so code is told apart by its own extension, the same way
// an archive and a PDF are. Anything not listed reads as text, which draws no gutter and is the safe
// default: a wrong gutter is worse than a missing one.

function pair(label, value) {
    return { label: label, value: value }
}

// HANDOFF rule 19: a label with no value is not drawn. Every surface here already refuses to invent
// a value, so the row goes rather than printing a label with nothing after it.
function filled(rows) {
    var out = []
    for (var i = 0; i < rows.length; i++) {
        if (rows[i].value !== undefined && rows[i].value !== null && String(rows[i].value).length > 0) {
            out.push(rows[i])
        }
    }
    return out
}

// A count the backend had to stop early is a floor, marked the way a partial directory size is.
function lineCount(meta) {
    // A file the backend could not open has no count at all, and its zero would print as "Lines: 0".
    if (!meta || meta.linesFailed || meta.lines === undefined || meta.lines === null) {
        return ""
    }
    return (meta.partial ? "> " : "") + meta.lines
}

function pixels(meta) {
    if (!meta || !meta.w || !meta.h) {
        return ""
    }
    return meta.w + " × " + meta.h
}

// Duration and rate come off the same meta answer as pixels do, so no caller assembles them by hand.
function mediaExtra(meta) {
    if (!meta) {
        return {}
    }
    return {
        duration: meta.durationMs > 0 ? Format.duration(meta.durationMs) : "",
        rate: Format.sampleRate(meta.sampleRate)
    }
}

// Entries and the unpacked total come off the same meta answer, read from the archive's own index.
// Both are exact whenever they are non-zero: a read that failed or ran out of time sends zero and
// sets afailed, so a partial count never reaches this table.
function archiveExtra(meta) {
    if (!meta || !meta.entries) {
        return {}
    }
    return {
        entries: String(meta.entries),
        unpacked: meta.unpacked > 0 ? Format.size(meta.unpacked) : ""
    }
}

// "214 entries · 3.4 GB unpacked", the Quick Look's one line under an archive's name; the column
// states the same two facts as table rows, and an index that failed or is still arriving states nothing.
function archiveLine(meta) {
    var a = archiveExtra(meta)
    if (!a.entries) {
        // An index that read cleanly and holds nothing is still a fact; a failed or absent one is not.
        return meta && meta.archiveFailed !== true && meta.entries === 0 ? "0 entries" : ""
    }
    var noun = a.entries === "1" ? " entry" : " entries"
    return a.unpacked ? a.entries + noun + " \u00b7 " + a.unpacked + " unpacked" : a.entries + noun
}

// The rows the Archive tile lists above its "+ N more" line, which is the only place a name from
// inside an archive is ever drawn. shown is how many the frame has room for, so the list is bounded
// twice: by what the wire carried and by what the frame can draw.
function archiveEntries(meta, shown) {
    if (!meta || !meta.names) {
        return []
    }
    return shown === undefined ? meta.names : meta.names.slice(0, Math.max(0, shown))
}

// What the canvas puts under the listed rows: every entry the frame did not name, whether the wire
// left it out or the frame had no room for it.
function archiveMore(meta, shown) {
    if (!meta || !meta.entries || !meta.names) {
        return 0
    }
    return Math.max(0, meta.entries - archiveEntries(meta, shown).length)
}

// The rows the canvas draws for each state, in its own order, with the labels it uses verbatim.
function facts(st, row, meta, kindName, extra) {
    return filled(stateRows(st, row, meta, kindName, extra))
}

function stateRows(st, row, meta, kindName, extra) {
    // A caller's own value wins; the probe fills only the two fields it did not supply.
    var e = extra || {}
    if (st === VIDEO || st === AUDIO) {
        var m = mediaExtra(meta)
        e = Object.assign({}, e, { duration: e.duration || m.duration, rate: e.rate || m.rate })
    } else if (st === ARCHIVE) {
        var a = archiveExtra(meta)
        e = Object.assign({}, e, { entries: e.entries || a.entries, unpacked: e.unpacked || a.unpacked })
    }
    // The three rows every ordinary settled kind opens with, so a second selection lands each answer on the line the first one used; Preview board rule 1.
    var head = [pair("Kind", kindName), pair("Size", Format.rowSize(row)),
                pair("Modified", Format.date(row.m))]
    switch (st) {
    case IMAGE:
        return head.concat([pair("Pixels", pixels(meta))])
    case VIDEO:
        return head.concat([pair("Duration", e.duration || ""), pair("Pixels", pixels(meta))])
    case AUDIO:
        return head.concat([pair("Duration", e.duration || ""), pair("Rate", e.rate || "")])
    case PDF:
        return head.concat([pair("Pages", e.pages || "")])
    case TEXT:
        return head.concat([pair("Lines", lineCount(meta))])
    // Mode stays: columns view has no permission column, so dropping it here would remove it.
    case CODE:
        return head.concat([pair("Lines", lineCount(meta)), pair("Mode", Format.permissions(row.p))])
    case ARCHIVE:
        return head.concat([pair("Entries", archiveFact(e))])
    case SYMLINK:
        return [pair("Kind", "Symbolic link"), pair("Target", meta ? meta.target : ""),
                pair("Points at", meta && meta.targetDir ? "Folder" : "File"),
                pair("Mode", Format.permissions(row.p))]
    case LOADING:
        return [pair("Kind", kindName), pair("Size", Format.rowSize(row)), pair("State", "loading")]
    case ERROR:
        return [pair("Kind", kindName), pair("Size", Format.rowSize(row)),
                pair("Mode", Format.permissions(row.p)), pair("Owner", e.owner || "")]
    }
    // Unsupported, which is also where a kind whose facts are a later plan lands until it arrives.
    return head.concat([pair("Mode", Format.permissions(row.p))])
}

// The archive's one kind fact, the board's "3, 240.8 kB out": Size already says what it weighs packed.
function archiveFact(e) {
    if (!e.entries) return ""
    return e.unpacked ? e.entries + ", " + e.unpacked + " out" : String(e.entries)
}

// The multi-selection summary, which is a summary and never a collage: counts and a combined size.
// selectionCount is the true size of the selection; rows is only what the held window actually
// carries. A selection wider than that window cannot be summed without a metadata sweep, which this
// codebase refuses everywhere, so the numbers become floors and say so rather than undercounting.
function multiFacts(rows, selectionCount) {
    var groups = kindGroups(rows)
    var bytes = 0, hasDirectories = false
    var newest = 0
    var oldest = 0
    var counted = 0
    for (var i = 0; i < rows.length; i++) {
        var r = rows[i]
        if (!r) {
            continue
        }
        counted++
        if (r.d) hasDirectories = true
        else bytes += r.s
        if (newest === 0 || r.m > newest) { newest = r.m }
        if (oldest === 0 || r.m < oldest) { oldest = r.m }
    }
    var floor = (selectionCount !== undefined && selectionCount > counted) ? "> " : ""
    return filled([pair("Kinds", kindSummary(groups, floor)), pair("Combined", hasDirectories ? "Not calculated" : floor + Format.size(bytes)),
            pair("Newest", newest > 0 ? floor + Format.date(newest) : ""),
            pair("Oldest", oldest > 0 ? floor + Format.date(oldest) : "")])
}

// The distinct kinds in a selection, in the order they were met, each carrying the mark its own
// first member draws. The frame's stack and the Kinds row under it read this one pass, so the two
// can never group the same selection differently.
function kindGroups(rows) {
    var order = []
    var byWord = {}
    for (var i = 0; i < rows.length; i++) {
        var r = rows[i]
        if (!r) {
            continue
        }
        var word = kindWord(r.i)
        if (byWord[word] === undefined) {
            byWord[word] = { word: word, glyph: Icons.glyphFor(r.i), count: 0 }
            order.push(byWord[word])
        }
        byWord[word].count++
    }
    return order
}

// The canvas stacks three marks over a multi-selection, one per kind, front-most the kind the Kinds
// row names first. Three is the canvas's own count and the most a 16 by 10 frame reads at a glance.
var MULTI_MARK_CAP = 3

function multiMarks(rows) {
    var groups = kindGroups(rows || [])
    var marks = []
    for (var i = 0; i < groups.length && i < MULTI_MARK_CAP; i++) {
        marks.push(groups[i].glyph)
    }
    return marks
}

// "2 images, 1 video, 1 text", the canvas's own phrasing for the Kinds row.
function kindSummary(groups, floor) {
    var parts = []
    for (var i = 0; i < groups.length; i++) {
        var g = groups[i]
        parts.push((floor || "") + g.count + " " + (g.count === 1 ? g.word : plural(g.word)))
    }
    return parts.join(", ")
}

function plural(word) {
    return word === "text" ? word : word + "s"
}

function kindWord(icon) {
    switch (Kinds.kindState(icon)) {
    case IMAGE: return "image"
    case VIDEO: return "video"
    case AUDIO: return "audio"
    case CODE: return "code file"
    case TEXT: return "text"
    case ARCHIVE: return "archive"
    }
    return "file"
}
