.import "../../ui/js/Devices.js" as Devices
.import "../../ui/js/Mounts.js" as Mounts
.import "../../ui/js/RailMenu.js" as RailMenu

// RailAdditions rules 1 and 2: the volumes nothing has mounted, behind their own switch, and the
// menu those rows carry. Split out of tests/js/devices.js, which keeps the 0.2.1 rail's own parse.

// One internal disk carrying /, a second drive with five volumes, one of each exception the rule
// names. fstype and parttypename are what it reads, and tests/js/devices.js's live listing predates
// both columns, which is the other half of why this fixture is its own.
var unmountedBox = '{"blockdevices":['
                 + '{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G","fstype":null,"parttypename":null,'
                 + '"children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":256060514304,"type":"part","model":null,"fstype":"btrfs","parttypename":"Linux filesystem"}]},'
                 + '{"name":"sdb","path":"/dev/sdb","label":null,"mountpoints":[null],"rm":false,"size":2000398934016,"type":"disk","model":"Samsung SSD 870","fstype":null,"parttypename":null,'
                 + '"children":[{"name":"sdb1","path":"/dev/sdb1","label":"Archive","mountpoints":[null],"rm":false,"size":1000398934016,"type":"part","model":null,"fstype":"ext4","parttypename":"Linux filesystem"},'
                 + '{"name":"sdb2","path":"/dev/sdb2","label":null,"mountpoints":[null],"rm":false,"size":536870912,"type":"part","model":null,"fstype":"vfat","parttypename":"EFI System"},'
                 + '{"name":"sdb3","path":"/dev/sdb3","label":null,"mountpoints":["[SWAP]"],"rm":false,"size":8589934592,"type":"part","model":null,"fstype":"swap","parttypename":"Linux swap"},'
                 + '{"name":"sdb4","path":"/dev/sdb4","label":null,"mountpoints":[null],"rm":false,"size":268435456,"type":"part","model":null,"fstype":null,"parttypename":"Linux filesystem"},'
                 + '{"name":"sdb5","path":"/dev/sdb5","label":null,"mountpoints":[null],"rm":false,"size":268435456,"type":"part","model":null,"fstype":"crypto_LUKS","parttypename":"Linux filesystem"}]}'
                 + ']}'

function labels(rows) {
    return rows.map(function (e) { return e.label }).join(",")
}

// A parser row the way ui/DeviceMounts.qml rebuilds it into a rail entry: the group is the
// Service's to add, so a releaseMark check has to add it the same way.
function entry(r) {
    return { path: r.path, label: r.label, group: "device", kind: r.kind, device: r.device,
             mounted: r.mounted, removable: r.removable, size: r.size,
             volumeMenu: r.volumeMenu === true, attached: r.attached === true, glyph: "drive" }
}

