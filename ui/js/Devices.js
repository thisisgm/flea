.pragma library

// The DEVICES half of the rail: one lsblk listing turned into the rows ui/DeviceMounts.qml draws.
// Split out of ui/js/Mounts.js, which keeps the rail's shared vocabulary and the NETWORK half; a
// block device and a gvfs share have nothing in common but the rail they land in.

// Sample lsblk --bytes --json row, with the columns ui/DeviceMounts.qml asks for:
// {"name":"sda1","path":"/dev/sda1","label":"128GB","mountpoints":["/run/media/gm/128GB"],"rm":true,
//  "size":124656812032,"type":"part","model":null}.
// Two row kinds come out: one "disk" row for the disk that carries /, then one "volume" row for each
// volume on every other disk. ui/DeviceMounts.qml turns these into rail entries.
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
    var system = systemDisk(nodes)
    if (system)
        out.push({ kind: "disk", label: String(system.name), device: devicePath(system), path: "/",
                   mounted: true, removable: false, size: deviceBytes(system.size) })
    for (var i = 0; i < nodes.length; i++) {
        // The system disk is never walked: /boot and a separate home are the box's own plumbing and
        // the row above already stands for that disk. Everything else on the box is walked, which is
        // what puts a second internal drive in the rail (operator, 2026-09-11: only sticks appeared).
        if (!nodes[i].name || nodes[i] === system || isPseudo(nodes[i].name))
            continue
        collectVolumes([nodes[i]], "", out)
    }
    return out
}

// zram and loop devices are type "disk" too, and neither is a disk anyone browses.
function isPseudo(name) {
    return /^(zram|loop)/.test(String(name))
}

// The disk that actually carries /, found through the mountpoints of its own subtree. It used to be
// guessed as the first non-removable disk, and with a second internal drive present that guess names
// whichever disk lsblk lists first: the wrong drive was then labelled with the hostname and pointed
// at /, while the real system disk had no row at all.
function systemDisk(nodes) {
    for (var i = 0; i < nodes.length; i++) {
        if (nodes[i].name && holdsRoot(nodes[i]))
            return nodes[i]
    }
    return null
}

function holdsRoot(node) {
    var points = node.mountpoints || []
    for (var i = 0; i < points.length; i++) {
        if (points[i] === "/")
            return true
    }
    var kids = node.children || []
    for (var k = 0; k < kids.length; k++) {
        if (holdsRoot(kids[k]))
            return true
    }
    return false
}

// A volume earns a rail row when it is removable, which is a stick whether or not anything mounted
// it, or when it is mounted, which is every internal drive the operator actually uses. An unmounted
// internal partition stays out: a box's spare EFI and recovery partitions are not places to browse.
function collectVolumes(nodes, model, out) {
    for (var i = 0; i < nodes.length; i++) {
        var n = nodes[i]
        var kids = n.children || []
        // Only the disk carries a product name, so it is passed down to its own partitions.
        var own = n.model ? String(n.model) : model
        // Only a leaf is a volume. A partition holding a LUKS container is not what mounts, its crypt
        // child is, and emitting both would put one drive in the rail twice.
        if (n.name && kids.length === 0 && (n.rm === true || mountOf(n).length > 0))
            out.push(volumeRow(n, own))
        collectVolumes(kids, own, out)
    }
}

// Sample: ["/home", "/var/log", "/"] for one btrfs device with several subvolumes mounted, ["[SWAP]"]
// for swap, which is no directory, and [null] for a volume nothing has mounted. The first real path
// wins, which is also what drops swap: only a mountpoint is browsable and only one row is drawn.
function mountOf(node) {
    var points = node.mountpoints || []
    for (var i = 0; i < points.length; i++) {
        var point = points[i] === null ? "" : String(points[i])
        if (point.length > 0 && point.charAt(0) === "/")
            return point
    }
    return ""
}

// lsblk's own PATH column, because a device-mapper leaf lives at /dev/mapper/<name> and "/dev/" plus
// its kernel name is a path that does not exist. gio is handed this, so it has to be the real one.
function devicePath(node) {
    return node.path ? String(node.path) : "/dev/" + String(node.name)
}

// The label ladder is the filesystem label, then the drive's product name, then the kernel name.
function volumeRow(n, model) {
    var path = mountOf(n)
    var label = n.label ? String(n.label) : (model.length > 0 ? model : String(n.name))
    return { kind: "volume", label: label, device: devicePath(n), path: path, mounted: path.length > 0,
             removable: n.rm === true, size: deviceBytes(n.size) }
}

// An unavailable or malformed capacity stays absent; only the delegate formats valid byte counts.
function deviceBytes(value) {
    return Number.isSafeInteger(value) && value >= 0 ? value : null
}
