.pragma library

.import "Mounts.js" as Mounts

// Sample input: the lsblk --json body ui/js/Mounts.js "parseDevices" reads, taken again after
// gio mount -e returned for /dev/sda1:
// {"blockdevices":[
//   {"name":"sda","label":null,"mountpoint":null,"rm":true,"size":"116.1G","type":"disk","model":"USB Flash Disk",
//    "children":[{"name":"sda1","label":"128GB","mountpoint":null,"rm":true,"size":"116.1G","type":"part","model":null}]}]}
// The sda entry is then either gone, or still listed with every mountpoint under it null.
// gio's exit code is never the witness: it has been 0 over a volume that stayed mounted.
// "safe" when nothing under the disk that carries the device is mounted, "mounted" when anything
// under it still is, "unknown" when the body is not a listing this can judge.
function verdict(body, device) {
    var disk = diskOf(body, device)
    if (disk === undefined)
        return "unknown"
    if (disk === null)
        return "safe"
    return anyMounted(disk) ? "mounted" : "safe"
}

// The top-level lsblk node that holds the device: undefined when the body is not a listing, null
// when no node holds it, which is the powered-off shape. The whole disk is what gets judged: gvfs
// ejects the drive, so a sibling partition or a crypt child still mounted means still in use.
function diskOf(body, device) {
    var tree
    try {
        tree = JSON.parse(String(body || ""))
    } catch (e) {
        return undefined
    }
    var nodes = tree && tree.blockdevices
    // A box with no block devices at all is not running, so an empty listing is a broken one.
    if (!Array.isArray(nodes) || nodes.length === 0)
        return undefined
    var name = kernelName(device)
    if (name.length === 0)
        return undefined
    for (var i = 0; i < nodes.length; i++) {
        if (holds(nodes[i], name))
            return nodes[i]
    }
    return null
}

// Both sides of every comparison here come from lsblk's own NAME column, see Mounts.js "volumeRow".
function kernelName(device) {
    return String(device || "").replace(/^\/dev\//, "")
}

function holds(node, name) {
    if (String(node.name) === name)
        return true
    var kids = node.children || []
    for (var i = 0; i < kids.length; i++) {
        if (holds(kids[i], name))
            return true
    }
    return false
}

// lsblk writes "[SWAP]" as the mountpoint of active swap, which is in use like any other mount.
function anyMounted(node) {
    if (node.mountpoint)
        return true
    var kids = node.children || []
    for (var i = 0; i < kids.length; i++) {
        if (anyMounted(kids[i]))
            return true
    }
    return false
}

// What the refusal has to name. Empty when the row itself is still mounted, because a re-press on
// it is then the right next step; otherwise the labels of what is still mounted under the same disk,
// because a re-press on a row lsblk now shows unmounted would silently do nothing.
function blockers(body, device) {
    var disk = diskOf(body, device)
    if (!disk)
        return []
    var mounted = []
    collectMounted(disk, mounted)
    var name = kernelName(device)
    for (var i = 0; i < mounted.length; i++) {
        if (mounted[i].name === name)
            return []
    }
    return mounted.map(function (m) { return m.label })
}

function collectMounted(node, out) {
    if (node.mountpoint)
        out.push({ name: String(node.name), label: node.label ? String(node.label) : String(node.name) })
    var kids = node.children || []
    for (var i = 0; i < kids.length; i++)
        collectMounted(kids[i], out)
}

// "Safe to unplug" is a safety claim, so only the "safe" verdict earns it; every other line tells
// the user to leave the stick in, and says what to press next only when pressing it does something.
function sentence(v, label, others) {
    if (v === "safe")
        return { text: "Ejected " + label + ", it is safe to unplug.", isError: false }
    if (v === "mounted") {
        var rest = others || []
        if (rest.length === 0)
            return { text: label + " could not be ejected; it is still mounted, close anything using it and try again.", isError: true }
        var tail = rest.length > 1 ? " are still mounted, eject those instead." : " is still mounted, eject that instead."
        return { text: label + " could not be ejected; " + rest.join(", ") + " on the same drive" + tail, isError: true }
    }
    return { text: "Could not confirm " + label + " was ejected; do not unplug it yet.", isError: true }
}

// Finder's Cmd+E with Cmd read as Ctrl: from the rail it releases the cursor row, from a listing the
// removable volume the directory is inside, so the key can never release a volume the operator is
// not looking at. The verdict is Mounts.railMenu's either way, off the lsblk poll, and the release
// goes through the same releaseChosen a chosen menu row takes, carrying the row's key.
function release(root, sidebar, fromRail) {
    var entry = fromRail ? sidebar.entries[sidebar.cursorIndex] : Mounts.holding(sidebar.entries, root.path)
    if (!entry) {
        root.message("This directory is not inside a removable volume, so there is nothing to eject.", false)
        return
    }
    var action = Mounts.releaseAction(entry)
    if (!action) {
        root.message(entry.label + " has nothing to eject or unmount.", false)
        return
    }
    sidebar.releaseChosen(action, Mounts.railKey(entry))
}
