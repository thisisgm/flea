.pragma library

.import "Protocols.js" as Protocols

// Sample input, captured live on the box with one network share mounted (2026-08-31):
// Drive(0): KBG40ZNS256G NVMe KIOXIA 256GB
//   Type: GProxyDrive (GProxyVolumeMonitorUDisks2)
// Mount(0): isos on 192.168.1.10 -> smb://192.168.1.10/isos/
//   Type: GDaemonMount
// Only Mount() lines matter here; Drive() and every indented "Type:" line are noise for the rail.
function parseMounts(output) {
    var out = []
    var lines = String(output || "").split("\n")
    for (var i = 0; i < lines.length; i++) {
        var m = lines[i].match(/^Mount\(\d+\):\s*(.+?)\s*->\s*(\S+)\s*$/)
        if (!m)
            continue
        var uri = m[2]
        // corner: a local device mount (file://) is Favorites territory, not Network; Places.js skips the inverse.
        if (uri.indexOf("file://") === 0)
            continue
        out.push({ label: Protocols.shareName(m[1], uri), uri: uri })
    }
    return out
}

// Sample input, "gio info" under the C locale ui/NetworkMounts.qml pins on every gio call:
// uri: smb://192.168.1.10/isos/
// local path: /run/user/1000/gvfs/smb-share:server=192.168.1.10,share=isos
// Only a location GVFS exposes through its FUSE mount prints that line, so "" means there is no
// browsable folder. One resolver, so the product and tests/js/network.js read the same wording.
function localPath(body) {
    var lines = String(body || "").split("\n")
    for (var i = 0; i < lines.length; i++) {
        if (lines[i].indexOf("local path: ") === 0)
            return lines[i].substring("local path: ".length).trim()
    }
    return ""
}

