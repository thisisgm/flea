.import "../../ui/js/Mounts.js" as Mounts
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

    // Real lsblk --json output, captured on the box with a USB stick plugged in (2026-09-02).
    // ui/DeviceMounts.qml feeds this exact command's stdout to parseDevices on the rail's own clock.
    var live = '{"blockdevices":['
             + '{"name":"loop0","label":"FLEATEST","mountpoint":null,"rm":false,"size":"64M","type":"loop","model":null},'
             + '{"name":"sda","label":null,"mountpoint":null,"rm":true,"size":"116.1G","type":"disk","model":"USB Flash Disk",'
             + '"children":[{"name":"sda1","label":"128GB","mountpoint":"/run/media/gm/128GB","rm":true,"size":"116.1G","type":"part","model":null}]},'
             + '{"name":"zram0","label":"zram0","mountpoint":"[SWAP]","rm":false,"size":"19.3G","type":"disk","model":null},'
             + '{"name":"nvme0n1","label":null,"mountpoint":null,"rm":false,"size":"238.5G","type":"disk","model":"KBG40ZNS256G",'
             + '"children":[{"name":"nvme0n1p1","label":null,"mountpoint":"/boot","rm":false,"size":"2G","type":"part","model":null}]}'
             + ']}'
    var d = Mounts.parseDevices(live)
    check("the live box has one internal disk and one removable volume", d.length, 2)
    check("the internal disk sorts first", d[0].kind, "disk")
    check("the internal disk is named by its kernel name", d[0].label, "nvme0n1")
    check("the internal disk row opens the root of the filesystem", d[0].path, "/")
    check("the internal disk is always mounted", d[0].mounted, true)
    check("zram is not the internal disk", d[0].device, "/dev/nvme0n1")
    check("the removable volume takes the filesystem label", d[1].label, "128GB")
    check("the removable volume carries its device node for gio", d[1].device, "/dev/sda1")
    check("a mounted volume carries the mountpoint lsblk reported", d[1].path, "/run/media/gm/128GB")
    check("a volume with a mountpoint reads as mounted", d[1].mounted, true)
    check("a loop device is not a device row", d.map(function (e) { return e.label }).join(","), "nvme0n1,128GB")

    // No devices at all: the rail self-hides on this, so it must be an empty list and never a throw.
    check("empty lsblk output parses to nothing", Mounts.parseDevices("").length, 0)
    check("garbage lsblk output parses to nothing", Mounts.parseDevices("not json at all\n").length, 0)
    check("valid json with no blockdevices key parses to nothing", Mounts.parseDevices("{}").length, 0)
    check("an empty blockdevices array parses to nothing", Mounts.parseDevices('{"blockdevices":[]}').length, 0)

    // Present and unmounted: the state a stick sits in on this box, which automounts nothing.
    var unmounted = '{"blockdevices":[{"name":"sda","label":null,"mountpoint":null,"rm":true,"size":"116.1G","type":"disk","model":"USB Flash Disk",'
                  + '"children":[{"name":"sda1","label":"128GB","mountpoint":null,"rm":true,"size":"116.1G","type":"part","model":null}]}]}'
    var u = Mounts.parseDevices(unmounted)
    check("an unmounted stick is still a row", u.length, 1)
    check("an unmounted volume reads as unmounted", u[0].mounted, false)
    check("an unmounted volume has no path to open yet", u[0].path, "")
    check("an unmounted volume still carries the device node its mount needs", u[0].device, "/dev/sda1")

    // Several at once, and the internal disk row is one whatever the box has.
    var many = '{"blockdevices":['
             + '{"name":"nvme0n1","label":null,"mountpoint":null,"rm":false,"size":"238.5G","type":"disk","model":"KBG40ZNS256G"},'
             + '{"name":"nvme1n1","label":null,"mountpoint":null,"rm":false,"size":"1T","type":"disk","model":"Second NVMe"},'
             + '{"name":"sda","label":null,"mountpoint":null,"rm":true,"size":"116.1G","type":"disk","model":"USB Flash Disk",'
             + '"children":[{"name":"sda1","label":"first","mountpoint":"/run/media/gm/first","rm":true,"size":"58G","type":"part","model":null},'
             + '{"name":"sda2","label":"second","mountpoint":null,"rm":true,"size":"58G","type":"part","model":null}]},'
             + '{"name":"sdb","label":"CARD","mountpoint":null,"rm":true,"size":"32G","type":"disk","model":"SD Reader"}'
             + ']}'
    var m = Mounts.parseDevices(many)
    check("four rows come out of two sticks and two internal disks", m.length, 4)
    check("only one internal disk row is ever emitted", m[0].label, "nvme0n1")
    check("both partitions of one stick are rows", m[1].label + "," + m[2].label, "first,second")
    check("an unpartitioned removable disk is a row of its own", m[3].label, "CARD")
    check("an unpartitioned removable disk carries its own device node", m[3].device, "/dev/sdb")

    // The label ladder: filesystem label, then the drive's product name, then the kernel name.
    var noLabel = '{"blockdevices":[{"name":"sda","label":null,"mountpoint":null,"rm":true,"size":"116.1G","type":"disk","model":"USB Flash Disk",'
                + '"children":[{"name":"sda1","label":null,"mountpoint":null,"rm":true,"size":"116.1G","type":"part","model":null}]}]}'
    check("an unlabelled volume falls back to the drive's product name", Mounts.parseDevices(noLabel)[0].label, "USB Flash Disk")
    var noModel = '{"blockdevices":[{"name":"sdb","label":null,"mountpoint":null,"rm":true,"size":"32G","type":"disk","model":null,'
                + '"children":[{"name":"sdb1","label":null,"mountpoint":null,"rm":true,"size":"32G","type":"part","model":null}]}]}'
    check("a volume with neither label nor model falls back to the kernel name", Mounts.parseDevices(noModel)[0].label, "sdb1")

    // A label is a name off somebody else's filesystem, so it is data: the parser never rewrites it
    // and ui/SidebarRow.qml draws it through Text.PlainText.
    var awkward = '{"blockdevices":[{"name":"sda","label":null,"mountpoint":null,"rm":true,"size":"8G","type":"disk","model":null,'
                + '"children":[{"name":"sda1","label":"Sauvegarde & Co \\"2026\\" <b>","mountpoint":null,"rm":true,"size":"8G","type":"part","model":null}]}]}'
    check("an awkward label survives the parse verbatim", Mounts.parseDevices(awkward)[0].label, 'Sauvegarde & Co "2026" <b>')

    var longName = "Photographs and scans of every receipt from two thousand and twenty six, quarter one through quarter four"
    var longLabel = '{"blockdevices":[{"name":"sda","label":null,"mountpoint":null,"rm":true,"size":"8G","type":"disk","model":null,'
                  + '"children":[{"name":"sda1","label":"' + longName + '","mountpoint":null,"rm":true,"size":"8G","type":"part","model":null}]}]}'
    check("a very long label is elided by the row, never truncated by the parser", Mounts.parseDevices(longLabel)[0].label, longName)

    // A mountpoint with a space needs no decoding here, and that is measured rather than assumed:
    // the kernel writes /tmp/.../USB\040Drive in /proc/self/mountinfo, and lsblk --json was run
    // against a real vfat mount at that path and printed "/tmp/.../USB Drive" with a literal space.
    var spaced = '{"blockdevices":[{"name":"sda","label":null,"mountpoint":null,"rm":true,"size":"8G","type":"disk","model":null,'
               + '"children":[{"name":"sda1","label":"USB Drive","mountpoint":"/run/media/gm/USB Drive","rm":true,"size":"8G","type":"part","model":null}]}]}'
    check("a mountpoint with a space is opened verbatim", Mounts.parseDevices(spaced)[0].path, "/run/media/gm/USB Drive")

    // A trust boundary: a node with no name would build "/dev/undefined" and hand it to gio.
    var noName = '{"blockdevices":[{"label":"nameless","mountpoint":null,"rm":true,"size":"8G","type":"part","model":null}]}'
    check("a node with no name is not a row", Mounts.parseDevices(noName).length, 0)

    // The two listings share the rail but never the parser, so a device body must not read as a
    // network mount: parseMounts anchors Mount() at column zero and lsblk emits no such line.
    check("an lsblk body yields no network mounts", Mounts.parseMounts(live).length, 0)

    // The eject verdict is read off the listing taken after gio exits, never off gio's exit code,
    // which has been 0 over a volume that stayed mounted. It judges the whole disk the device sits on.
    check("a listing that still mounts the ejected volume refuses the verdict", Eject.verdict(live, "/dev/sda1"), "mounted")
    check("a listing with the volume present but unmounted is safe", Eject.verdict(unmounted, "/dev/sda1"), "safe")
    var gone = '{"blockdevices":[{"name":"nvme0n1","label":null,"mountpoint":null,"rm":false,"size":"238.5G","type":"disk","model":"KBG40ZNS256G"}]}'
    check("a listing the device has vanished from is safe", Eject.verdict(gone, "/dev/sda1"), "safe")
    var mediaOut = '{"blockdevices":[{"name":"sda","label":null,"mountpoint":null,"rm":true,"size":"0B","type":"disk","model":"USB Flash Disk"}]}'
    check("a stick whose media ejected but whose disk node stayed is safe", Eject.verdict(mediaOut, "/dev/sda1"), "safe")
    check("a sibling partition still mounted refuses the verdict for its unmounted neighbour", Eject.verdict(many, "/dev/sda2"), "mounted")
    var crypt = '{"blockdevices":[{"name":"sda","label":null,"mountpoint":null,"rm":true,"size":"8G","type":"disk","model":null,'
              + '"children":[{"name":"sda1","label":null,"mountpoint":null,"rm":true,"size":"8G","type":"part","model":null,'
              + '"children":[{"name":"luks-vault","label":"vault","mountpoint":"/run/media/gm/vault","rm":false,"size":"8G","type":"crypt","model":null}]}]}]}'
    check("a mounted crypt child under an unmounted partition refuses the verdict", Eject.verdict(crypt, "/dev/sda1"), "mounted")
    var swap = '{"blockdevices":[{"name":"sda","label":null,"mountpoint":null,"rm":true,"size":"8G","type":"disk","model":null,'
             + '"children":[{"name":"sda1","label":null,"mountpoint":"[SWAP]","rm":true,"size":"8G","type":"part","model":null}]}]}'
    check("active swap on the stick counts as mounted", Eject.verdict(swap, "/dev/sda1"), "mounted")
    check("garbage in place of a listing is unknown, never safe", Eject.verdict("not json at all\n", "/dev/sda1"), "unknown")
    check("an empty body is unknown, never safe", Eject.verdict("", "/dev/sda1"), "unknown")
    check("json with no blockdevices key is unknown, never safe", Eject.verdict("{}", "/dev/sda1"), "unknown")
    check("a listing with no block devices at all is unknown, never safe", Eject.verdict('{"blockdevices":[]}', "/dev/sda1"), "unknown")
    check("an empty device name is unknown, never safe", Eject.verdict(live, ""), "unknown")

    // Only the safe verdict may say safe; the other two tell the user to leave the stick in.
    check("the safe verdict earns the unplug sentence", Eject.sentence("safe", "128GB").text, "Ejected 128GB, it is safe to unplug.")
    check("the safe verdict is not an error", Eject.sentence("safe", "128GB").isError, false)
    check("the mounted verdict with the row itself still mounted says try again", Eject.sentence("mounted", "128GB", []).text, "128GB could not be ejected; it is still mounted, close anything using it and try again.")
    check("the mounted verdict is an error", Eject.sentence("mounted", "128GB", []).isError, true)
    check("a sibling blocker is named instead of an instruction that would do nothing", Eject.sentence("mounted", "second", ["first"]).text, "second could not be ejected; first on the same drive is still mounted, eject that instead.")
    check("two blockers are both named", Eject.sentence("mounted", "third", ["first", "second"]).text, "third could not be ejected; first, second on the same drive are still mounted, eject those instead.")
    check("the unknown verdict says do not unplug", Eject.sentence("unknown", "128GB").text, "Could not confirm 128GB was ejected; do not unplug it yet.")
    check("the unknown verdict is an error", Eject.sentence("unknown", "128GB").isError, true)
    var others = ["mounted", "unknown", "", undefined, "SAFE"]
    check("no verdict but safe ever says safe to unplug", others.some(function (v) { return /safe to unplug/.test(Eject.sentence(v, "X", ["Y"]).text) }), false)

    // What the refusal names: nothing while the row itself is still mounted, else what still is.
    check("a row still mounted itself names no blocker, a re-press is the next step", Eject.blockers(live, "/dev/sda1").join(","), "")
    check("an unmounted row names the mounted sibling by its label", Eject.blockers(many, "/dev/sda2").join(","), "first")
    check("an unmounted partition names its mounted crypt child", Eject.blockers(crypt, "/dev/sda1").join(","), "vault")
    check("a vanished device names nothing", Eject.blockers(gone, "/dev/sda1").join(","), "")
    check("garbage names nothing", Eject.blockers("not json", "/dev/sda1").join(","), "")

    // The rail's own context menu. Which release a row offers is decided from the kind the rail
    // already tagged: parseDevices above tags a removable volume "volume" off lsblk's RM flag and
    // the box's own disk "disk", and ui/NetworkMounts.qml tags a gvfs share "share".
    var volume = { label: "128GB", group: "device", kind: "volume", device: "/dev/sda1", mounted: true }
    var idle = { label: "128GB", group: "device", kind: "volume", device: "/dev/sda1", mounted: false }
    var internal = { label: "nvme0n1", group: "device", kind: "disk", device: "/dev/nvme0n1", mounted: true }
    var share = { label: "isos", group: "network", kind: "share", uri: "smb://example.com/isos/", mounted: true }
    var bookmark = { label: "NAS", group: "network", kind: "share", uri: "smb://example.com/", mounted: false }
    var dropbox = { label: "Dropbox", group: "network", kind: "dropbox", uri: "", mounted: true }
    var favourite = { label: "Home", group: "favorite", kind: "favorite", path: "/home/user" }

    check("a mounted removable volume offers one row", Mounts.railMenu(volume).length, 1)
    check("and that row is Eject", Mounts.railMenu(volume)[0].label, "Eject")
    check("the Eject row carries the eject action", Mounts.railMenu(volume)[0].action, "eject")
    check("the Eject row draws the eject mark", Mounts.railMenu(volume)[0].glyph, "eject")
    check("a mounted network share offers one row", Mounts.railMenu(share).length, 1)
    check("and that row is Unmount", Mounts.railMenu(share)[0].label, "Unmount")
    check("the Unmount row carries the unmount action", Mounts.railMenu(share)[0].action, "unmount")
    check("the shelf lists eject for unmount too, so Unmount draws it", Mounts.railMenu(share)[0].glyph, "eject")

    // Every rail row that must never be offered a release, each for its own reason.
    check("the internal disk offers nothing, it is the box's own system disk", Mounts.railMenu(internal).length, 0)
    check("the Dropbox row offers nothing, it is a local folder the stock service owns", Mounts.railMenu(dropbox).length, 0)
    check("a favourite offers nothing, it is not a mount at all", Mounts.railMenu(favourite).length, 0)
    check("an unmounted volume offers nothing, there is nothing to release", Mounts.railMenu(idle).length, 0)
    check("a bookmark nothing has mounted offers nothing", Mounts.railMenu(bookmark).length, 0)
    check("no entry at all offers nothing rather than throwing", Mounts.railMenu(null).length, 0)
    check("an undefined entry offers nothing rather than throwing", Mounts.railMenu(undefined).length, 0)

    // The safety property as one check: eject reaches exactly one kind of row and no other.
    var never = [idle, internal, dropbox, favourite, bookmark, share, null, undefined]
    check("no row but a mounted removable volume is ever offered eject",
          never.some(function (e) {
              return Mounts.railMenu(e).some(function (r) { return r.action === "eject" })
          }), false)

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
    var rows = Mounts.railMenu(volume).concat(Mounts.railMenu(share))
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
    check("and an unplugged volume differs",
          Mounts.sameEntries([vol], [{ path: "", label: "128GB", group: "device", kind: "volume", device: "/dev/sda1", mounted: false, glyph: "drive" }]), false)
    check("a missing side is never equal, so a first poll always assigns", Mounts.sameEntries(null, []), false)

    // What Ctrl+E releases from a listing: the mounted removable volume the directory is inside,
    // and nothing else, so the key can never release a volume the operator is not looking at.
    var stick = { label: "128GB", group: "device", kind: "volume", device: "/dev/sda1", path: "/run/media/user/128GB", mounted: true }
    var pulled = { label: "128GB", group: "device", kind: "volume", device: "/dev/sda1", path: "", mounted: false }
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
        Mounts.release(action, key, d, n, { favoriteEntries: [], deviceEntries: [disk, stick], networkEntries: [{ label: "isos", group: "network", kind: "share", uri: "smb://x/isos/", path: "", mounted: true }] })
        return d.a.concat(n.a).join(",")
    }
    check("eject resolves the volume's position and never the network Service", released("eject", "/dev/sda1"), "e1")
    check("unmount resolves the share's position and never the device Service", released("unmount", "smb://x/isos/"), "u0")
    check("a key that no longer names a row releases nothing", released("eject", "/dev/sdz9"), "")
    check("an action that is neither release does nothing", released("forget", "/dev/sda1"), "")
}
