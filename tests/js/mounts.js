.import "../../ui/js/Devices.js" as Devices
.import "../../ui/js/Mounts.js" as Mounts
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
    var share = { label: "isos", group: "network", kind: "share", uri: "smb://example.com/isos/", mounted: true }
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

    check("a mounted removable volume opens beside its release", labelsOf(Mounts.railMenu(volume)), "Open,Eject")
    check("Open runs the row's own activation", solid(Mounts.railMenu(volume))[0].action, "open")
    check("Open takes the listing menu's own mark", solid(Mounts.railMenu(volume))[0].glyph, "folder-open")
    check("the Eject row carries the eject action", solid(Mounts.railMenu(volume))[1].action, "eject")
    check("the Eject row draws the eject mark", solid(Mounts.railMenu(volume))[1].glyph, "eject")
    check("a mounted loop device opens beside Eject despite removable being false", labelsOf(Mounts.railMenu(loopRow)), "Open,Eject")
    check("the loop row's Eject carries the eject action", solid(Mounts.railMenu(loopRow))[1].action, "eject")
    check("a group change draws the listing menu's own separator", Mounts.railMenu(volume).length, 3)
    check("a mounted network share opens beside its release", labelsOf(Mounts.railMenu(share)), "Open,Unmount,Rename,Edit address,Remove from Network")
    check("its Open is the listing menu's own row too", actionsOf(Mounts.railMenu(share)), "open,unmount,rename,editPlace,remove")
    check("Mount and Unmount share the drive mark, Eject keeps its own", glyphsOf(Mounts.railMenu(share)), "folder-open,drive,rename,sliders,minus")

    // Rows never offered a release, each for its own reason; an unmounted place keeps its own three rows.
    check("the internal disk offers nothing, it is the box's own system disk", Mounts.railMenu(internal).length, 0)
    check("a fixed internal volume offers nothing, a disk bolted in is not ejected", Mounts.railMenu(fixed).length, 0)
    check("the Dropbox row offers nothing, it is a local folder the stock service owns", Mounts.railMenu(dropbox).length, 0)
    check("a favourite offers INVENTORY's own Remove from Favorites", labelsOf(Mounts.railMenu(favourite)), "Remove from Favorites")
    check("an unmounted volume offers nothing, there is nothing to release", Mounts.railMenu(idle).length, 0)
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

    // What Ctrl+E releases from a listing: the mounted removable volume the directory is inside,
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
}
