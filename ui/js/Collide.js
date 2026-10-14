.pragma library
.import "Format.js" as Format
.import "Ops.js" as Ops
.import "Status.js" as Status
.import "Swap.js" as Swap

// The question a paste or a drop asks before it lands on names that exist, as decisions ui/CollideHost.qml and ui/CollideConfirm.qml wire.

// Whole sentences with "%1" for a value, filled by Format.fill so each translates as one; one per count, because the verb changes with it.
var WORDS = {
    existsOne: "%1 already exists in %2",
    existsMany: "%1 items already exist in %2"
}

// Left to right, the order h, l and Tab walk.
var BUTTONS = ["cancel", "skip", "keep", "merge", "replace"]
var LABELS = { cancel: "Cancel", skip: "Skip", keep: "Keep both", merge: "Merge", replace: "Replace" }
// Where the card opens, and so what a reflexive Enter takes: the one choice that changes nothing already there.
var START = "keep"
var EXPLAIN = "Replaced items go to Trash, and Undo restores them."
// What a transfer carries when nothing collided: any name that appears after the question is refused.
var NONE = "refuse"
var WAITING = "A transfer is already waiting on its question; try again once it is answered."

// The folder's own name, which is what the title says; the root has no leaf, so it says itself.
function folderName(dest) {
    var path = String(dest || "")
    var cut = path.lastIndexOf("/")
    var leaf = cut >= 0 ? path.substring(cut + 1) : path
    return leaf.length > 0 ? leaf : "/"
}

// "screenshot.png already exists in Pictures" for one, "3 items already exist in Downloads" for several.
function title(names, total, dest) {
    if (total === 1 && names.length > 0)
        return Format.fill(WORDS.existsOne, names[0].n, folderName(dest))
    return Format.fill(WORDS.existsMany, Format.count(total), folderName(dest))
}

// The row under the list, empty when every collision is already on it.
function more(total, shown) {
    return total > shown ? "and " + Format.count(total - shown) + " more" : ""
}

// Only the buttons the card shows take the ring; a hidden Merge never holds it.
function shownButtons(showMerge) {
    return showMerge === true ? BUTTONS : BUTTONS.filter(function (b) { return b !== "merge" })
}

// h and l stop at the ends like TrashConfirm's pair; Tab and Backtab come round. A focus not in order starts from start.
function walk(order, focus, key, modifiers, start) {
    var at = order.indexOf(focus)
    if (at < 0)
        at = Math.max(0, order.indexOf(start))
    var last = order.length - 1
    if (key === Qt.Key_H || key === Qt.Key_Left)
        return order[Math.max(0, at - 1)]
    if (key === Qt.Key_L || key === Qt.Key_Right)
        return order[Math.min(last, at + 1)]
    // Shift+Tab reaches a Keys handler as Backtab or as Tab with Shift held, depending on the input path.
    if (key === Qt.Key_Backtab || key === Qt.Key_Tab && (modifiers & Qt.ShiftModifier))
        return order[(at + last) % order.length]
    if (key === Qt.Key_Tab)
        return order[(at + 1) % order.length]
    return order[at]
}

function moved(focus, key, showMerge, modifiers) {
    return walk(shownButtons(showMerge), focus, key, modifiers, START)
}

function activates(key) {
    return key === Qt.Key_Return || key === Qt.Key_Enter || key === Qt.Key_Space
}

// The transfer the card holds, its rows stamped in the numbering they were read in; a menu's is resolved from the menu's own capture.
function waiting(request, held) {
    return request.menuId ? request : Swap.named(request, held)
}

// One question at a time, open or still in flight: a second is refused out loud rather than replacing the first.
function refusal(opened, pending) {
    return opened || pending !== null ? WAITING : ""
}

// A refused question ends the wait the same way an answered one does: the card never opens, so nothing stays waiting.
function droppedPending(where, input) {
    return where === "stale" && (input === "collisions" || input === "mergecollisions")
}

// The question names the same sources the transfer will, in the backend's order: a shelf drag's paths, a menu's selection, paths, rows.
function question(request, probe, id) {
    var asked = { c: "collisions", id: id, dest: request.dest }
    if (request.fat)
        asked.fat = request.fat
    if (probe && probe.length > 0)
        asked.paths = probe
    else if (request.menuId)
        asked.menuId = request.menuId
    else if (request.paths && request.paths.length > 0)
        asked.paths = request.paths
    else
        asked.rows = request.rows || []
    // Rows are asked about in the numbering the transfer will name, ui/js/Swap.js named().
    if (asked.rows && request.listing !== undefined)
        asked.listing = request.listing
    return asked
}

// The waiting transfer with the answer on it; the id is what lets the backend cover only the names it listed.
function transfer(request, choice, id) {
    return Object.assign({}, request, { collide: choice, collideId: id })
}

