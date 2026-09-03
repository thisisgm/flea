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

function taggedShare(uri, label, mounted, meta, health, forceShare) {
    var source = meta || {}
    return {
        path: "", label: label, group: "network", kind: forceShare ? "share" : (source.kind || "share"),
        uri: uri, mounted: mounted, glyph: source.glyph || "server",
        origin: source.origin || (mounted ? "mount" : "bookmark"),
        health: mounted ? "mounted" : (health || source.health || "unknown"),
        address: source.address || authority(uri), peerId: source.peerId || "",
        taildrop: !!source.taildrop, mac: source.mac || ""
    }
}

function authority(uri) {
    var m = String(uri || "").match(/^[a-z][a-z0-9+.-]*:\/\/([^\/]+)/i)
    return m ? m[1].replace(/^.*@/, "") : ""
}

// gvfsd-sftp (and ftp) mounts one connection per host, not per path. SMB/NFS stay per share.
function connectionKey(uri) {
    var n = Mounts.normalize(uri)
    var s = (n.match(/^([a-z][a-z0-9+.-]*)/i) || [""])[0].toLowerCase()
    if (s !== "sftp" && s !== "ssh" && s !== "ftp" && s !== "ftps")
        return n
    var m = n.match(/^([a-z][a-z0-9+.-]*:\/\/[^\/]+)/i)
    return m ? Mounts.normalize(m[1]) : n
}

function covers(liveUri, otherUri) {
    var a = connectionKey(liveUri)
    return a.length > 0 && a === connectionKey(otherUri)
}

function coveringUri(mounts, uri) {
    var list = mounts || []
    for (var i = 0; i < list.length; i++) {
        if (covers(list[i].uri, uri))
            return list[i].uri
    }
    return uri
}

// Live mounts first, then bookmarks not already shown. A bookmark's label wins over gio's
// "user on host" name. An SFTP path on a live host is mounted even when gio only lists the root.
function networkEntries(mounts, marks, discovered, healthMap) {
    var labels = {}
    var list = marks || []
    for (var j = 0; j < list.length; j++) {
        var k = Mounts.normalize(list[j].uri)
        if (k.length === 0 || labels[k] !== undefined)
            continue
        labels[k] = list[j].label
    }
    var out = []
    var seen = {}
    var live = mounts || []
    for (var i = 0; i < live.length; i++) {
        var mkey = Mounts.normalize(live[i].uri)
        if (seen[mkey])
            continue
        seen[mkey] = true
        out.push(taggedShare(live[i].uri, labels[mkey] || live[i].label, true,
                             matching(discovered, live[i].uri), stateFor(healthMap, live[i].uri), true))
    }
    for (var n = 0; n < list.length; n++) {
        var bkey = Mounts.normalize(list[n].uri)
        if (seen[bkey])
            continue
        seen[bkey] = true
        var mounted = false
        for (var k = 0; k < live.length; k++) {
            if (covers(live[k].uri, list[n].uri)) {
                mounted = true
                break
            }
        }
        out.push(taggedShare(list[n].uri, list[n].label, mounted,
                             matching(discovered, list[n].uri), stateFor(healthMap, list[n].uri), true))
    }
    var extras = discovered || []
    for (var x = 0; x < extras.length; x++) {
        var duplicate = false
        for (var y = 0; y < out.length; y++) {
            if (covers(out[y].uri, extras[x].uri)) {
                duplicate = true
                break
            }
        }
        if (duplicate)
            continue
        var isMounted = false
        for (var z = 0; z < live.length; z++) {
            if (covers(live[z].uri, extras[x].uri)) {
                isMounted = true
                break
            }
        }
        var combined = matching(extras, extras[x].uri) || extras[x]
        out.push(taggedShare(extras[x].uri, extras[x].label, isMounted, combined,
                             stateFor(healthMap, extras[x].uri), false))
    }
    return out
}

function matching(entries, uri) {
    var list = entries || []
    var found = null
    for (var i = 0; i < list.length; i++) {
        if (!covers(list[i].uri, uri))
            continue
        if (!found) {
            found = copyMeta(list[i])
            continue
        }
        found.taildrop = found.taildrop || !!list[i].taildrop
        if (!found.peerId) found.peerId = list[i].peerId || ""
        if (!found.address) found.address = list[i].address || ""
        if (!found.mac) found.mac = list[i].mac || ""
        if ((found.health || "unknown") === "unknown" && list[i].health)
            found.health = list[i].health
    }
    if (found && found.mac)
        found.origin = "lan"
    return found
}

function copyMeta(entry) {
    var out = {}
    for (var key in entry) out[key] = entry[key]
    return out
}

function stateFor(map, uri) {
    return (map && map[Mounts.normalize(uri)]) || ""
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

// Sample input: body as relabel's, oldUri the bookmark being rewritten, newUri the form's Mounts-as
// line. Matching is the same normalized compare relabel uses, so a live gio trailing slash still
// finds the written line. An empty oldUri matches nothing and appends, which is the add-dialog path.
function replace(body, oldUri, newUri, label) {
    var trimmed = String(label || "").replace(/[\r\n]/g, "").trim()
    var next = Mounts.normalize(newUri)
    if (next.length === 0)
        return String(body || "")
    if (trimmed.length === 0)
        trimmed = Mounts.leaf(next)
    var target = Mounts.normalize(oldUri)
    var lines = String(body || "").split("\n")
    var found = false
    for (var i = 0; i < lines.length; i++) {
        var line = lines[i]
        if (line.trim().length === 0)
            continue
        var space = line.indexOf(" ")
        var uri = space < 0 ? line : line.substring(0, space)
        if (target.length > 0 && Mounts.normalize(uri) === target) {
            lines[i] = next + " " + trimmed
            found = true
        }
    }
    if (found)
        return lines.join("\n")
    var out = String(body || "")
    if (out.length > 0 && out.charAt(out.length - 1) !== "\n")
        out += "\n"
    return out + next + " " + trimmed + "\n"
}

// Drops every line whose uri matches, leaving the rest byte-identical. An empty uri is a no-op
// so a Dropbox row with no bookmark cannot wipe the file.
function remove(body, uri) {
    var target = Mounts.normalize(uri)
    if (target.length === 0)
        return String(body || "")
    var lines = String(body || "").split("\n")
    var out = []
    for (var i = 0; i < lines.length; i++) {
        var line = lines[i]
        if (line.trim().length === 0) {
            out.push(line)
            continue
        }
        var space = line.indexOf(" ")
        var u = space < 0 ? line : line.substring(0, space)
        if (Mounts.normalize(u) === target)
            continue
        out.push(line)
    }
    return out.join("\n")
}