// Sample input: the operator's real bookmarks file, ui/js/Places.js "bookmarks" reads the same lines.
// file:///home/gm/Downloads Downloads
// smb://192.168.1.10/ NAS
// Places.bookmarks() keeps the file:// line and drops the smb:// one; this is the exact mirror.
function nonFileBookmarks(body) {
    var out = []
    var lines = String(body || "").split("\n")
    for (var i = 0; i < lines.length; i++) {
        var line = lines[i].trim()
        if (line.length === 0 || line.indexOf("file://") === 0)
            continue
        var space = line.indexOf(" ")
        var uri = space < 0 ? line : line.substring(0, space)
        // corner: a line with no scheme at all is not a bookmark, not just not a file:// one.
        if (!/^[a-z][a-z0-9+.-]*:\/\//i.test(uri))
            continue
        var label = space < 0 ? "" : line.substring(space + 1).trim()
        out.push({ uri: uri, label: label.length > 0 ? label : leaf(uri) })
    }
    return out
}

// The bare-root form (smb://host/) has no path leaf, so this falls back to the host itself.
function leaf(uri) {
    var stripped = String(uri).replace(/^[a-z][a-z0-9+.-]*:\/\//i, "").replace(/\/+$/, "")
    var slash = stripped.lastIndexOf("/")
    var tail = slash < 0 ? stripped : stripped.substring(slash + 1)
    return tail.length > 0 ? tail : stripped
}

// decodeURIComponent throws on a malformed escape, and a bookmarks line is arbitrary text, so a
// failure answers the raw path rather than taking the whole rail rebuild with it.
function decodePath(raw) {
    try {
        return decodeURIComponent(String(raw))
    } catch (e) {
        return String(raw)
    }
}

// One canonical form: every trailing slash stripped except a bare host root, which keeps one, and
// a port the scheme would have used anyway dropped, because gio's own listing never reports one.
function normalize(uri) {
    var stripped = Protocols.stripDefaultPort(String(uri || "").replace(/\/+$/, ""))
    var bareRoot = /^[a-z][a-z0-9+.-]*:\/\/[^\/]+$/i.test(stripped)
    return bareRoot ? stripped + "/" : stripped
}

// Sample input, captured live on the box with a USB stick plugged in (2026-09-02), from
// lsblk --json -o NAME,LABEL,MOUNTPOINT,RM,SIZE,TYPE,MODEL:
// {"blockdevices":[
//   {"name":"sda","label":null,"mountpoint":null,"rm":true,"size":"116.1G","type":"disk","model":"USB Flash Disk",
//    "children":[{"name":"sda1","label":"128GB","mountpoint":"/run/media/gm/128GB","rm":true,"size":"116.1G","type":"part","model":null}]}]}
// Two row kinds come out: one "disk" row for the box's own internal disk, then one "volume" row
// per removable partition, mounted or not. ui/DeviceMounts.qml turns these into rail entries.
function parseDevices(body) {
    var tree
    try {
        tree = JSON.parse(String(body || ""))
    } catch (e) {
        // A parse failure returns the empty shape rather than throwing, so the rail self-hides.
        return []
    }
    var nodes = (tree && tree.blockdevices) || []
    var out = []
    var disk = internalDisk(nodes)
    if (disk)
        out.push(disk)
    collectVolumes(nodes, "", out)
    return out
}

// corner: one internal disk on this box, so the first non-removable disk is "the" disk and its row means "/".
function internalDisk(nodes) {
    for (var i = 0; i < nodes.length; i++) {
        var n = nodes[i]
        // lsblk on this box reports rm as a JSON boolean, measured 2026-09-02.
        if (!n.name || n.type !== "disk" || n.rm)
            continue
        // zram and loop devices are type "disk" too, and neither is a disk anyone browses.
        if (/^(zram|loop)/.test(String(n.name)))
            continue
        return { kind: "disk", label: String(n.name), device: "/dev/" + n.name, path: "/", mounted: true }
    }
    return null
}

// A removable row is a partition on a removable disk, or a removable disk nobody ever partitioned.
function collectVolumes(nodes, model, out) {
    for (var i = 0; i < nodes.length; i++) {
        var n = nodes[i]
        var kids = n.children || []
        // Only the disk carries a product name, so it is passed down to its own partitions.
        var own = n.model ? String(n.model) : model
        if (n.name && n.rm && (n.type === "part" || (n.type === "disk" && kids.length === 0)))
            out.push(volumeRow(n, own))
        collectVolumes(kids, own, out)
    }
}

// The label ladder is the filesystem label, then the drive's product name, then the kernel name.
function volumeRow(n, model) {
    var path = n.mountpoint ? String(n.mountpoint) : ""
    var label = n.label ? String(n.label) : (model.length > 0 ? model : String(n.name))
    return { kind: "volume", label: label, device: "/dev/" + n.name, path: path, mounted: path.length > 0 }
}

// Sample input: two arrays of rail entries as ui/NetworkMounts.qml and ui/DeviceMounts.qml build
// them, [{path:"", label:"NAS", group:"network", kind:"share", uri:"smb://example.com/data",
// mounted:false, glyph:"server"}]. A poll that found no change must not assign a fresh array: the
// Repeater rebinds every rail row on the assignment, and a rebound row is a row whose editor lost
// what was typed into it. Both builders are covered, so every field either one writes is compared.
function sameEntries(a, b) {
    if (!a || !b || a.length !== b.length)
        return false
    for (var i = 0; i < a.length; i++) {
        if (!sameEntry(a[i], b[i]))
            return false
    }
    return true
}

// The two shapes differ only in uri against device, and an absent field is undefined on both sides.
function sameEntry(x, y) {
    return x.path === y.path && x.label === y.label && x.group === y.group && x.kind === y.kind
        && x.uri === y.uri && x.device === y.device && x.mounted === y.mounted && x.glyph === y.glyph
}

// Sample input: one rail entry as ui/DeviceMounts.qml and ui/NetworkMounts.qml build them,
// {label:"128GB", group:"device", kind:"volume", device:"/dev/sda1", mounted:true}.
// A removable volume ejects and a mounted network share unmounts; every other rail row offers
// neither and opens no menu. The kind is read here, never re-derived: the internal disk reads as
// mounted too, the Dropbox row is a local folder the stock service owns, and a favourite is not a
// mount. gio's -f is offered nowhere: forcing an unmount over an open write is how data is lost.
function railMenu(entry) {
    if (!entry || !entry.mounted)
        return []
    if (entry.group === "device" && entry.kind === "volume")
        return [{ label: "Eject", action: "eject", glyph: "eject" }]
    if (entry.group === "network" && entry.kind === "share")
        return [{ label: "Unmount", action: "unmount", glyph: "eject" }]
    return []
}

// What the rail's own right click opens: the release row above, then the two rows a saved place owns
// whether or not anything mounted it, marked as a removal because forgetting a place trashes
// nothing. ui/js/Eject.js reads railMenu and never this, so Ctrl+E still refuses an unmounted row.
function rowMenu(entry) {
    var rows = railMenu(entry)
    if (entry && entry.group === "network" && entry.kind === "share") {
        rows.push({ label: "Rename", action: "rename", glyph: "rename" })
        rows.push({ label: "Remove", action: "remove", glyph: "minus" })
    }
    return rows
}

// The handle a chosen menu row carries back: a volume's device node, a share's uri, "" for a row
// with no release. The rail rebuilds on a five second poll, so an index taken when the menu opened
// can name a different row by the time a row inside it is chosen; a key cannot.
function railKey(entry) {
    if (!entry)
        return ""
    if (entry.group === "device" && entry.kind === "volume")
        return String(entry.device || "")
    if (entry.group === "network" && entry.kind === "share")
        return String(entry.uri || "")
    return ""
}

// Where that key resolves back to a position in the group's own list, or -1 when the row is gone.
// An empty key matches nothing: a row that carries no key must never resolve to whatever is at 0.
function rowByKey(entries, key) {
    if (String(key || "").length === 0)
        return -1
    var list = entries || []
    for (var i = 0; i < list.length; i++) {
        if (railKey(list[i]) === key)
            return i
    }
    return -1
}

// The mounted row with a release whose path holds the directory, or null: what Ctrl+E acts on from
// a listing. A share's FUSE path is not known to the rail (its path is ""), so a listing inside one
// answers null rather than guessing, and the internal disk never offers a release to begin with.
function holding(entries, dir) {
    var list = entries || []
    var path = String(dir || "")
    for (var i = 0; i < list.length; i++) {
        var e = list[i]
        if (!e.path || railMenu(e).length === 0)
            continue
        if (path === e.path || path.indexOf(e.path + "/") === 0)
            return e
    }
    return null
}

// The keyboard's route into the rail menu, and where a rail row without one is answered: the sheet
// advertises m, so a key that silently did nothing on a favourite would read as a broken one.
function raiseMenu(pane, sidebar) {
    var entry = sidebar.entries[sidebar.cursorIndex]
    if (!entry)
        return
    if (rowMenu(entry).length > 0)
        sidebar.openCursorMenu()
    else
        pane.message(entry.label + " has nothing to eject or unmount.", false)
}

// The rail menu's chosen row, handed the row's key rather than its position: the rail rebuilds on
// a five second poll, so the index the menu opened over can name a different row by now. A key
// that no longer names a row does nothing, because the row it named has left the rail already.
// Both Services re-check the kind themselves; this only resolves which row was meant, and the rail
// itself owns the two rows that need no mount at all.
function release(action, key, devices, mounts, sidebar) {
    if (action === "eject") {
        var volume = rowByKey(sidebar.deviceEntries, key)
        if (volume >= 0)
            devices.eject(volume)
        return
    }
    var share = rowByKey(sidebar.networkEntries, key)
    if (share < 0)
        return
    if (action === "unmount")
        mounts.unmount(share)
    else if (action === "rename")
        sidebar.startRename(sidebar.favoriteEntries.length + share)
    else if (action === "remove")
        mounts.forget(sidebar.networkEntries[share].uri)
}

// Sample input: the operator's own bookmarks file, favourites and network places in one list.
// smb://192.168.1.10/isos NAS isos
// Matched normalized, the way Places.relabel matches, so a line spelling out a default port still
// names the same place as the live mount that omits it.
function removeBookmark(body, uri) {
    var target = normalize(uri)
    var lines = String(body || "").split("\n")
    var out = []
    for (var i = 0; i < lines.length; i++) {
        var space = lines[i].indexOf(" ")
        var one = space < 0 ? lines[i] : lines[i].substring(0, space)
        if (target.length > 0 && one.length > 0 && normalize(one) === target)
            continue
        out.push(lines[i])
    }
    return out.join("\n")
}
