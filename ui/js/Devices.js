.pragma library

.import "Format.js" as Format
.import "Mounts.js" as Mounts

// The DEVICES half of the rail: one lsblk listing turned into the rows ui/DeviceMounts.qml draws.
// Split out of ui/js/Mounts.js, which keeps the rail's shared vocabulary and the NETWORK half; a
// block device and a gvfs share have nothing in common but the rail they land in.

// Sample lsblk --bytes --json row, with the columns ui/DeviceMounts.qml asks for:
// {"name":"sda1","path":"/dev/sda1","label":"128GB","mountpoints":["/run/media/gm/128GB"],"rm":true,"ro":false,
//  "tran":null,"size":124656812032,"type":"part","model":null,"fstype":"vfat","parttype":"0xb",
//  "pttype":"dos","partn":1,"uuid":"A1B2-C3D4"}.
// Two row kinds come out: one "disk" row for the disk that carries /, then one "volume" row for each
// volume on every disk. ui/DeviceMounts.qml turns these into rail entries.
function parseDevices(body, unmounted) {
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
        // Every disk walks the same way; hidden() hides the system disk's plumbing, and its disk row stands for the leaf holding /.
        if (!nodes[i].name || isPseudo(nodes[i]))
            continue
        // No transport to inherit at the top: the disk answers for itself inside the walk.
        collectVolumes([nodes[i]], "", false, out, unmounted === true, unpluggable(nodes[i]))
    }
    return dedupeVolumes(out)
}

// zram stays out; a loop device is browsable only while it or a child is mounted, despite rm=false.
function isPseudo(node) {
    var name = String(node.name || "")
    if (/^zram/.test(name))
        return true
    return /^loop/.test(name) && !holdsMount(node)
}

