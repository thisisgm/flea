.pragma library

.import "DragOut.js" as DragOut

// Downstream (driveway-fm): drag one folder onto the rail's FAVORITES heading to pin it. Pure
// decisions alone, so tests/js/pindrop.js pins them headless; ui/PinHeadingDrop.qml owns the
// DropArea, the kind ask and the cue. The path normalisation mirrors upstream PR #139's
// favouritePath so a pin and a menu Add to Favorites agree on what "already pinned" means; drop
// this file's copy the day that PR merges and Places.js carries the pair.

// Sample input: "/home/u/Projects/" and "~/Work" name "/home/u/Projects" and "$HOME/Work".
function normalised(path, home) {
    if (typeof path !== "string" || !path.length) return ""
    if (path === "~") path = home || path
    else if (path.indexOf("~/") === 0 && home) path = home + path.substring(1)
    return path.replace(/\/+$/, "") || "/"
}

function alreadyPinned(records, path, home) {
    var target = normalised(path, home)
    if (!target) return false
    for (var i = 0; i < (records || []).length; i++)
        if (records[i] && normalised(records[i].path, home) === target) return true
    return false
}

// One local path, and only that, is a pin candidate; a shelf token is a transfer, not a pin, and
// two paths at once name no favourite. Sample input: urls ["file:///home/u/Work"] and no marker.
function candidate(marker, urls, plain, shelf) {
    if (shelf) return ""
    var paths = DragOut.sources(urls, plain, marker, shelf)
    return paths.length === 1 ? paths[0] : ""
}

// The backend's kind line arms the drop; "dir" alone does. Sample input: kinds {"/home/u/Work":"dir"}.
function kindAnswer(kinds, path) {
    return kinds && kinds[path] ? String(kinds[path]) : ""
}

function armed(kind, path) {
    return !!path && kind === "dir"
}

// The favourite's label is the folder's own name, the way the menu's Add to Favorites names it.
function label(path) {
    var parts = String(path).split("/").filter(function (part) { return part.length > 0 })
    return parts.length ? parts[parts.length - 1] : "/"
}
