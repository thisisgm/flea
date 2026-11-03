.pragma library

.import "Format.js" as Format
.import "Protocols.js" as Protocols
.import "Menu.js" as Menu

// Whole sentences with "%1" for a value, filled by fill so each translates as one; replace would read "$$" in a label as a pattern.
var WORDS = {
    nothingToRelease: "%1 has nothing to eject or unmount",
    stillEjecting: "Still ejecting %1; wait for its result",
    stillUnmounting: "Still unmounting %1; wait for its result",
    anotherMounting: "Another drive is still mounting. Try again when it finishes",
    stillUnmountingTryAgain: "%1 is still unmounting. Try again when it finishes",
    ejecting: "Ejecting %1. Do not unplug it yet.",
    noFolder: "%1 was mounted, but Flea could not find its folder.",
    ejectUnfinished: "%1 did not finish ejecting and is still mounted.",
    mountFailed: "%1 could not be mounted.",
    damaged: "%1 could not be mounted: its file system is damaged or not supported.",
    unmountFailed: "%1 could not be unmounted.",
    ejectFailed: "%1 could not be ejected.",
    busy: "%1 is busy; close what is using it and try again.",
    mountRefused: "%1 was not mounted: you do not have permission.",
    unmountRefused: "%1 was not unmounted: you do not have permission.",
    ejectRefused: "%1 was not ejected: you do not have permission.",
    noDisc: "%1 has no disc or card in it.",
    notMountable: "%1 cannot be mounted on this computer.",
    anotherOpening: "Another network location is still opening. Try again when it opens",
    notSaved: "%1 is not saved, and stays in the sidebar until it is unmounted",
    forgottenStays: "%1 is forgotten, and stays in the sidebar until it is unmounted",
    forgotten: "%1 is forgotten",
    placeSaved: "%1 is saved to places, and stays past its unmount",
    unmountedLine: "Unmounted %1",
    bridgeMissing: "Could not open %1 because part of Flea did not load. Reinstall Flea.",
    transferUsing: "%1 could not be unmounted. A transfer may still be using it."
}