// Like holdsRoot, but a mount on the loop itself or any child keeps the image browsable.
function holdsMount(node) {
    if (mountOf(node).length > 0)
        return true
    var kids = node.children || []
    for (var k = 0; k < kids.length; k++) {
        if (holdsMount(kids[k]))
            return true
    }
    return false
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
function collectVolumes(nodes, model, unplugs, out, unmounted, removableDisk, holder, underLoop) {
    for (var i = 0; i < nodes.length; i++) {
        var n = nodes[i]
        var kids = n.children || []
        // Only the disk carries a product name, so it is passed down to its own partitions.
        var own = n.model ? String(n.model) : model
        // Only the disk carries the transport, so its partitions take its answer: measured by
        // mariobgsp (PR 74), whose USB drive reports tran=usb on sdb and null on sdb1. It stops
        // there: what a crypt leaf under it reads as is what it read as before that PR.
        var pulls = unpluggable(n) || (unplugs && String(n.type || "") === "part")
        // Loop ancestry routes eject through unmount and detach, and skips the optional unmounted-volume rule.
        var loopHere = underLoop || /^loop/.test(String(n.name || ""))
        // No row on any setting for a hidden leaf (ESP, swap, member) or for the leaf holding /, which its disk row stands for.
        if (n.name && (kids.length === 0 || mountOf(n).length > 0) && !hidden(n) && !holdsRoot(n)
                && (pulls || mountOf(n).length > 0 || (unmounted && browsable(n) && !loopHere) || isLockedNode(n)))
            out.push(volumeRow(n, own, pulls, removableDisk === true, holder, loopHere))
        collectVolumes(kids, own, pulls, out, unmounted, removableDisk === true || unpluggable(n), containerName(n), loopHere)
    }
}

// A leaf udisks hides is no row in any branch.
function hidden(n) {
    if (mountOf(n).length === 0 && String(n.fstype || "").length === 0)
        return true // H1: a partition with no filesystem and nothing mounted.
    var fs = String(n.fstype || "").toLowerCase()
    if (fs === "swap" || fs.slice(-7) === "_member" || fs === "bcache" || fs === "ceph" || fs === "drbd")
        return true // H2: swap, LVM and RAID members, and other non-filesystem types.
    if (/^(crypto_|bitlocker$)/.test(fs))
        return false // The Unlock board keeps a locked LUKS or BitLocker container as a row.
    if (isFirmwarePart(n) && !isIsohybridKeep(n))
        return true // H3: ESP, MSR, BIOS boot, WinRE GUID or MBR type, except an installer ISO part.
    if (isRecoveryLabel(n))
        return true // H4: System Reserved and vendor recovery labels on ntfs or vfat.
    if (isSystemMountOnly(n))
        return true // H5: mounted only where the system itself lives, never a user folder.
    return false
}

// GPT GUIDs and MBR numbers udisks hides; MBR compared numerically, lsblk drops the leading zero.
function isFirmwarePart(n) {
    var raw = String(n.parttype || "")
    if (raw.length === 0)
        return false
    var lower = raw.toLowerCase()
    if (lower.indexOf("-") >= 0)
        return gptHidden(lower)
    var num = parseInt(lower, 16)
    if (isNaN(num))
        return false // An unparsable type hides nothing rather than everything.
    return mbrHidden(num)
}

// Sample input: "c12a7328-f81f-11d2-ba4b-00a0c93ec93b" trues, "ebd0a0a2-b9e5-4433-87c0-68b6b72699c7" falses.
function gptHidden(lower) {
    var hide = ["c12a7328-f81f-11d2-ba4b-00a0c93ec93b", "21686148-6449-6e6f-744e-656564454649",
        "e3c9e316-0b5c-4db8-817d-f92df00215ae", "de94bba4-06d1-4d40-a16a-bfd50179d6ac",
        "bc13c2ff-59e6-4262-a352-b275fd6f7172", "426f6f74-0000-11aa-aa11-00306543ecac",
        "a19d880f-05fc-4d3b-a006-743f0f84911e", "e6d6d379-f507-44c2-a23c-238f2a3df928"]
    for (var i = 0; i < hide.length; i++)
        if (lower === hide[i])
            return true
    return false
}

// Sample input: 0x27 trues, 0x7 falses; 0x0 hides unless the isohybrid keep below answers first.
function mbrHidden(num) {
    var hide = [0x0, 0x5, 0xf, 0x85, 0x11, 0x12, 0x14, 0x16, 0x17, 0x1b, 0x1c, 0x1e, 0x27, 0x3d,
        0x84, 0x8d, 0x8e, 0x90, 0x91, 0x92, 0x93, 0x97, 0x98, 0x9a, 0x9b, 0xab, 0xbb, 0xc2, 0xc3, 0xdd, 0xef, 0xfd]
    for (var i = 0; i < hide.length; i++)
        if (num === hide[i])
            return true
    return false
}

// Sample input: dos with 0x0 part 1 iso9660 keeps, the same part as vfat hides; udisks isohybrid undo.
function isIsohybridKeep(n) {
    if (String(n.pttype || "").toLowerCase() !== "dos")
        return false
    var num = parseInt(String(n.parttype || ""), 16)
    if (!(num === 0x0 || num === 0x17))
        return false
    var partn = String(n.partn === null || n.partn === undefined ? "" : n.partn)
    if (!(partn === "1" || partn === "2"))
        return false
    var fs = String(n.fstype || "").toLowerCase()
    return fs === "iso9660" || fs === "udf"
}

// Sample input: ntfs "System Reserved" trues, ext4 "System Reserved" falses; spaces read as udev writes them.
function isRecoveryLabel(n) {
    var fs = String(n.fstype || "").toLowerCase()
    if (!(fs === "ntfs" || fs === "vfat"))
        return false
    var label = String(n.label || "").replace(/ /g, "_")
    var hide = ["Recovery", "RECOVERY", "Lenovo_Recovery", "HP_RECOVERY", "Recovery_Partition",
        "DellUtility", "DellRestore", "IBM_SERVICE", "SERVICEV001", "SERVICEV002",
        "SYSTEM_RESERVED", "System_Reserved", "WINRE_DRV", "DIAGS", "IntelRST"]
    for (var i = 0; i < hide.length; i++)
        if (label === hide[i])
            return true
    return false
}

// Sample input: ["/"] trues, ["/mnt/data"] falses; every mountpoint must be system for the row to hide.
function isSystemMountOnly(n) {
    var points = []
    var raw = n.mountpoints || []
    for (var i = 0; i < raw.length; i++) {
        var p = raw[i] === null ? "" : String(raw[i])
        if (p.length > 0 && p.charAt(0) === "/")
            points.push(p)
    }
    if (points.length === 0)
        return false
    for (var j = 0; j < points.length; j++)
        if (!isSystemPath(points[j]))
            return false
    return true
}

// Sample input: "/boot" trues, "/run" trues, "/home/gm/Data" falses; exact glib list, prefix only under /dev /proc /sys.
function isSystemPath(p) {
    if (p === "/" || p.indexOf("/.") >= 0)
        return true
    if (p === "/dev" || p === "/proc" || p === "/sys")
        return true
    if (p.indexOf("/dev/") === 0 || p.indexOf("/proc/") === 0 || p.indexOf("/sys/") === 0)
        return true
    var exact = ["/bin", "/boot", "/compat/linux/proc", "/compat/linux/sys", "/etc", "/home",
        "/lib", "/lib64", "/libexec", "/live/cow", "/live/image", "/media", "/mnt", "/net",
        "/opt", "/rescue", "/root", "/run", "/sbin", "/srv", "/tmp", "/usr", "/usr/X11R6", "/usr/local",
        "/usr/obj", "/usr/ports", "/usr/src", "/usr/xobj", "/var", "/var/crash", "/var/local",
        "/var/log", "/var/log/audit", "/var/mail", "/var/run", "/var/tmp"]
    for (var i = 0; i < exact.length; i++)
        if (p === exact[i])
            return true
    return false
}

// One row per device path and per btrfs UUID: an md repeat lists one path twice, multi-device btrfs shares one.
function dedupeVolumes(rows) {
    var head = []
    var byUuid = {}
    var at = {}
    for (var i = 0; i < rows.length; i++) {
        var r = rows[i]
        var dkey = String(r.device || "")
        var ukey = String(r.uuid || "")
        var dup = -1
        if (dkey.length > 0 && at[dkey] !== undefined)
            dup = at[dkey]
        else if (ukey.length > 0 && byUuid[ukey] !== undefined)
            dup = byUuid[ukey]
        if (dup < 0) {
            at[dkey] = head.length
            if (ukey.length > 0)
                byUuid[ukey] = head.length
            head.push(r)
        } else if (r.mounted === true && head[dup].mounted !== true) {
            at[dkey] = dup
            if (ukey.length > 0)
                byUuid[ukey] = dup
            head[dup] = r
        }
    }
    return head
}

// Sample input: {"name":"sdb1","mountpoints":[null],"type":"part","fstype":"crypto_LUKS","fsver":"2"}, a locked leaf.
function isLockedNode(n) {
    if (!n || (n.children || []).length > 0)
        return false
    if (mountOf(n).length > 0)
        return false
    return /^(crypto_|bitlocker$)/.test(String(n.fstype || "").toLowerCase())
}

// RailAdditions rule 1 gate (old lsblk without PARTTYPE): empty-fs and swap die in hidden() ahead; only crypto and efi decide here.
function browsable(n) {
    var fs = String(n.fstype || "").toLowerCase()
    if (/^(crypto_|bitlocker$)/.test(fs))
        return false
    return String(n.parttypename || "").toLowerCase().indexOf("efi") < 0
}

// A drive somebody can pull out, which is what Eject is for. RM alone is not the answer: a USB
// bridge (a WD My Passport, PR 74) reports rm=false and is still a drive you unplug, while a hot-swap
// SATA bay reports its own hotplug and is not, which is why the transport decides and not that.
function unpluggable(n) {
    return !!n && (n.rm === true || String(n.tran || "").toLowerCase() === "usb")
}

// The mount wait starts when gio exits 0, never at launch: an admin polkit prompt longer than the
// wait is a slow prompt and not a mount that never reported a folder to open.
function mountTimerStart(exitCode) {
    return exitCode === 0
}

// The disk a partition powers off through, so a USB bridge stops instead of re-announcing.
// Sample input: "/dev/sda1" answers "/dev/sda", "/dev/nvme0n1p2" answers "/dev/nvme0n1.
function powerOffDisk(device) {
    var text = String(device || "")
    var m = text.match(/^(\/dev\/(?:sd[a-z]+|hd[a-z]+|vd[a-z]+|nvme\d+n\d+|mmcblk\d+|loop\d+))p?\d+$/)
    if (m)
        return m[1]
    var bare = text.match(/^\/dev\/(?:sd[a-z]+|hd[a-z]+|vd[a-z]+|nvme\d+n\d+|mmcblk\d+|loop\d+)$/)
    return bare ? text : ""
}

// With another partition mounted, loop-delete only marks autoclear and that mount keeps reading (measured in 0.3.10).
// Sample input: {device: "/dev/loop0p1", path: "/run/media/u/ISO", loop: true} answers the sh chain on /dev/loop0.
var detachFailedExit = 3
function ejectCommand(e) {
    var path = String(e.path || "")
    if (e.loop !== true)
        return ["gio", "mount", "-e", path]
    var loop = String(e.device || "").match(/^\/dev\/(loop\d+)(?:p\d+)?$/)
    if (!loop)
        return ["gio", "mount", "-u", path]
    var loopDisk = "/dev/" + loop[1]
    return ["sh", "-c", 'autoclear=$(cat -- "$4" 2>/dev/null) || autoclear=; before=$(cat -- "$3" 2>/dev/null); '
            + 'status=0; gio mount -u "$1" || status=1; if [ "$autoclear" != 0 ]; then exit "$status"; fi; ' // Autoclear 1 detaches at the kernel's last close; unreadable values are left alone.
            + 'after=$(cat -- "$3" 2>/dev/null); if [ -n "$before" ] && [ "$after" = "$before" ]; then '
            + 'udisksctl loop-delete --no-user-interaction -b "$2" || exit ' + detachFailedExit + '; fi; exit "$status"',
            "sh", path, loopDisk, "/sys/block/" + loop[1] + "/loop/backing_file", "/sys/block/" + loop[1] + "/loop/autoclear"]
}

// The sysfs block name for a disk path, so the eject chain can watch its write counter.
// Sample input: "/dev/sda" answers "sda", "/dev/nvme0n1" answers "nvme0n1.
function sysBase(disk) {
    var text = String(disk || "")
    var m = text.match(/^\/dev\/(.+)$/)
    return m ? m[1] : ""
}

// The written-sector counter out of one /sys/block/<disk>/stat read, "" when unreadable.
// Sample input: "   1 0 2 3 0 0 42 0 0 0 0 0 0 0 0" answers "42".
function writtenSectors(body) {
    var fields = String(body || "").trim().split(/\s+/)
    return fields.length >= 7 ? fields[6] : ""
}

// Every mounted node under one disk in the last listing, hidden ones too (an ESP has no rail row, yet a power-off must unmount it); system mounts never queue.
// Sample input: sda1 (ESP) mounted at /run/media/u/EFI answers [{device: "/dev/sda1", path: "/run/media/u/EFI"}] for "/dev/sda".
function powerOffQueue(listing, disk, entries) {
    var tree
    var out = []
    try {
        tree = JSON.parse(String(listing || ""))
    } catch (e) {
        // An unparsable listing keeps the mounted rail volumes queued, as before the hidden-node walk.
        console.warn("Device listing for " + disk + " could not be parsed (" + e.message + "); mounted rail volumes were queued instead.")
        var rows = entries || []
        for (var r = 0; r < rows.length; r++) {
            var row = rows[r]
            if (row.kind === "volume" && row.mounted && powerOffDisk(row.device) === disk)
                out.push({ device: row.device, path: row.path })
        }
        return dedupeVolumes(out)
    }
    var nodes = (tree && tree.blockdevices) || []
    for (var i = 0; i < nodes.length; i++) {
        if (nodes[i].name && powerOffDisk(devicePath(nodes[i])) === disk)
            collectMounted(nodes[i], out)
    }
    return dedupeVolumes(out)
}

function collectMounted(node, out) {
    var point = mountOf(node)
    if (point.length > 0 && !holdsRoot(node) && !isSystemMountOnly(node))
        out.push({ device: devicePath(node), path: point })
    var kids = node.children || []
    for (var i = 0; i < kids.length; i++)
        collectMounted(kids[i], out)
}

// Sample gio stderr: "Error 1: target is busy", "Not authorized to perform operation",
// "No volume for device /dev/sda9", "Location is already mounted", "No medium found".
// "" opens rather than failing: the volume is already mounted, so there is no failure to say.
function mountError(op, exitCode, stderr, label) {
    if (exitCode === 0)
        return ""
    var text = String(stderr || "").toLowerCase()
    var name = String(label || "")
    if (/already mounted/.test(text))
        return ""
    if (/busy|in use|in use by/.test(text))
        return Format.fill(Mounts.WORDS.busy, name)
    if (/not authorized|not allowed|refused|dismissed|no authentication|auth/i.test(String(stderr || "")))
        return Format.fill(op === "unmount" ? Mounts.WORDS.unmountRefused : op === "eject" ? Mounts.WORDS.ejectRefused : Mounts.WORDS.mountRefused, name)
    if (/no volume|no medium|no media|empty|no device/.test(text))
        return Format.fill(/medium|media|empty/.test(text) ? Mounts.WORDS.noDisc : Mounts.WORDS.notMountable, name)
    if (op === "mount" && /wrong fs type|bad superblock|unknown filesystem type|can.t read superblock|structure needs cleaning/.test(text))
        return Format.fill(Mounts.WORDS.damaged, name)
    if (op === "unmount")
        return Format.fill(Mounts.WORDS.unmountFailed, name)
    if (op === "eject")
        return Format.fill(Mounts.WORDS.ejectFailed, name)
    return Format.fill(Mounts.WORDS.mountFailed, name)
}

// gio's own reason as the status bar's detail line; sample "Error mounting /dev/sdb1 at /run/media/u/DATA #2: wrong fs type" and "Error unmounting /dev/sdb1: target is busy" give "wrong fs type" and "target is busy", "gio: file:///x: Error ejecting /dev/sdb: busy" gives "busy" (gio's file-URI lead), "" gives "".
function mountDetail(stderr) {
    var lines = String(stderr || "").split("\n").map(function (line) { return line.trim() }).filter(function (line) { return line.length > 0 })
    return lines.join(" ").replace(/^gio:\s*/, "").replace(/^[a-z][a-z0-9+.-]*:\/\/\S*:\s+/i, "").replace(/^Error (?:mounting|unmounting|ejecting) \S+(?: at .*?)?:\s+/, "").trim()
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

// Sample input: {"name":"sdb1","label":"Backup","fstype":"crypto_LUKS"}, a container its owner named; "" for any other node.
function containerName(n) {
    return /^(crypto_|bitlocker$)/.test(String(n.fstype || "").toLowerCase()) && n.label ? String(n.label) : ""
}

// Labelled by the filesystem label, then the drive's product name, then the kernel name; a crypt keeps its named container's.
function volumeRow(n, model, unplugs, removableDisk, holder, loop) {
    var path = mountOf(n)
    var named = String(n.type || "") === "crypt" ? String(holder || "") : ""
    var label = named.length > 0 ? named : n.label ? String(n.label) : (model.length > 0 ? model : String(n.name))
    var fs = String(n.fstype || "").toLowerCase()
    // A crypt leaf on a removable drive releases like the stick it sits on; an internal one never does.
    var removable = unplugs === true || (String(n.type || "") === "crypt" && removableDisk === true)
    return { kind: "volume", label: label, device: devicePath(n), path: path, mounted: path.length > 0,
             removable: removable, mediaRemovable: n.rm === true, readOnly: n.ro === true, size: deviceBytes(n.size),
             locked: isLockedNode(n), fstype: String(n.fstype || ""), fsver: String(n.fsver || ""),
             uuid: fs === "btrfs" && n.uuid ? String(n.uuid) : "", loop: !!loop }
}

// An unavailable or malformed capacity stays absent; only the delegate formats valid byte counts.
function deviceBytes(value) {
    return Number.isSafeInteger(value) && value >= 0 ? value : null
}
