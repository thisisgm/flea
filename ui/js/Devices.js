.pragma library

// The DEVICES half of the rail: one lsblk listing turned into the rows ui/DeviceMounts.qml draws.
// Split out of ui/js/Mounts.js, which keeps the rail's shared vocabulary and the NETWORK half; a
// block device and a gvfs share have nothing in common but the rail they land in.

// Sample lsblk --bytes --json row, with the columns ui/DeviceMounts.qml asks for:
// {"name":"sda1","path":"/dev/sda1","label":"128GB","mountpoints":["/run/media/gm/128GB"],"rm":true,
//  "tran":null,"size":124656812032,"type":"part","model":null}.
// Two row kinds come out: one "disk" row for the disk that carries /, then one "volume" row for each
// volume on every other disk, plus, with systemPartitions on, one more for every sibling partition of
// the system disk itself (a second OS, a Windows volume). ui/DeviceMounts.qml turns these into rail entries.
function parseDevices(body, unmounted, systemPartitions) {
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
    if (system && systemPartitions === true)
        collectSystemPartitions(system.children || [], out)
    for (var i = 0; i < nodes.length; i++) {
        // The system disk is walked separately, above, and only with the switch on: /boot and a
        // separate home are the box's own plumbing and the row above already stands for that disk.
        // Everything else on the box is walked here, which is what puts a second internal drive in
        // the rail (operator, 2026-09-11: only sticks appeared).
        if (!nodes[i].name || nodes[i] === system || isPseudo(nodes[i].name))
            continue
        // No transport to inherit at the top: the disk answers for itself inside the walk.
        collectVolumes([nodes[i]], "", false, out, unmounted === true)
    }
    return out
}

// places.showSystemPartitions's own walk: every leaf partition of the system disk earns a row,
// mounted or not, the way Nautilus lists them, except the one that actually holds / (the disk row
// above already opens it) and the box's own plumbing (see systemPlumbing). Nothing pulled from here
// is removable, and every row carries volumeMenu so it can be mounted from the rail like any other
// unmounted volume.
function collectSystemPartitions(nodes, out) {
    for (var i = 0; i < nodes.length; i++) {
        var n = nodes[i]
        var kids = n.children || []
        if (kids.length > 0) {
            collectSystemPartitions(kids, out)
            continue
        }
        if (n.name && !holdsRoot(n) && !systemPlumbing(n))
            out.push(systemPartitionRow(n))
    }
}

// The box's own plumbing on the system disk, which Nautilus does not offer as a place to browse
// either: an EFI system partition, a Linux extended boot partition (systemd-boot's ESP mirror), a
// Microsoft reserved partition, a recovery partition, a BIOS boot partition, and whatever is actually
// mounted as the box's own boot area, read off the mountpoint rather than guessed from a label.
function systemPlumbing(n) {
    var type = String(n.parttypename || "").toLowerCase()
    if (/efi|extended boot|reserved|recovery|bios boot/.test(type))
        return true
    var point = mountOf(n)
    return point === "/boot" || point === "/efi" || point === "/boot/efi"
}

// A system-disk sibling is never a drive anyone unplugs, and it always earns the fuller rail menu
// (Mount, Open, Unmount), since it may not be mounted yet.
function systemPartitionRow(n) {
    var path = mountOf(n)
    var label = n.label ? String(n.label) : (sizeName(n.size) || String(n.name))
    return { kind: "volume", label: label, device: devicePath(n), path: path, mounted: path.length > 0,
             removable: false, size: deviceBytes(n.size), volumeMenu: true }
}

// Nautilus's own name for an unlabeled volume, "53.5 GB Volume", in decimal units.
function sizeName(bytes) {
    if (!Number.isSafeInteger(bytes) || bytes <= 0)
        return ""
    var units = ["kB", "MB", "GB", "TB"]
    var value = bytes / 1000
    var u = 0
    while (value >= 1000 && u < units.length - 1) {
        value /= 1000
        u++
    }
    return value.toFixed(1) + " " + units[u] + " Volume"
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

// A volume earns a rail row when it can be unplugged, which is a stick whether or not anything
// mounted it, or when it is mounted, which is every internal drive the operator actually uses.
// RailAdditions rule 1 adds the third case behind its own switch: a volume nothing has mounted, with
// a filesystem to browse. With the switch off this is the 0.2.1 rule exactly, which kept a spare EFI
// or recovery partition out of the rail.
function collectVolumes(nodes, model, unplugs, out, unmounted) {
    for (var i = 0; i < nodes.length; i++) {
        var n = nodes[i]
        var kids = n.children || []
        // Only the disk carries a product name, so it is passed down to its own partitions.
        var own = n.model ? String(n.model) : model
        // Only the disk carries the transport, so its partitions take its answer: measured by
        // mariobgsp (PR 74), whose USB drive reports tran=usb on sdb and null on sdb1. It stops
        // there: what a crypt leaf under it reads as is what it read as before that PR.
        var pulls = unpluggable(n) || (unplugs && String(n.type || "") === "part")
        // Only a leaf is a volume. A partition holding a LUKS container is not what mounts, its crypt
        // child is, and emitting both would put one drive in the rail twice.
        var emptyOptical = String(n.type || "").toLowerCase() === "rom"
                        && mountOf(n).length === 0 && String(n.fstype || "").length === 0
        if (n.name && kids.length === 0 && !emptyOptical
                && (pulls || mountOf(n).length > 0 || (unmounted && browsable(n))))
            out.push(volumeRow(n, own, pulls, unmounted))
        collectVolumes(kids, own, pulls, out, unmounted)
    }
}

// RailAdditions rule 1's three exceptions, read off lsblk's own columns rather than guessed from a
// name: swap is not a place to browse, the EFI system partition is the box's own plumbing, and a
// volume with no filesystem has nothing to mount. A locked LUKS container is that last case: what
// mounts is its crypt child, which lsblk reports as a child of its own once it is unlocked.
function browsable(n) {
    var fs = String(n.fstype || "").toLowerCase()
    if (fs.length === 0 || fs === "swap" || fs.indexOf("crypto_") === 0)
        return false
    return String(n.parttypename || "").toLowerCase().indexOf("efi") < 0
}

// A drive somebody can pull out, which is what Eject is for. RM alone is not the answer: a USB
// bridge (a WD My Passport, PR 74) reports rm=false and is still a drive you unplug, while a hot-swap
// SATA bay reports its own hotplug and is not, which is why the transport decides and not that.
function unpluggable(n) {
    return !!n && (n.rm === true || String(n.tran || "").toLowerCase() === "usb")
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
// volumeMenu says the row was built under RailAdditions rule 1, which is what gives it the Mount,
// Open and Unmount rows; without the switch the row carries the menu it carried in 0.2.1.
function volumeRow(n, model, unplugs, unmounted) {
    var path = mountOf(n)
    var label = n.label ? String(n.label) : (model.length > 0 ? model : String(n.name))
    return { kind: "volume", label: label, device: devicePath(n), path: path, mounted: path.length > 0,
             removable: unplugs === true, size: deviceBytes(n.size), volumeMenu: unmounted === true }
}

// An unavailable or malformed capacity stays absent; only the delegate formats valid byte counts.
function deviceBytes(value) {
    return Number.isSafeInteger(value) && value >= 0 ? value : null
}