// Paste as links answers on the waiting request like a transfer; rows ride along only when named, so paths-only pastes skip listing checks.
function link(request, choice, id) {
    var out = { c: "link", op: request.op, paths: request.paths || [],
                dest: request.dest, collide: choice, collideId: id }
    if (request.fat)
        out.fat = request.fat
    if (request.rows && request.rows.length > 0) out.rows = request.rows
    if (request.listing !== undefined) out.listing = request.listing
    return out
}

// After Merge, one walk of the sources against the destination, before anything is written.
function mergeQuestion(request, probe, id) {
    var asked = { c: "mergecollisions", id: id, dest: request.dest }
    if (probe && probe.length > 0)
        asked.paths = probe
    else if (request.menuId)
        asked.menuId = request.menuId
    else if (request.paths && request.paths.length > 0)
        asked.paths = request.paths
    else
        asked.rows = request.rows || []
    if (asked.rows && request.listing !== undefined)
        asked.listing = request.listing
    return asked
}

// The merge transfer with the second card's answer on it; the id names the walk it answers.
function mergeTransfer(request, inner, id) {
    return Object.assign({}, request, { collide: "merge", collideId: id, inner: inner })
}

// The second card names the folder it merges into: the one merging folder, else the destination.
function mergeDest(request, outerNames) {
    var folders = (outerNames || []).filter(function (n) { return n.d === true }).map(function (n) { return n.n })
    if (folders.length === 1)
        return String(request.dest).replace(/\/$/, "") + "/" + folders[0]
    return request.dest
}

// The board's merged line as whole sentences, one per count, joined once; the undo hint stays its suffix.
var MERGED_ONE = "Merged 1 item into %1"
var MERGED_MANY = "Merged %1 items into %2"
var MERGED_KEPT = "%1 kept both"
var MERGED_FAILED = "%1 failed"
var MERGED_SKIPPED = "%1 skipped"
function merged(count, keptBoth, leaf, failed, skipped, cancelled, note) {
    var segments = [count === 1 ? MERGED_ONE.replace("%1", leaf) : MERGED_MANY.replace("%1", Format.count(count)).replace("%2", leaf)]
    if (keptBoth > 0) segments.push(MERGED_KEPT.replace("%1", Format.count(keptBoth)))
    if (failed > 0) segments.push(MERGED_FAILED.replace("%1", Format.count(failed)))
    if (skipped > 0) segments.push(MERGED_SKIPPED.replace("%1", Format.count(skipped)))
    if (cancelled) segments.push("cancelled")
    if (count > 0 && String(note || "").length > 0) segments.push(note)
    var line = segments.join(" · ")
    return line + (count > 0 ? Status.undoHint() : "")
}

// The done line off the transferdone line itself, so a refused merge never mislabels the next copy.
function doneLine(t, r) {
    if (r.into && r.into.length > 0)
        return merged(r.merged || 0, r.keptBoth || 0, r.into, r.failed, r.skipped, r.cancelled, r.note)
    return Ops.transferDone(t, r.ok, r.failed, r.skipped, r.cancelled, r.durable, r.note)
}

// Drops callout 3, the FAT names card: every string is one whole sentence with placeholders for its values.
var FAT_DEFAULT = "rename"
var FAT_BUTTONS = { cancel: "Cancel", skip: "Skip them", rename: "Rename and copy" }
var FAT_TITLE_ONE = "1 name cannot exist on %1 (%2)"
var FAT_TITLE_MANY = "%1 names cannot exist on %2 (%3)"
var FAT_NOTE = "%1 refuses : ? * \" < > | and trailing dots or spaces."
// The mount table's own word as the card spells it; vfat could be any FAT width, so it claims none.
var FAT_FILESYSTEMS = { exfat: "exFAT", vfat: "FAT", fat: "FAT", msdos: "FAT", fat12: "FAT12", fat16: "FAT16", fat32: "FAT32", ntfs: "NTFS", ntfs3: "NTFS" }
function filesystemLabel(fstype) {
    return FAT_FILESYSTEMS[String(fstype).toLowerCase()] || String(fstype)
}
// One pass over the placeholders, so a value that holds a placeholder or a dollar sign is never read twice.
function fill(template, values) {
    return template.replace(/%(\d)/g, function (whole, n) { return values[n - 1] })
}
function cardTitle(total, volume, fstype) {
    var label = filesystemLabel(fstype)
    return total === 1 ? fill(FAT_TITLE_ONE, [volume, label]) : fill(FAT_TITLE_MANY, [Format.count(total), volume, label])
}
function cardNote(fstype) {
    return fill(FAT_NOTE, [filesystemLabel(fstype)])
}