// Sample input, captured live on the box with one network share mounted (2026-08-31):
// Drive(0): KBG40ZNS256G NVMe KIOXIA 256GB
//   Type: GProxyDrive (GProxyVolumeMonitorUDisks2)
// Mount(0): isos on 192.168.1.10 -> smb://192.168.1.10/isos/
//   Type: GDaemonMount
// Only Mount() lines matter here; Drive() and every indented "Type:" line are noise for the rail.
function parseMounts(output) {
    var out = []
    var mounts = mountLines(output)
    for (var i = 0; i < mounts.length; i++) {
        var uri = mounts[i].uri
        // corner: a local device mount (file://) is Favorites territory, not Network; Places.js skips the inverse.
        if (uri.indexOf("file://") === 0)
            continue
        // A phone is DEVICES territory the same way: its row is ui/js/Phones.js's, built from the
        // volume block, and this line is gio's shadow GDaemonMount printed beside it. afc is the
        // iPhone's own, whose root mount prints here rather than inside its volume block.
        if (/^(mtp|gphoto2|afc):\/\//i.test(uri))
            continue
        out.push({ label: Protocols.shareName(mounts[i].name, uri), uri: uri })
    }
    return out
}

// Sample input: "Mount(2): isos on 192.168.1.10 -> smb://192.168.1.10/isos/" answers { name: "isos on 192.168.1.10", uri: "smb://192.168.1.10/isos/" }.
// Every top-level "Mount(N): name -> uri" line of gio mount -l; the one owner of that line's grammar.
function mountLines(output) {
    var out = []
    var lines = String(output || "").split("\n")
    for (var i = 0; i < lines.length; i++) {
        var m = lines[i].match(/^Mount\(\d+\):\s*(.+?)\s*->\s*(\S+)\s*$/)
        if (m)
            out.push({ name: m[1], uri: m[2] })
    }
    return out
}

// Sample input: "Mount(0): photos -> file:///mnt/kmount-photos" answers { path: "/mnt/kmount-photos", label: "photos", uri: "file:///mnt/kmount-photos" }.
// Issue #257: what gio adds to a kernel row, matched by path; a non-file line adds nothing, an unlisted mount keeps its leaf label.
function gioKernelMounts(gioText) {
    var out = []
    var mounts = mountLines(gioText)
    for (var i = 0; i < mounts.length; i++) {
        var m = mounts[i]
        if (m.uri.indexOf("file://") !== 0)
            continue
        var path = decodePath(m.uri.substring("file://".length)).replace(/\/+$/, "")
        out.push({ path: path.length > 0 ? path : "/", label: Protocols.shareName(m.name, m.uri), uri: m.uri })
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

// Issue 133 (mfilm77): no GVFS backend implements trash, so gio trash refuses every path under its FUSE folder.
// Sample input: /run/user/1000/gvfs/smb-share:server=192.168.21.25,share=data/photos, where "smb-share:" names the mount.
// A read-only folder has no trash either: the listed line's own writability rides along, so iso9660
// and a root-owned stick root offer Delete permanently instead of a Trash row that only fails.
function trashable(path, writable) {
    if (writable === false)
        return false
    return !underGvfs(path)
}

// A path inside the GVFS FUSE root /run/user/<uid>/gvfs, where the "scheme:" segment names the mount; a folder named gvfs elsewhere is plain.
function underGvfs(path) {
    return /^\/run\/user\/[^/]+\/gvfs\/[A-Za-z0-9-]+:/.test(String(path || ""))
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

// NETWORK shows on its discoveries, DEVICES on those plus its own; the deadline shows whatever each has.
function railGroupsReady(networkDone, devicesDone, elapsedMs, deadlineMs) {
    if (elapsedMs >= deadlineMs)
        return { showNetwork: true, showDevices: true }
    var showNetwork = networkDone === true
    return { showNetwork: showNetwork, showDevices: showNetwork && devicesDone === true }
}

// A password may be given to any scheme here whose authority names a user.
function credentialed(uri) {
    return /^(smb|sftp|ftp|ftps|dav|davs):\/\/[^\/]*@/i.test(String(uri || ""))
}

// Try sftp keys before asking for a password.
function keyless(uri) {
    return /^sftp:\/\//i.test(String(uri || ""))
}

// PR #21: a live mount wins the rail row, and the operator's own bookmark label wins its name, or a
// rename typed on a mounted share is written to the file and never drawn. Matched the way rebuild dedups.
function railLabel(mount, marks) {
    for (var i = 0; i < marks.length; i++) {
        if (normalize(marks[i].uri) === normalize(mount.uri)) return marks[i].label
    }
    return mount.label
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
        && x.size === y.size && x.editable === y.editable && x.removable === y.removable
        && x.fstype === y.fstype && x.fsver === y.fsver && x.mediaRemovable === y.mediaRemovable && x.readOnly === y.readOnly
        && (x.locked || false) === (y.locked || false) && x.loop === y.loop
}

// Sample input: one rail entry as ui/DeviceMounts.qml and ui/NetworkMounts.qml build them,
// {label:"128GB", group:"device", kind:"volume", device:"/dev/sda1", mounted:true, removable:true}.
// Rows come from ui/js/Menu.js kind R; gio's -f is offered nowhere, since forcing an unmount over an open write loses data.
function railMenu(entry) {
    if (!entry)
        return []
    return Menu.railEntries(entry)
}

// A root-only remote mount covers its saved addressable paths; SMB shares remain path-specific.
function addressMountCovers(liveUri, savedUri) {
    var live = normalize(liveUri)
    var saved = normalize(savedUri)
    return /^(sftp|ftp|ftps|dav|davs):\/\/[^\/]+\/$/i.test(live)
        && saved.length > live.length && saved.indexOf(live) === 0
}

// The rail's right click opens railMenu's rows; ui/js/Eject.js reads railMenu too, so Ctrl+E refuses an unmounted row.
function rowMenu(entry) {
    return railMenu(entry)
}

// The handle a chosen menu row carries back: a volume's device node, a share's uri, "" for a row
// with no release. The rail rebuilds on a five second poll, so an index taken when the menu opened
// can name a different row by the time a row inside it is chosen; a key cannot.
function railKey(entry) {
    if (!entry)
        return ""
    if (entry.group === "device" && entry.kind === "volume")
        return String(entry.device || "")
    if (entry.group === "device" && entry.kind === "phone")
        return String(entry.uri || "")
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

// The device or network row with a release whose path holds the directory, or null; a favourite never names a volume.
function holding(entries, dir) {
    var list = entries || []
    var path = String(dir || "")
    for (var i = 0; i < list.length; i++) {
        var e = list[i]
        if (!e.path || (e.group !== "device" && e.group !== "network") || railMenu(e).length === 0)
            continue
        if (inside(path, e.path))
            return e
    }
    return null
}

// Whether dir is root itself or lies under it; ui/js/Places.js heldBy asks the rail the same question.
function inside(dir, root) {
    return dir === root || dir.indexOf(root + "/") === 0
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
        pane.message(Format.fill(WORDS.nothingToRelease, entry.label), false)
}

// Sample input: (true, "a", "a", "b", "b") answers "skip", while (true, "a", "x", "b", "b") answers "rebuild".
function pollDecision(listedOnce, polledListing, lastListing, mountinfoText, lastMountinfo) {
    if (!listedOnce) return "none"
    if (polledListing === lastListing && mountinfoText === lastMountinfo) return "skip"
    return "rebuild"
}

// Sample input: the operator's own bookmarks file, favourites and network places in one list.
// smb://192.168.1.10/isos NAS isos
// Read off the trimmed line the way nonFileBookmarks reads it, or an indented line is a rail row
// nothing removes, and normalized the way Places.relabel matches; a kept line is pushed back raw.
function removeBookmark(body, uri) {
    var target = normalize(uri)
    var lines = String(body || "").split("\n")
    var out = []
    for (var i = 0; i < lines.length; i++) {
        var trimmed = lines[i].trim(), space = trimmed.indexOf(" ")
        var one = space < 0 ? trimmed : trimmed.substring(0, space)
        if (target.length > 0 && one.length > 0 && normalize(one) === target)
            continue
        out.push(lines[i])
    }
    return out.join("\n")
}
