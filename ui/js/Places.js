.pragma library

.import "Mounts.js" as Mounts

// Sample input: XDG_DOWNLOAD_DIR="$HOME/Downloads"
function userDirs(body, home) {
    var out = []
    var lines = String(body || "").split("\n")
    for (var i = 0; i < lines.length; i++) {
        var kv = lines[i].match(/^\s*XDG_([A-Z]+)_DIR\s*=\s*"([^"]*)"/)
        if (!kv)
            continue
        var path = kv[2].replace("$HOME", home).replace(/\/+$/, "")
        // corner: this box points TEMPLATES, PUBLICSHARE and DESKTOP at $HOME, which is not a favourite.
        if (path.length === 0 || path === home)
            continue
        out.push({ path: path, label: leaf(path) })
    }
    return out
}

// Sample input: file:///home/gm/Downloads Downloads
function bookmarks(body) {
    var out = []
    var lines = String(body || "").split("\n")
    for (var i = 0; i < lines.length; i++) {
        var line = lines[i].trim()
        // corner: a bookmark may be smb:// or sftp://, which is a Location and not a Favorite.
        if (line.indexOf("file://") !== 0)
            continue
        var space = line.indexOf(" ")
        var uri = space < 0 ? line : line.substring(0, space)
        var label = space < 0 ? "" : line.substring(space + 1).trim()
        var path = Mounts.decodePath(uri.substring("file://".length))
        out.push({ path: path, label: label.length > 0 ? label : leaf(path) })
    }
    return out
}

// The FAVORITES rail's rows, Home first and always, then XDG dirs and bookmarked places in file
// order, a path seen twice keeping its first position. glyphFor resolves each row's mark so the
// rail's Icons import stays the rail's own business; see ui/Sidebar.qml's rebuild.
function favorites(home, dirsText, marksText, glyphFor) {
    var favs = [{ path: home, label: "Home", group: "favorite", kind: "favorite", glyph: glyphFor("Home") }]
    var seen = {}
    seen[home] = true
    var groups = [userDirs(dirsText, home), bookmarks(marksText)]
    for (var g = 0; g < groups.length; g++) {
        for (var i = 0; i < groups[g].length; i++) {
            if (seen[groups[g][i].path])
                continue
            var e = groups[g][i]
            seen[e.path] = true
            favs.push({ path: e.path, label: e.label, group: "favorite", kind: "favorite", glyph: glyphFor(e.label) })
        }
    }
    return favs
}

function leaf(path) {
    var cut = path.lastIndexOf("/")
    return cut < 0 ? path : path.substring(cut + 1)
}

// Sample input: "smb://192.168.1.10/data NAS"; see AGENTS.md "Places.relabel" for the matching, duplicate and control-character rules.
function relabel(body, path, name) {
    // A trust boundary: an embedded newline could otherwise split one bookmark into two lines.
    var trimmed = String(name || "").replace(/[\r\n]/g, "").trim()
    if (trimmed.length === 0)
        return String(body || "")
    var target = Mounts.normalize(path)
    var lines = String(body || "").split("\n")
    var found = false
    for (var i = 0; i < lines.length; i++) {
        // Read off the trimmed line the way Mounts.removeBookmark reads it, or an indented bookmark
        // is one Remove can drop and this could only ever duplicate.
        var line = lines[i].trim()
        if (line.length === 0)
            continue
        var space = line.indexOf(" ")
        var uri = space < 0 ? line : line.substring(0, space)
        if (Mounts.normalize(uri) === target) {
            lines[i] = uri + " " + trimmed
            found = true
        }
    }
    if (found)
        return lines.join("\n")
    var out = String(body || "")
    if (out.length > 0 && out.charAt(out.length - 1) !== "\n")
        out += "\n"
    return out + target + " " + trimmed + "\n"
}

var WIDTH_STOPS = [160, 192, 224, 256]
function sidebarWidth(value) {
    if (typeof value !== "number" || !isFinite(value)) return 192
    var nearest = WIDTH_STOPS[0]
    for (var i = 1; i < WIDTH_STOPS.length; i++) {
        if (Math.abs(WIDTH_STOPS[i] - value) < Math.abs(nearest - value)) nearest = WIDTH_STOPS[i]
    }
    return nearest
}

function recordError(record) {
    if (!record || typeof record !== "object" || Array.isArray(record)) return "invalid favorite record"
    if (typeof record.label !== "string" || record.label.trim().length === 0) return "label needs visible text"
    if (typeof record.path !== "string" || /[\u0000-\u001f\u007f]/.test(record.path)) return "invalid path"
    if (record.path.charAt(0) === "/" || record.path === "~" || record.path.indexOf("~/") === 0) return ""
    var schemes = ["smb://", "sftp://", "ftp://", "dav://", "davs://", "afp://", "nfs://", "file://"]
    for (var i = 0; i < schemes.length; i++) {
        if (record.path.indexOf(schemes[i]) === 0 && record.path.length > schemes[i].length) return ""
    }
    return "path must be absolute, ~/ relative, or a supported URI"
}

function storedEntries(records, home) {
    var out = []
    for (var i = 0; Array.isArray(records) && i < records.length; i++) {
        var record = records[i]
        var error = recordError(record)
        var label = record && typeof record.label === "string" ? record.label : JSON.stringify(record)
        var path = record && typeof record.path === "string" ? record.path : ""
        var resolved = path === "~" ? home : path.indexOf("~/") === 0 ? home + path.substring(1) : path
        out.push({ label: label || "Invalid favorite", path: resolved, storedPath: path,
            error: error, original: record, favouriteIndex: i, group: "favourite", kind: "favourite",
            glyph: path.indexOf("://") >= 0 ? "network" : "folder" })
    }
    return out
}

function homeEntries(home, dirsText, glyphFor) {
    var entries = [{ label: "Home", path: home }].concat(userDirs(dirsText, home))
    return entries.map(function (entry) {
        return { label: entry.label, path: entry.path, group: "home", kind: "home", glyph: glyphFor(entry.label) }
    })
}

function railIdentity(entry) {
    if (!entry) return ""
    if (entry.kind === "favourite") return JSON.stringify([entry.kind, entry.original])
    return JSON.stringify([entry.group, entry.kind, Mounts.railKey(entry) || entry.device || entry.uri || entry.path])
}

// Duplicate originals have no stored ID; a changed multiplicity cannot safely identify the selected occurrence.
function railCursorAfter(before, after, index) {
    if (before.length === 0 && index === 0) return 0
    if (index < 0 || index >= before.length) return -1
    var key = railIdentity(before[index]), oldCount = 0, occurrence = 0, matches = []
    for (var i = 0; i < before.length; i++) {
        if (railIdentity(before[i]) !== key) continue
        oldCount++
        if (i < index) occurrence++
    }
    for (var j = 0; j < after.length; j++) {
        if (railIdentity(after[j]) === key) matches.push(j)
    }
    return matches.length === oldCount ? matches[occurrence] : -1
}
