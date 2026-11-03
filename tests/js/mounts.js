.import "../../ui/js/Devices.js" as Devices
.import "../../ui/js/Mounts.js" as Mounts
.import "../../ui/js/Cloud.js" as Cloud
.import "../../ui/js/RailMenu.js" as RailMenu
.import "../../ui/js/Eject.js" as Eject
.import "../../ui/js/Icons.js" as Icons

function run(check) {
    // Real gio mount -l output, captured on the box with the isos share mounted (2026-08-31).
    var live = 'Drive(0): KBG40ZNS256G NVMe KIOXIA 256GB\n'
             + '  Type: GProxyDrive (GProxyVolumeMonitorUDisks2)\n'
             + 'Mount(0): isos on 192.168.1.10 -> smb://192.168.1.10/isos/\n'
             + '  Type: GDaemonMount\n'
    var mounts = Mounts.parseMounts(live)
    check("a Drive() line is not a mount", mounts.length, 1)
    check("the on-host suffix is stripped from the label", mounts[0].label, "isos")
    check("the target uri survives whole", mounts[0].uri, "smb://192.168.1.10/isos/")

    var driveOnly = 'Drive(0): KBG40ZNS256G NVMe KIOXIA 256GB\n'
                   + '  Type: GProxyDrive (GProxyVolumeMonitorUDisks2)\n'
    check("no Mount() line means no entries", Mounts.parseMounts(driveOnly).length, 0)
    check("empty gio output parses to nothing", Mounts.parseMounts("").length, 0)
    check("garbage gio output parses to nothing", Mounts.parseMounts("not gio output at all\n").length, 0)

    // A local device mount uses file://, which is Favorites territory and not Network.
    var local = 'Mount(1): 32GB USB Drive -> file:///run/media/gm/32GB%20USB%20Drive/\n'
    check("a file:// mount is excluded", Mounts.parseMounts(local).length, 0)

    var multi = 'Mount(0): isos on 192.168.1.10 -> smb://192.168.1.10/isos/\n'
              + 'Mount(1): data on 192.168.1.10 -> smb://192.168.1.10/data/\n'
    check("two Mount() lines parse to two entries", Mounts.parseMounts(multi).length, 2)
    check("the second entry's label is its own", Mounts.parseMounts(multi)[1].label, "data")

    // The operator's real bookmarks file, ui/js/Places.js "bookmarks" reads the same lines the other way.
    var marks = 'file:///home/gm/Downloads Downloads\n'
              + 'file:///home/gm/Projects Projects\n'
              + 'file:///home/gm/Pictures Pictures\n'
              + 'file:///home/gm/Videos Videos\n'
              + 'smb://192.168.1.10/ NAS\n'
    var b = Mounts.nonFileBookmarks(marks)
    check("only the non-file bookmark survives", b.length, 1)
    check("the uri is the bare server root", b[0].uri, "smb://192.168.1.10/")
    check("the trailing label wins", b[0].label, "NAS")

    check("empty bookmarks parses to nothing", Mounts.nonFileBookmarks("").length, 0)
    check("garbage bookmarks parses to nothing", Mounts.nonFileBookmarks("not a bookmarks file\n").length, 0)

    var noLabel = 'smb://192.168.1.10/isos/\n'
    check("a bookmark with no label falls back to the share leaf", Mounts.nonFileBookmarks(noLabel)[0].label, "isos")

    var bareRoot = 'smb://192.168.1.10/\n'
    check("a bare server root with no label falls back to the host", Mounts.nonFileBookmarks(bareRoot)[0].label, "192.168.1.10")

    // Item 4: one canonical form, so a share dedupes against itself regardless of who typed the slash.
    check("a share uri normalizes its trailing slash away", Mounts.normalize("smb://h/data/"), "smb://h/data")
    check("a share uri with no trailing slash is already canonical", Mounts.normalize("smb://h/data"), "smb://h/data")
    check("smb://h/data and smb://h/data/ cannot coexist as two rows", Mounts.normalize("smb://h/data") === Mounts.normalize("smb://h/data/"), true)
    check("a bare server root keeps its one trailing slash", Mounts.normalize("smb://h/"), "smb://h/")
    check("a bare server root typed with no slash still canonicalizes to one", Mounts.normalize("smb://h"), "smb://h/")
    check("two different shares stay distinct after normalizing", Mounts.normalize("smb://h/data/") === Mounts.normalize("smb://h/other/"), false)

    // The rail's own menu, from ui/js/Menu.js INVENTORY kind R: what the rail already tagged decides the rows.
    var volume = { label: "128GB", group: "device", kind: "volume", device: "/dev/sda1", mounted: true, removable: true }
    var idle = { label: "128GB", group: "device", kind: "volume", device: "/dev/sda1", mounted: false, removable: true }
    var internal = { label: "nvme0n1", group: "device", kind: "disk", device: "/dev/nvme0n1", mounted: true, removable: false }
    var fixed = { label: "Vault", group: "device", kind: "volume", device: "/dev/sdb1", mounted: true, removable: false }
    var share = { label: "isos", group: "network", kind: "share", uri: "smb://example.com/isos/", mounted: true, saved: true }
    var bookmark = { label: "NAS", group: "network", kind: "share", uri: "smb://example.com/", mounted: false }
    var dropbox = { label: "Dropbox", group: "network", kind: "dropbox", uri: "", mounted: true }
    var favourite = { label: "Home", group: "favourite", kind: "favourite", path: "/home/user" }
    // A mounted loop device (an opened .iso): never removable at the kernel level, same as `fixed`,
    // but unlike a bolted-in disk it exists only because the user attached it, so it still offers
    // Eject -- DeviceMounts.qml's eject() already knows to route it through gio mount -u.
    var loopRow = { label: "OMARCHY_202608", group: "device", kind: "volume", device: "/dev/loop0p1", mounted: true, removable: false, loop: true }
    function solid(rows) { return rows.filter(function (r) { return r.separator !== true }) }
    function labelsOf(rows) { return solid(rows).map(function (r) { return r.label }).join(",") }
    function actionsOf(rows) { return solid(rows).map(function (r) { return r.action }).join(",") }
    function glyphsOf(rows) { return solid(rows).map(function (r) { return r.glyph }).join(",") }

    check("a mounted removable volume opens beside its releases", labelsOf(Mounts.railMenu(volume)), "Open,Unmount,Eject,Flash image")
    check("Open runs the row's own activation", solid(Mounts.railMenu(volume))[0].action, "open")
    check("Open takes the listing menu's own mark", solid(Mounts.railMenu(volume))[0].glyph, "folder-open")
    check("the Unmount row carries the unmount action", solid(Mounts.railMenu(volume))[1].action, "unmountVolume")
    check("the Unmount row draws the drive mark", solid(Mounts.railMenu(volume))[1].glyph, "drive")
    check("the Eject row carries the eject action", solid(Mounts.railMenu(volume))[2].action, "eject")
    check("the Eject row draws the eject mark", solid(Mounts.railMenu(volume))[2].glyph, "eject")
    check("a mounted loop device opens beside its releases despite removable being false", labelsOf(Mounts.railMenu(loopRow)), "Open,Unmount,Eject")
    check("the loop row's Eject carries the eject action", solid(Mounts.railMenu(loopRow))[2].action, "eject")
    check("a group change draws the listing menu's own separator", Mounts.railMenu(volume).length, 6)
    check("a mounted network share opens beside its release", labelsOf(Mounts.railMenu(share)), "Open,Unmount,Rename,Edit address,Remove from Network")
    check("its Open is the listing menu's own row too", actionsOf(Mounts.railMenu(share)), "open,unmount,rename,editPlace,remove")
    check("Mount and Unmount share the drive mark, Eject keeps its own", glyphsOf(Mounts.railMenu(share)), "folder-open,drive,rename,sliders,minus")

    // Rows never offered a release, each for its own reason; an unmounted place keeps its own three rows.
    check("the internal disk offers nothing, it is the box's own system disk", Mounts.railMenu(internal).length, 0)
    check("a fixed internal volume offers Open and Unmount, only Eject stays removable-only", labelsOf(Mounts.railMenu(fixed)), "Open,Unmount")
    check("the Dropbox row offers nothing, it is a local folder the stock service owns", Mounts.railMenu(dropbox).length, 0)
    check("a favourite offers INVENTORY's own Remove from Favorites", labelsOf(Mounts.railMenu(favourite)), "Remove from Favorites")
    check("an unmounted removable volume offers the Mount its activation does, and Flash image",
          labelsOf(Mounts.railMenu(idle)), "Mount,Flash image")
    check("a bookmark nothing has mounted offers no release, only its own rows",
          actionsOf(Mounts.railMenu(bookmark)), "rename,editPlace,remove")
    check("no entry at all offers nothing rather than throwing", Mounts.railMenu(null).length, 0)
    check("an undefined entry offers nothing rather than throwing", Mounts.railMenu(undefined).length, 0)

    // The safety property as one check: eject reaches exactly one kind of row and no other.
    var never = [idle, internal, fixed, dropbox, favourite, bookmark, share, null, undefined]
    check("no row but a mounted removable volume is ever offered eject",
          never.some(function (e) {
              return Mounts.railMenu(e).some(function (r) { return r.action === "eject" })
          }), false)
    check("a mounted loop device is the one other row eject reaches",
          Mounts.railMenu(loopRow).some(function (r) { return r.action === "eject" }), true)

    // The key a chosen row carries back: the rail rebuilds on a five second poll, so an index taken
    // when the menu opened can name a different row by the time a row inside it is chosen.
    check("a volume's key is its device node", Mounts.railKey(volume), "/dev/sda1")
    check("a share's key is its uri", Mounts.railKey(share), "smb://example.com/isos/")
    check("the internal disk has no key", Mounts.railKey(internal), "")
    check("the Dropbox row has no key", Mounts.railKey(dropbox), "")
    check("a favourite has no key", Mounts.railKey(favourite), "")
    check("no entry at all has no key rather than throwing", Mounts.railKey(null), "")

    // Resolving a key back to a position, which is what that rebuild race makes necessary.
    var rail = [share, bookmark, dropbox]
    check("a share's key finds its own row", Mounts.rowByKey(rail, "smb://example.com/isos/"), 0)
    check("a key finds the row it named after the list moved", Mounts.rowByKey([bookmark, dropbox, share], "smb://example.com/isos/"), 2)
    check("a key naming a row that is gone resolves to nothing", Mounts.rowByKey([bookmark, dropbox], "smb://example.com/isos/"), -1)
    check("an empty key never resolves to whatever sits at index 0", Mounts.rowByKey(rail, ""), -1)
    check("an empty rail resolves nothing", Mounts.rowByKey([], "/dev/sda1"), -1)
    check("no rail at all resolves nothing rather than throwing", Mounts.rowByKey(null, "/dev/sda1"), -1)
    var volumes = [internal, volume]
    check("a volume's key finds it past the internal disk", Mounts.rowByKey(volumes, "/dev/sda1"), 1)
    check("the internal disk's device node is not a key and finds nothing", Mounts.rowByKey(volumes, "/dev/nvme0n1"), -1)

    // The call site and the path table checked together: Icons.pathFor answers the file mark in
    // silence, so a row naming a glyph PATHS has never heard of would draw a document instead.
    var rows = solid(Mounts.railMenu(volume)).concat(solid(Mounts.railMenu(share)))
    // Flash image is the one row the scan leaves out: the design names the file mark on purpose.
    rows = rows.filter(function (r) { return r.action !== "flashImage" })
    for (var r = 0; r < rows.length; r++) {
        check(rows[r].label + "'s mark is real, not the silent file fallback",
              Icons.pathFor(rows[r].glyph) === Icons.pathFor("file"), false)
    }

    // The rail polls every five seconds forever, so an unchanged poll must hand its Repeater the
    // array it already has: assigning a fresh one rebinds every row and empties an open editor.
    var share = { path: "", label: "NAS", group: "network", kind: "share", uri: "smb://example.com/data", mounted: false, glyph: "server" }
    var again = { path: "", label: "NAS", group: "network", kind: "share", uri: "smb://example.com/data", mounted: false, glyph: "server" }
    check("two polls that found the same share compare equal", Mounts.sameEntries([share], [again]), true)
    check("a share that has since mounted does not",
          Mounts.sameEntries([share], [{ path: "", label: "NAS", group: "network", kind: "share", uri: "smb://example.com/data", mounted: true, glyph: "server" }]), false)
    check("a relabelled share does not",
          Mounts.sameEntries([share], [{ path: "", label: "Homelab", group: "network", kind: "share", uri: "smb://example.com/data", mounted: false, glyph: "server" }]), false)
    check("a different share at the same position does not, which is the rebind that emptied an editor",
          Mounts.sameEntries([share], [{ path: "", label: "NAS", group: "network", kind: "share", uri: "smb://example.com/photos", mounted: false, glyph: "server" }]), false)
    check("a share appearing does not", Mounts.sameEntries([share], [share, again]), false)
    check("an empty rail still compares equal to itself", Mounts.sameEntries([], []), true)
    // The device builder writes device where the network one writes uri, so the comparison covers
    // both shapes: a volume that has just been unplugged differs in path and mounted alike.
    var vol = { path: "/run/media/user/128GB", label: "128GB", group: "device", kind: "volume", device: "/dev/sda1", mounted: true, glyph: "drive" }
    check("a device entry compares on its own fields too", Mounts.sameEntries([vol], [vol]), true)
    var capacity = Object.assign({}, vol, { size: "116.1G" })
    check("a capacity-only update reaches the live rail", Mounts.sameEntries([capacity], [Object.assign({}, capacity, { size: "119.2G" })]), false)
    check("and an unplugged volume differs",
          Mounts.sameEntries([vol], [{ path: "", label: "128GB", group: "device", kind: "volume", device: "/dev/sda1", mounted: false, glyph: "drive" }]), false)
    check("a missing side is never equal, so a first poll always assigns", Mounts.sameEntries(null, []), false)

    // What Ctrl+E releases from a listing: the mounted volume the directory is inside,
    // and nothing else, so the key can never release a volume the operator is not looking at.
    var stick = { label: "128GB", group: "device", kind: "volume", device: "/dev/sda1", path: "/run/media/user/128GB", mounted: true, removable: true }
    var pulled = { label: "128GB", group: "device", kind: "volume", device: "/dev/sda1", path: "", mounted: false, removable: true }
    var disk = { label: "nvme0n1", group: "device", kind: "disk", device: "/dev/nvme0n1", path: "/", mounted: true }
    var houses = [disk, stick]
    check("a directory inside the volume names it", Mounts.holding(houses, "/run/media/user/128GB/photos"), stick)
    check("the volume's own root names it", Mounts.holding(houses, "/run/media/user/128GB"), stick)
    check("a sibling that only shares the prefix does not", Mounts.holding(houses, "/run/media/user/128GB-old"), null)
    check("the internal disk holds everything and is never the answer", Mounts.holding(houses, "/etc"), null)
    check("an unmounted volume has no path to be inside", Mounts.holding([disk, pulled], "/run/media/user/128GB"), null)
    check("a share carries no path, so a listing inside one answers nothing rather than guessing",
          Mounts.holding([{ label: "isos", group: "network", kind: "share", uri: "smb://example.com/isos/", path: "", mounted: true }], "/run/user/1000/gvfs/x"), null)
    check("no rail at all answers nothing rather than throwing", Mounts.holding(null, "/x"), null)

    // The rail menu's chosen row, resolved by key; a key that no longer names a row releases nothing.
    function rec() { var a = []; return { a: a, eject: function (i) { a.push("e" + i) }, unmount: function (i) { a.push("u" + i) } } }
    function released(action, key) {
        var d = rec(), n = rec()
        RailMenu.release(action, key, d, n, { placesEntries: [], deviceEntries: [disk, stick], networkEntries: [{ label: "isos", group: "network", kind: "share", uri: "smb://x/isos/", path: "", mounted: true }] })
        return d.a.concat(n.a).join(",")
    }
    check("eject resolves the volume's position and never the network Service", released("eject", "/dev/sda1"), "e1")
    check("unmount resolves the share's position and never the device Service", released("unmount", "smb://x/isos/"), "u0")
    check("a key that no longer names a row releases nothing", released("eject", "/dev/sdz9"), "")
    check("an action that is neither release does nothing", released("forget", "/dev/sda1"), "")

    // The rail's one-step settle: network shows on its first answer and devices waits for it, else the deadline shows both.
    var gate = Mounts.railGroupsReady
    check("nothing answered before the deadline shows nothing",
          gate(false, false, 0, 800).showNetwork === false && gate(false, false, 0, 800).showDevices === false, true)
    check("network answered alone shows network only",
          gate(true, false, 0, 800).showNetwork === true && gate(true, false, 0, 800).showDevices === false, true)
    check("devices answered alone shows nothing, it would land above an empty network",
          gate(false, true, 0, 800).showNetwork === false && gate(false, true, 0, 800).showDevices === false, true)
    check("both answered before the deadline shows both",
          gate(true, true, 0, 800).showNetwork === true && gate(true, true, 0, 800).showDevices === true, true)
    check("one millisecond before the deadline still shows nothing",
          gate(false, false, 799, 800).showNetwork === false && gate(false, false, 799, 800).showDevices === false, true)
    check("the deadline itself shows both whatever each has",
          gate(false, false, 800, 800).showNetwork === true && gate(false, false, 800, 800).showDevices === true, true)
    check("a late answer after the deadline changes nothing visible",
          gate(true, false, 5000, 800).showNetwork === true && gate(true, false, 5000, 800).showDevices === true, true)

    // A mounted phone row draws the one-click mark like a drive or share; NFS keeps its square.
    var stick = { label: "128GB", group: "device", kind: "volume", device: "/dev/sda1", path: "/run/media/u/128GB", mounted: true, removable: true }
    var fixedDisk = { label: "Vault", group: "device", kind: "volume", device: "/dev/sdb1", path: "/mnt/vault", mounted: true, removable: false }
    var smb = { label: "NAS", group: "network", kind: "share", uri: "smb://h/data/", mounted: true }
    var nfs = { label: "Archive", group: "network", kind: "share", uri: "nfs://h/archive/", mounted: true }
    var phone = { label: "Pixel", group: "device", kind: "phone", uri: "mtp://x/", path: "/run/user/1000/gvfs/x", mounted: true }
    var gphoto = { label: "Camera", group: "device", kind: "phone", uri: "gphoto2://x/", path: "/run/user/1000/gvfs/y", mounted: true }
    var afc = { label: "iPhone", group: "device", kind: "phone", uri: "afc://x/", path: "/run/user/1000/gvfs/z", mounted: true }
    var cloud = { label: "gdrive", group: "network", kind: "cloud", uri: "", path: "/home/u/gdrive", mounted: true }
    check("a mounted removable drive draws the mark", Eject.releasable(stick), true)
    check("a mounted SMB share draws the mark", Eject.releasable(smb), true)
    check("an NFS share keeps its square", Eject.releasable(nfs), false)
    check("a mounted mtp phone draws the mark", Eject.releasable(phone), true)
    check("a mounted gphoto2 phone draws the mark", Eject.releasable(gphoto), true)
    check("a mounted afc phone draws the mark", Eject.releasable(afc), true)
    check("a cloud row keeps its square", Eject.releasable(cloud), false)
    check("a fixed internal volume keeps its square", Eject.releasable(fixedDisk), false)
    var idlePhone = { label: "Pixel", group: "device", kind: "phone", uri: "mtp://x/", path: "", mounted: false }
    check("an unmounted drive keeps its square", Eject.releasable(idle), false)
    check("an unmounted phone keeps its square", Eject.releasable(idlePhone), false)
    check("an unmounted share keeps its square", Eject.releasable(bookmark), false)
    check("no entry at all draws no mark", Eject.releasable(null), false)

    // A click on the mark is the release row itself: Eject for a drive, Unmount for a
    // share, the same action the menu row and Ctrl+E take.
    check("the mark on a drive is Eject", Eject.releaseAction(stick), "eject")
    check("the mark on a share is Unmount", Eject.releaseAction(smb), "unmount")
    check("the mark on a phone is its Unmount", Eject.releaseAction(phone), "unmountPhone")
    check("the mark on a gphoto2 phone is its Unmount", Eject.releaseAction(gphoto), "unmountPhone")
    check("the mark on an afc phone is its Unmount", Eject.releaseAction(afc), "unmountPhone")
    check("a row with no mark has no click action", Eject.releaseAction(nfs), "")
    check("and neither does a cloud row", Eject.releaseAction(cloud), "")

    // The five second poll rebuilds only on changed text; see ui/js/Mounts.js pollDecision.
    check("before the first listing nothing rebuilds", Mounts.pollDecision(false, "a", "", "b", ""), "none")
    check("unchanged texts rebuild nothing", Mounts.pollDecision(true, "a", "a", "b", "b"), "skip")
    check("a changed listing rebuilds", Mounts.pollDecision(true, "a", "x", "b", "b"), "rebuild")
    check("a changed mountinfo rebuilds", Mounts.pollDecision(true, "a", "a", "b", "y"), "rebuild")
    check("both changed still rebuilds once", Mounts.pollDecision(true, "a", "x", "b", "y"), "rebuild")

    // Defect 8: gio's stderr is mapped to one plain sentence per known failure, never dropped.
    check("a busy target names what holds it", Devices.mountError("mount", 1, "gio: Error 1: target is busy", "128GB"), "128GB is busy; close what is using it and try again.")
    check("a polkit refusal names the refusal", Devices.mountError("mount", 1, "Not authorized to perform operation", "Vault"), "Vault was not mounted: you do not have permission.")
    check("no volume for the id names the missing volume", Devices.mountError("mount", 1, "No volume for device /dev/sda9", "sda9"), "sda9 cannot be mounted on this computer.")
    check("already mounted opens rather than failing", Devices.mountError("mount", 1, "Location is already mounted", "isos"), "")
    check("no medium names the empty drive", Devices.mountError("mount", 1, "No medium found", "CARD"), "CARD has no disc or card in it.")
    check("an unknown error falls back to the plain sentence", Devices.mountError("mount", 1, "weird new failure", "128GB"), "128GB could not be mounted.")
    check("a refused unmount names the refusal too", Devices.mountError("unmount", 1, "Not authorized", "Vault"), "Vault was not unmounted: you do not have permission.")
    check("a refused eject names the eject", Devices.mountError("eject", 1, "Not authorized", "Disk"), "Disk was not ejected: you do not have permission.")
    check("exit 0 never reads stderr", Devices.mountError("mount", 0, "target is busy", "128GB"), "")

    // MountFail: a corrupted volume names its damaged file system, and gio's own reason rides as the detail.
    var damaged = "Error mounting /dev/sdb1 at /run/media/user/DATA #2: wrong fs type, bad option, bad superblock on /dev/sdb1, missing codepage or helper program, or other error"
    var damagedLine = "DATA #2 could not be mounted: its file system is damaged or not supported."
    check("a bad superblock names the damaged file system", Devices.mountError("mount", 1, damaged, "DATA #2"), damagedLine)
    check("wrong fs type alone is the damaged sentence", Devices.mountError("mount", 1, "gio: Error mounting /dev/sdc1 at /run/media/g/X: wrong fs type", "X"), "X could not be mounted: its file system is damaged or not supported.")
    check("an unknown filesystem type is the damaged sentence", Devices.mountError("mount", 1, "mount: unknown filesystem type 'zfs_member'", "Pool"), "Pool could not be mounted: its file system is damaged or not supported.")
    check("an unreadable superblock is the damaged sentence", Devices.mountError("mount", 1, "can't read superblock on /dev/sdb1", "Disk"), "Disk could not be mounted: its file system is damaged or not supported.")
    check("a structure that needs cleaning is the damaged sentence", Devices.mountError("mount", 1, "mount(2) system call failed: Structure needs cleaning.", "Disk"), "Disk could not be mounted: its file system is damaged or not supported.")
    check("an unmount never takes the mount sentence", Devices.mountError("unmount", 1, damaged, "Disk"), "Disk could not be unmounted.")
    check("the detail drops gio's lead and keeps the reason verbatim", Devices.mountDetail(damaged), "wrong fs type, bad option, bad superblock on /dev/sdb1, missing codepage or helper program, or other error")
    check("the detail drops a gio: prefix and the lead", Devices.mountDetail("gio: " + damaged + "\n"), "wrong fs type, bad option, bad superblock on /dev/sdb1, missing codepage or helper program, or other error")
    check("a reason with no lead keeps itself", Devices.mountDetail("gio: Error 1: target is busy"), "Error 1: target is busy")
    check("a multi-line reason is one line", Devices.mountDetail("Not authorized\n  to perform operation\n"), "Not authorized to perform operation")
    check("an unmount reason drops its lead", Devices.mountDetail("Error unmounting /dev/sdb1: target is busy"), "target is busy")
    check("an eject reason drops its lead", Devices.mountDetail("gio: Error ejecting /dev/sdb: Not authorized\n"), "Not authorized")
    check("an unmount reason drops gio's file-URI lead", Devices.mountDetail("gio: file:///run/media/u/Vault: Error unmounting /dev/sdb1: target is busy"), "target is busy")
    check("an eject reason drops gio's file-URI lead", Devices.mountDetail("gio: file:///run/media/u/Stick: Error ejecting /dev/sdb: Not authorized\n"), "Not authorized")
    check("a lookup failure drops gio's file-URI lead", Devices.mountDetail("gio: file:///nonexistent: Containing mount for file /nonexistent not found"), "Containing mount for file /nonexistent not found")
    check("an unmount refusal with no lead keeps itself", Devices.mountDetail("Not authorized"), "Not authorized")
    check("an empty stderr has no detail", Devices.mountDetail(""), "")
    check("a missing stderr has no detail", Devices.mountDetail(undefined), "")

    // Defect 9: a USB disk that is not media-removable powers off instead of ejecting its media.
    check("a scsi partition powers off its disk", Devices.powerOffDisk("/dev/sda1"), "/dev/sda")
    check("an nvme namespace strip powers off its disk", Devices.powerOffDisk("/dev/nvme0n1p2"), "/dev/nvme0n1")
    check("an mmc partition powers off its card", Devices.powerOffDisk("/dev/mmcblk0p1"), "/dev/mmcblk0")
    check("a mapper leaf has no disk to power off", Devices.powerOffDisk("/dev/mapper/luks-vault"), "")
    check("an empty device has no disk to power off", Devices.powerOffDisk(""), "")

    // Defect 17: Move to Trash is absent where the volume cannot trash, read-only included.
    check("a writable local folder trashes", Mounts.trashable("/home/gm/work", true), true)
    check("a gvfs share never trashes", Mounts.trashable("/run/user/1000/gvfs/smb-share:server=h,share=d/x", true), false)
    check("a read-only folder never trashes", Mounts.trashable("/home/gm/work", false), false)
    check("an old caller passing no writability still trashes a local folder", Mounts.trashable("/home/gm/work"), true)

    // gio only enriches kernel rows, which the rail matches by path: each file:// Mount() line decodes to its path with gio's label and uri.
    var kgio = 'Mount(0): photos -> file:///mnt/kmount-photos\n'
             + 'Mount(1): iso-images -> file:///mnt/kmount-iso/\n'
             + 'Mount(2): isos on 192.168.1.10 -> smb://192.168.1.10/isos/\n'
             + 'Mount(3): 32GB USB Drive -> file:///run/media/gm/32GB%20USB%20Drive/\n'
             + 'Mount(4): idle-share -> file:///mnt/kmount-idle\n'
    var krows = Mounts.gioKernelMounts(kgio)
    check("every file:// line decodes with gio's own spelling", krows.length, 4)
    check("the nfs line keeps gio's mount name", krows[0].label, "photos")
    check("the nfs line keeps gio's uri spelling", krows[0].uri, "file:///mnt/kmount-photos")
    check("the nfs line resolves to its mountpoint", krows[0].path, "/mnt/kmount-photos")
    check("a trailing slash still resolves to the mountpoint", krows[1].path, "/mnt/kmount-iso")
    check("a percent-encoded path decodes before the compare",
          krows[2].path, "/run/media/gm/32GB USB Drive")
    check("no gio text means nothing to join", Mounts.gioKernelMounts("").length, 0)
    check("garbage means nothing to join", Mounts.gioKernelMounts("not gio output\n").length, 0)
    check("a non-file line is not a kernel enrichment",
          Mounts.gioKernelMounts('Mount(0): isos -> smb://h/isos/\n').length, 0)

    // The kernel rows ride the one release path: Open beside Unmount, no place-editing rows, the mark split by fstype.
    var knfs = { path: "/mnt/kmount-photos", label: "photos", group: "network", kind: "share",
                 uri: "file:///mnt/kmount-photos", fstype: "nfs4", mounted: true, glyph: "server", editable: false }
    var kcifs = { path: "/mnt/kmount-iso", label: "iso-images", group: "network", kind: "share",
                  uri: "file:///mnt/kmount-iso", fstype: "cifs", mounted: true, glyph: "server", editable: false }
    var kbulk = { path: "/mnt/kmount-bulk", label: "kmount-bulk", group: "network", kind: "share",
                  uri: "/mnt/kmount-bulk", fstype: "nfs", mounted: true, glyph: "server", editable: false }
    var krclone = { path: "/mnt/kmount-rclone", label: "kmount-rclone", group: "network", kind: "share",
                    uri: "/mnt/kmount-rclone", fstype: "fuse.rclone", mounted: true, glyph: "server", editable: false }
    function klabels(rows) { return rows.filter(function (r) { return r.separator !== true }).map(function (r) { return r.label }).join(",") }
    function kactions(rows) { return rows.filter(function (r) { return r.separator !== true }).map(function (r) { return r.action }).join(",") }
    check("a kernel cifs row opens beside its unmount and nothing else",
          klabels(Mounts.railMenu(kcifs)), "Open,Unmount")
    check("its rows take the same actions a share row's do",
          kactions(Mounts.railMenu(kcifs)), "open,unmount")
    check("a kernel nfs row keeps the same two rows",
          klabels(Mounts.railMenu(knfs)), "Open,Unmount")
    check("a kernel cifs mark releases like a gvfs smb one", Eject.releasable(kcifs), true)
    check("a kernel nfs row keeps its square like a gvfs nfs one", Eject.releasable(knfs), false)
    check("a kernel row gio never lists keeps its square too", Eject.releasable(kbulk), false)
    check("the mark on a kernel cifs row is Unmount", Eject.releaseAction(kcifs), "unmount")
    check("a kernel nfs row has no mark to click", Eject.releaseAction(knfs), "")
    check("a kernel row's key is its file uri", Mounts.railKey(kcifs), "file:///mnt/kmount-iso")
    check("a row gio never lists is keyed by its path", Mounts.railKey(kbulk), "/mnt/kmount-bulk")
    check("a row gio never lists opens beside its unmount and nothing else",
          klabels(Mounts.railMenu(kbulk)), "Open,Unmount")
    check("a FUSE network row opens beside its unmount too",
          klabels(Mounts.railMenu(krclone)), "Open,Unmount")
    check("a FUSE network row keeps its square", Eject.releasable(krclone), false)
    check("a listing inside a kernel mount resolves to its row",
          Mounts.holding([knfs, kcifs], "/mnt/kmount-iso/films"), kcifs)
    check("the mountpoint itself resolves too",
          Mounts.holding([knfs, kcifs], "/mnt/kmount-iso"), kcifs)
    check("a sibling prefix does not resolve",
          Mounts.holding([knfs, kcifs], "/mnt/kmount-iso-old"), null)
    check("an fstype change rebuilds the row",
          Mounts.sameEntries([kcifs], [Object.assign({}, kcifs, { fstype: "nfs" })]), false)
    check("the same fstype compares equal",
          Mounts.sameEntries([kcifs], [Object.assign({}, kcifs)]), true)
    // An idle systemd automount is an autofs line and no row; on a triggered one the stacked nfs4 line wins the mountpoint.
    check("idle automount no row, triggered one nfs4", Cloud.parseNetworkMounts("39 1 0:62 / /mnt/idle rw - autofs systemd-1 rw\n37 1 0:60 / /mnt/live rw - autofs systemd-1 rw\n38 37 0:61 / /mnt/live rw - nfs4 nas:/x rw\n").map(function (r) { return r.path + ":" + r.fstype }).join(","), "/mnt/live:nfs4")
}
