.pragma library

.import "Anchor.js" as Anchor
.import "Format.js" as Format
.import "Nav.js" as Nav
.import "Swap.js" as Swap

// The status line a reload answers with, one whole sentence per count, each filled by Format.fill.
var WORDS = {
    one: "Reloaded · 1 row changed",
    many: "Reloaded · %1 rows changed"
}

// Sample input: 2 is "Reloaded · 2 rows changed", 1 is "Reloaded · 1 row changed".
function line(changed) {
    return changed === 1 ? WORDS.one : Format.fill(WORDS.many, Format.count(changed))
}

// Inotify hears only this kernel's own writes, so a FUSE folder (rclone, sshfs, gvfs) or a network or phone
// one never learns of a change made on the far side; the window coming back to the front re-reads it instead.
// Sample input: fsName "fuse" or class "network" is true, "btrfs" with class "" is false, and so is a listing out.
function blind(pane) {
    var remote = pane.fsName === "fuse" || pane.storageClass === "network" || pane.storageClass === "phone"
    return remote && pane.visible !== false && !pane.listInFlight && pane.searchMode.length === 0 && pane.recentMode.length === 0
}

// The watched re-read, owed as a changed line owes it, so a rename editor, menu or drag still holds it; true when owed.
function returned(pane) {
    if (!blind(pane))
        return false
    pane.wire.stale = true
    pane.wire.reread()
    return true
}

// The window's hold on the front: 0 before it first arrives, 1 while it holds it, 2 once it has left. Only
// a return from 2 re-reads, so the launch's own arrival and a repeated activation re-read nothing.
// Sample input: (0, true) is 1, (1, false) is 2, (2, true) is 1 after returned(), (0, false) stays 0.
function front(pane, was, active) {
    if (!active)
        return was === 0 ? 0 : 2
    if (was === 2)
        returned(pane)
    return 1
}

// A listing out refuses itself and a search owns its header, so both go quiet.
function begin(pane, wire) {
    if (Nav.loading(pane)) {
        pane.message(Swap.LOADING, false)
        return false
    }
    if (pane.searchMode.length > 0)
        return false
    // A reload past the wait retries the folder waited on, which a first listing has not yet made the pane's path.
    var waitedOn = pane.listingState === "waiting" ? Nav.trimSlash(pane.listingPath || "") : ""
    // A reload that lists lets a waiting listing go first, the way ui/Pane.qml openWithoutHistory does.
    Anchor.clearWaiting(pane)
    // Recent re-reads its history rather than re-listing its base, which is the root.
    if (pane.recentMode.length > 0) {
        pane.refresh("")
    } else {
        if (waitedOn.length > 0 && waitedOn !== pane.path) {
            pane.openWithoutHistory(waitedOn)
            return true
        }
        wire.anchor = Anchor.watched(pane, true, pane.anchorRowHeight)
    }
    // Set after the anchored re-read above, because opening the listing clears it first.
    pane.reloadFrom = pane.total
    pane.reloadChanged = -1
    return true
}

// Runs on every rows reply; only a manual reload owes a notice, said whenever rows changed.
function landed(pane) {
    if (pane.reloadFrom < 0)
        return ""
    // The backend counts added plus removed on a same-path re-list; a missing one falls back to net delta.
    var changed = pane.reloadChanged >= 0 ? pane.reloadChanged : Math.abs(pane.total - pane.reloadFrom)
    pane.reloadFrom = -1
    pane.reloadChanged = -1
    if (changed === 0)
        return ""
    var text = line(changed)
    pane.message(text, false)
    return text
}