function run(check) {
    var off = Devices.parseDevices(unmountedBox, false)
    check("with the switch off the rail is the one 0.2.1 drew", labels(off), "nvme0n1")
    var on = Devices.parseDevices(unmountedBox, true)
    check("the volume nothing mounted joins the rail, and only it", labels(on), "nvme0n1,Archive")
    check("the unmounted volume reads as unmounted", on[1].mounted, false)
    check("it has no mountpoint to open, so it has no path", on[1].path, "")
    check("it keeps the RailDetails column's own size", on[1].size, 1000398934016)
    check("it is not removable, so nothing offers to eject a fixed disk", on[1].removable, false)
    check("and it carries its device node, which is what gio mounts", on[1].device, "/dev/sdb1")
    check("only a row built under the switch carries the board's own menu", on[1].volumeMenu, true)
    // A stick is a row either way, so it is the one that says what the switch does to an existing row.
    var stickBox = '{"blockdevices":[{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":124656812032,"type":"disk","model":"USB Flash Disk","fstype":null,"parttypename":null,'
              + '"children":[{"name":"sda1","path":"/dev/sda1","label":"128GB","mountpoints":["/run/media/gm/128GB"],"rm":true,"size":124656812032,"type":"part","model":null,"fstype":"vfat","parttypename":"W95 FAT32"}]}]}'
    check("and a row built without it carries the menu it carried in 0.2.1",
          Devices.parseDevices(stickBox, false)[0].volumeMenu, false)

    // Rule 1's three exceptions, each named by lsblk rather than guessed from a name.
    check("swap is not a place to browse", labels(on).indexOf("sdb3"), -1)
    check("the EFI system partition is the box's own plumbing", labels(on).indexOf("sdb2"), -1)
    check("a volume with no filesystem has nothing to mount", labels(on).indexOf("sdb4"), -1)
    check("a locked LUKS container mounts through its crypt child, never itself", labels(on).indexOf("sdb5"), -1)

    // Rule 2's rows, which only a row from rule 1 carries.
    var unmounted = { group: "device", kind: "volume", mounted: false, removable: false, volumeMenu: true }
    check("an unmounted volume offers the mount its own activation does",
          Mounts.railMenu(unmounted).map(function (r) { return r.label + ":" + r.action }).join(","), "Mount:mountVolume")
    var mounted = { group: "device", kind: "volume", mounted: true, removable: false, volumeMenu: true }
    check("a mounted one offers the open beside the release",
          Mounts.railMenu(mounted).map(function (r) { return r.label + ":" + r.action }).join(","),
          "Open:openVolume,Unmount:unmountVolume")
    var stick = { group: "device", kind: "volume", mounted: true, removable: true, volumeMenu: true }
    check("and Eject stays where it stands today, on a volume somebody can pull out",
          Mounts.railMenu(stick).map(function (r) { return r.label }).join(","), "Open,Unmount,Eject")
    check("with the switch off a mounted stick offers exactly what it offered in 0.2.1",
          Mounts.railMenu({ group: "device", kind: "volume", mounted: true, removable: true }).map(function (r) { return r.label }).join(","),
          "Eject")
    check("and an unmounted one opens no menu at all",
          Mounts.railMenu({ group: "device", kind: "volume", mounted: false, removable: true }).length, 0)

    // Captured live on this box on 2026-09-24: udisksctl loop-setup on an ext4 image automounts it
    // at /run/media/<user>, exactly where Nautilus then lists it, and lsblk reports the loop itself
    // as the mounted leaf. The idle loop in tests/js/devices.js's live listing stays off the rail,
    // because only a mount earns a pseudo disk its walk.
    var loopBox = '{"blockdevices":[{"name":"loop0","path":"/dev/loop0","label":"FLEATEST","mountpoints":["/run/media/gm/FLEATEST"],"rm":false,"tran":null,"size":67108864,"type":"loop","model":null,"fstype":"ext4","parttypename":null}]}'
    var image = Devices.parseDevices(loopBox, false)
    check("a mounted disk image is a rail row, on either switch", labels(image), "FLEATEST")
    check("its device is the loop itself and its path is where udisks put it",
          image[0].device + "|" + image[0].path, "/dev/loop0|/run/media/gm/FLEATEST")
    check("it is attached, which is what carries its release", image[0].attached, true)
    check("and it is not removable: eject is for a drive somebody pulls out", image[0].removable, false)

    // A VeraCrypt file container is that loop with a crypt leaf on it: the loop half is the live
    // capture above, the crypt leaf the dm shape tests/js/devices.js's live listing already records.
    var veracrypt = '{"blockdevices":[{"name":"loop1","path":"/dev/loop1","label":null,"mountpoints":[null],"rm":false,"tran":null,"size":67108864,"type":"loop","model":null,"fstype":null,"parttypename":null,'
                  + '"children":[{"name":"veracrypt1","path":"/dev/mapper/veracrypt1","label":"SECRETS","mountpoints":["/mnt/veracrypt1"],"rm":false,"size":66846720,"type":"crypt","model":null,"fstype":"ext4","parttypename":null}]}]}'
    var vc = Devices.parseDevices(veracrypt, false)
    check("an unlocked VeraCrypt container is a rail row, without any switch", labels(vc), "SECRETS")
    check("its row is the crypt mapping, which is the device gio acts on",
          vc[0].device + "|" + vc[0].attached, "/dev/mapper/veracrypt1|true")
    check("and the loop carries that wherever VeraCrypt mounted it, /mnt/veracrypt1 included",
          RailMenu.releaseMark(entry(vc[0])), "unmountVolume")

    // The dismounted half of both: an attached loop nobody mounted is plumbing rather than a place,
    // on either switch, which also keeps a squashfs loop farm off the rail on a box that grows one.
    var idle = '{"blockdevices":[{"name":"loop2","path":"/dev/loop2","label":"FLEATEST","mountpoints":[null],"rm":false,"tran":null,"size":67108864,"type":"loop","model":null,"fstype":"ext4","parttypename":null}]}'
    check("an idle loop is not a row", Devices.parseDevices(idle, false).length, 0)
    check("and rule 1's switch does not surface it either", Devices.parseDevices(idle, true).length, 0)

    // A crypt leaf on a real partition is attached only where udisks mounted an interactive unlock:
    // /home on a second encrypted disk is crypttab's boot-time mapping, and giving it the row's
    // release mark is the finding CodeRabbit raised on PR 200, so it keeps its Unmount in the menu.
    var innerCrypt = '{"blockdevices":[{"name":"sdc","path":"/dev/sdc","label":null,"mountpoints":[null],"rm":false,"tran":null,"size":2000398934016,"type":"disk","model":"Samsung SSD 870","fstype":null,"parttypename":null,'
                   + '"children":[{"name":"sdc1","path":"/dev/sdc1","label":null,"mountpoints":[null],"rm":false,"size":2000398934016,"type":"part","model":null,"fstype":"crypto_LUKS","parttypename":"Linux filesystem",'
                   + '"children":[{"name":"home","path":"/dev/mapper/home","label":"home","mountpoints":["/home"],"rm":false,"size":2000380000256,"type":"crypt","model":null,"fstype":"ext4","parttypename":null}]}]}]}'
    var vault = Devices.parseDevices(innerCrypt, false)
    check("a boot-unlocked LUKS home is a row that is not attached",
          vault.length === 1 ? vault[0].label + "|" + vault[0].attached : labels(vault), "home|false")
    check("so it draws no release mark, keeping its unmount in the menu and Ctrl+E",
          vault.length === 1 ? RailMenu.releaseMark(entry(vault[0])) : "no row", "")
    var unlockedCrypt = innerCrypt.replace('"mountpoints":["/home"]', '"mountpoints":["/run/media/gm/home"]')
    var unlocked = Devices.parseDevices(unlockedCrypt, false)
    check("the same mapping udisks mounted for an interactive unlock is attached",
          unlocked.length === 1 ? unlocked[0].attached + "|" + RailMenu.releaseMark(entry(unlocked[0])) : labels(unlocked),
          "true|unmountVolume")

    // What attached carries: the open and the release on the row's menu, no Extras switch asked.
    var attached = { group: "device", kind: "volume", mounted: true, removable: false, attached: true }
    check("a mounted attached volume offers its open and its release",
          Mounts.railMenu(attached).map(function (r) { return r.label + ":" + r.action }).join(","),
          "Open:openVolume,Unmount:unmountVolume")

    // Issue 76: the release a row offers is drawn on the row, and releaseOf is Ctrl+E's own pick,
    // so a menu that leads with Open can never turn the release key into a second Enter.
    check("the mark on a mounted stick is its eject", RailMenu.releaseMark(stick), "eject")
    check("the mark on an attached volume is its unmount", RailMenu.releaseMark(attached), "unmountVolume")
    check("a mounted share draws its unmount",
          RailMenu.releaseMark({ group: "network", kind: "share", uri: "smb://nas/isos/", mounted: true }), "unmount")
    check("an NFS export keeps no visible release, the issue's own carve-out",
          RailMenu.releaseMark({ group: "network", kind: "share", uri: "nfs://nas/export/", mounted: true }), "")
    check("while its menu and Ctrl+E still offer the unmount",
          RailMenu.releaseOf({ group: "network", kind: "share", uri: "nfs://nas/export/", mounted: true }), "unmount")
    check("a mounted phone draws its unmount",
          RailMenu.releaseMark({ group: "device", kind: "phone", uri: "mtp://x/", mounted: true }), "unmountPhone")
    check("an unmounted row draws nothing", RailMenu.releaseMark(unmounted), "")
    check("the internal disk draws nothing", RailMenu.releaseMark({ group: "device", kind: "disk", mounted: true, path: "/" }), "")
    check("a mounted internal volume outside the switch draws nothing",
          RailMenu.releaseMark({ group: "device", kind: "volume", mounted: true, removable: false }), "")
    // Measured live before this cut: with rule 1's switch on, /home and every fstab sibling drew the
    // mark, because their menus carry Unmount. That release stays in the menu, and off the row's edge.
    check("and inside the switch it still draws nothing, keeping its unmount in the menu",
          RailMenu.releaseMark(mounted), "")
    check("Ctrl+E picks the release out of a menu that leads with Open", RailMenu.releaseOf(mounted), "unmountVolume")
    check("and prefers the eject when the row has one", RailMenu.releaseOf(stick), "eject")

    // The poll must redraw a row whose attachment changed, so sameEntries reads the flag.
    check("a row that gained its attachment does not compare equal",
          Mounts.sameEntries([attached], [{ group: "device", kind: "volume", mounted: true, removable: false }]), false)
}

