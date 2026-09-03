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
        var line = lines[i]
        if (line.trim().length === 0)
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
