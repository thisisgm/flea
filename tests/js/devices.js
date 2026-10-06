.import "../../ui/js/Devices.js" as Devices
.import "../../ui/js/Eject.js" as Eject
.import "../../ui/js/Mounts.js" as Mounts

// The DEVICES half of the rail: one lsblk listing turned into rows, and the verdict an eject earns
// from the listing that follows it. Split out of tests/js/mounts.js with ui/js/Devices.js, which
// keeps the gvfs half and the rail's shared vocabulary.

function run(check) {
    // minipc's own listing, taken live on 2026-09-11 with one stick in: the encrypted root carries
    // four mountpoints including /, which is why the parser reads MOUNTPOINTS and not MOUNTPOINT.
    // The plain column showed /home for that device and never /, so nothing could name the system disk.
    var live = '{"blockdevices":['
             + '{"name":"loop0","path":"/dev/loop0","label":"FLEATEST","mountpoints":[null],"rm":false,"size":67108864,"type":"loop","model":null},'
             + '{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":124656812032,"type":"disk","model":"USB Flash Disk",'
             + '"children":[{"name":"sda1","path":"/dev/sda1","label":"128GB","mountpoints":["/run/media/gm/128GB"],"rm":true,"size":124656812032,"type":"part","model":null}]},'
             + '{"name":"zram0","path":"/dev/zram0","label":"zram0","mountpoints":["[SWAP]"],"rm":false,"size":20724056064,"type":"disk","model":null},'
             + '{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G",'
             + '"children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/boot"],"rm":false,"size":2147483648,"type":"part","model":null},'
             + '{"name":"nvme0n1p2","path":"/dev/nvme0n1p2","label":null,"mountpoints":[null],"rm":false,"size":253910581248,"type":"part","model":null,'
             + '"children":[{"name":"root","path":"/dev/mapper/root","label":null,"mountpoints":["/home","/var/log","/"],"rm":false,"size":253893804032,"type":"crypt","model":null}]}]}'
             + ']}'
    var d = Devices.parseDevices(live)
    check("the live box has one internal disk and one removable volume", d.length, 2)
    check("the internal disk sorts first", d[0].kind, "disk")
    check("the internal disk is named by its kernel name", d[0].label, "nvme0n1")
    check("the internal disk row opens the root of the filesystem", d[0].path, "/")
    check("the internal disk is always mounted", d[0].mounted, true)
    check("the internal disk retains raw capacity bytes", d[0].size, 256060514304)
    check("zram is not the internal disk", d[0].device, "/dev/nvme0n1")
    check("the removable volume takes the filesystem label", d[1].label, "128GB")
    check("the removable volume carries its device node for gio", d[1].device, "/dev/sda1")
    check("a mounted volume carries the mountpoint lsblk reported", d[1].path, "/run/media/gm/128GB")
    check("a volume with a mountpoint reads as mounted", d[1].mounted, true)
    check("a removable volume is marked removable, which is what offers it Eject", d[1].removable, true)
    check("a loop device is not a device row", d.map(function (e) { return e.label }).join(","), "nvme0n1,128GB")
    check("a real removable volume is not marked loop", d[1].loop, false)

    // The system disk is the one place the walk never goes: /boot and /home are the box's own
    // plumbing, and the disk row above already stands for that drive.
    check("no partition of the system disk becomes a row of its own",
          d.map(function (e) { return e.path }).join(","), "/,/run/media/gm/128GB")

    // The operator's defect, 2026-09-11: only USB sticks appeared in DEVICES. A second internal drive
    // is mounted and not removable, and the old parser emitted a volume row only for a removable one.
    var twoDisks = '{"blockdevices":['
                 + '{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G",'
                 + '"children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":256060514304,"type":"part","model":null}]},'
                 + '{"name":"sdb","path":"/dev/sdb","label":null,"mountpoints":[null],"rm":false,"size":2000398934016,"type":"disk","model":"Samsung SSD 870",'
                 + '"children":[{"name":"sdb1","path":"/dev/sdb1","label":"Vault","mountpoints":["/mnt/vault"],"rm":false,"size":2000398934016,"type":"part","model":null}]}'
                 + ']}'
    var two = Devices.parseDevices(twoDisks)
    check("a second internal drive is a row, which it was not before 0.2.1", two.length, 2)
    check("and it is the mounted volume, not the whole disk", two[1].label, "Vault")
    check("it opens where it is mounted", two[1].path, "/mnt/vault")
    check("it reads as mounted", two[1].mounted, true)
    check("and it is not removable, so nothing offers to eject a fixed disk", two[1].removable, false)
    check("a fixed internal volume is offered no release at all",
          Mounts.railMenu({ group: "device", kind: "volume", mounted: true, removable: false }).length, 0)

    // Which disk is "the" disk was guessed as the first non-removable one, so a second internal drive
    // listed ahead of the system disk took the / row and the real system disk got none.
    var reordered = '{"blockdevices":['
                  + '{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":false,"size":2000398934016,"type":"disk","model":"Data",'
                  + '"children":[{"name":"sda1","path":"/dev/sda1","label":"Data","mountpoints":["/mnt/data"],"rm":false,"size":2000398934016,"type":"part","model":null}]},'
                  + '{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G",'
                  + '"children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":256060514304,"type":"part","model":null}]}'
                  + ']}'
    var order = Devices.parseDevices(reordered)
    check("the disk row is the one that carries /, not the one lsblk listed first", order[0].device, "/dev/nvme0n1")
    check("and the other internal drive is a volume row beside it", order[1].path, "/mnt/data")

    // PR 74 (mariobgsp): a USB bridge reports rm=false on the disk and tran=null on its partition,
    // so removability read off RM alone offered no Eject for a drive you unplug by hand. The
    // listing half was already answered by the rule above, which lists every mounted volume; this
    // is the release the row was missing. Shape captured from the PR's own WD My Passport.
    var bridge = '{"blockdevices":['
               + '{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"tran":"nvme","size":256060514304,"type":"disk","model":"KBG40ZNS256G",'
               + '"children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"tran":null,"size":256060514304,"type":"part","model":null}]},'
               + '{"name":"sdb","path":"/dev/sdb","label":null,"mountpoints":[null],"rm":false,"tran":"usb","size":2000398934016,"type":"disk","model":"My Passport 25E2",'
               + '"children":[{"name":"sdb1","path":"/dev/sdb1","label":"Passport","mountpoints":["/run/media/gm/Passport"],"rm":false,"tran":null,"size":2000398934016,"type":"part","model":null,"fstype":"vfat"}]}'
               + ']}'
    var usb = Devices.parseDevices(bridge)
    check("a mounted USB bridge is a rail row", usb.length + "|" + usb[1].label, "2|Passport")
    check("and it is removable however its RM column reads, because it unplugs", usb[1].removable, true)
    // The parsed row itself, not a hand-built one: the menu reads group, kind and mounted too.
    check("so the rail offers it the release a stick gets",
          Mounts.railMenu(Object.assign({ group: "device" }, usb[1]))
                .filter(function (r) { return r.separator !== true }).map(function (r) { return r.label }).join(","), "Open,Eject")
    // The same drive with nothing mounted is still a row, the way an unplugged stick is.
    check("an unmounted USB bridge is a row too",
          Devices.parseDevices(bridge.replace('["/run/media/gm/Passport"]', "[null]")).length, 2)
    // And the transport never makes the box's own disk removable: nvme is not usb.
    check("the system disk is not offered an eject", usb[0].removable, false)
    // The transport reaches a partition and stops: an unlocked crypt leaf under a USB disk reads as
    // it did before PR 74, because what an eject would release there is the drive and not the map.
    var lockedUsb = '{"blockdevices":['
                  + '{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":["/"],"rm":false,"tran":"nvme","size":256060514304,"type":"disk","model":"KBG40ZNS256G"},'
                  + '{"name":"sdb","path":"/dev/sdb","label":null,"mountpoints":[null],"rm":false,"tran":"usb","size":8589934592,"type":"disk","model":"Vault Drive",'
                  + '"children":[{"name":"sdb1","path":"/dev/sdb1","label":null,"mountpoints":[null],"rm":false,"tran":null,"size":8589934592,"type":"part","model":null,'
                  + '"children":[{"name":"luks-vault","path":"/dev/mapper/luks-vault","label":"vault","mountpoints":["/run/media/gm/vault"],"rm":false,"tran":null,"size":8589934592,"type":"crypt","model":null}]}]}'
                  + ']}'
    var locked = Devices.parseDevices(lockedUsb)
    check("an unlocked volume on a USB drive is the one row, as it always was",
          locked.length + "|" + locked[locked.length - 1].label, "2|vault")
    check("and it is not marked removable, so the transport stopped at the partition",
          locked[locked.length - 1].removable, false)

    // An internal partition nothing mounted stays out: a spare EFI or recovery partition is not a
    // place to browse, and Flea offers no way to mount one.
    var spare = '{"blockdevices":['
              + '{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G",'
              + '"children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":256060514304,"type":"part","model":null}]},'
              + '{"name":"sdb","path":"/dev/sdb","label":null,"mountpoints":[null],"rm":false,"size":2000398934016,"type":"disk","model":"Spare",'
              + '"children":[{"name":"sdb1","path":"/dev/sdb1","label":"RECOVERY","mountpoints":[null],"rm":false,"size":2000398934016,"type":"part","model":null}]}'
              + ']}'
    check("an unmounted internal partition is not a row", Devices.parseDevices(spare).length, 1)

    // Swap is not a mountpoint anyone can open, so a swap partition on a second drive is not a row
    // either, even though lsblk lists "[SWAP]" in the same column as a real path.
    var swapDisk = '{"blockdevices":['
                 + '{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G",'
                 + '"children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":256060514304,"type":"part","model":null}]},'
                 + '{"name":"sdb","path":"/dev/sdb","label":null,"mountpoints":[null],"rm":false,"size":17179869184,"type":"disk","model":"Swap",'
                 + '"children":[{"name":"sdb1","path":"/dev/sdb1","label":null,"mountpoints":["[SWAP]"],"rm":false,"size":17179869184,"type":"part","model":null}]}'
                 + ']}'
    check("a swap partition on another drive is not a row", Devices.parseDevices(swapDisk).length, 1)

    // glib's exact system list carries /run (GLIB_RUNSTATEDIR) and neither /efi nor /boot/efi.
    check("a volume mounted only at /run hides with the system mounts", Devices.isSystemPath("/run"), true)
    check("a volume mounted only at /boot hides with the system mounts", Devices.isSystemPath("/boot"), true)
    check("/efi is no system path, so a non-ESP volume there stays a row", Devices.isSystemPath("/efi"), false)
    check("/boot/efi is no system path either", Devices.isSystemPath("/boot/efi"), false)

    // Only a leaf is a volume. An encrypted stick lists the partition and the unlocked crypt under
    // it, and emitting both would put one drive in the rail twice.
    var lockedOpen = '{"blockdevices":[{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":8589934592,"type":"disk","model":"Stick",'
                   + '"children":[{"name":"sda1","path":"/dev/sda1","label":null,"mountpoints":[null],"rm":true,"size":8589934592,"type":"part","model":null,'
                   + '"children":[{"name":"luks-vault","path":"/dev/mapper/luks-vault","label":"vault","mountpoints":["/run/media/gm/vault"],"rm":false,"size":8589934592,"type":"crypt","model":null}]}]}]}'
    var opened = Devices.parseDevices(lockedOpen)
    check("an unlocked encrypted stick is one row, not two", opened.length, 1)
    check("and the row is the crypt, which is the thing that mounted", opened[0].label, "vault")
    check("a device-mapper volume carries the path gio can actually act on", opened[0].device, "/dev/mapper/luks-vault")

    // No devices at all: the rail self-hides on this, so it must be an empty list and never a throw.
    check("empty lsblk output parses to nothing", Devices.parseDevices("").length, 0)
    check("garbage lsblk output parses to nothing", Devices.parseDevices("not json at all\n").length, 0)
    check("valid json with no blockdevices key parses to nothing", Devices.parseDevices("{}").length, 0)
    check("an empty blockdevices array parses to nothing", Devices.parseDevices('{"blockdevices":[]}').length, 0)
    var badSizes = [undefined, null, "", "116.1G", "256060514304", -1, 1.5, Infinity, NaN, 9007199254740992]
    check("invalid capacity values stay absent rather than becoming display text", badSizes.every(function (value) {
        return Devices.deviceBytes(value) === null
    }), true)
    check("zero-byte devices retain their real numeric size", Devices.deviceBytes(0), 0)

    // Present and unmounted: the state a stick sits in on this box, which automounts nothing.
    var unmounted = '{"blockdevices":[{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":124656812032,"type":"disk","model":"USB Flash Disk",'
                  + '"children":[{"name":"sda1","path":"/dev/sda1","label":"128GB","mountpoints":[null],"rm":true,"size":124656812032,"type":"part","model":null,"fstype":"vfat"}]}]}'
    var u = Devices.parseDevices(unmounted)
    check("an unmounted stick is still a row", u.length, 1)
    check("an unmounted volume reads as unmounted", u[0].mounted, false)
    check("an unmounted volume has no path to open yet", u[0].path, "")
    check("an unmounted volume still carries the device node its mount needs", u[0].device, "/dev/sda1")
    check("an unmounted volume keeps its capacity separate from its label", u[0].size, 124656812032)

    // Issue 143's three states, with a USB disk beside the optical device as the negative control: empty rom hidden, inserted and mounted iso9660 rows retained.
    var opticalSystem = '{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G",'
                     + '"children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":256060514304,"type":"part","model":null}]},'
    var opticalUsb = '{"name":"sdb","path":"/dev/sdb","label":"USB","mountpoints":[null],"rm":true,"size":34359738368,"type":"disk","model":"USB Flash Disk","fstype":"vfat"}'
    var opticalEmpty = '{"blockdevices":[' + opticalSystem
                     + '{"name":"sr0","path":"/dev/sr0","label":null,"mountpoints":[null],"rm":true,"size":0,"type":"rom","model":"MATSHITA DVD+/-RW UJ8FB"},'
                     + opticalUsb + ']}'
    var emptyOpticalRows = Devices.parseDevices(opticalEmpty)
    check("an empty optical drive is hidden while the unmounted USB disk remains", emptyOpticalRows.map(function (e) { return e.label }).join(","), "nvme0n1,USB")
    var opticalInserted = '{"blockdevices":[' + opticalSystem
                        + '{"name":"sr0","path":"/dev/sr0","label":null,"mountpoints":[null],"rm":true,"size":0,"type":"rom","fstype":"iso9660","model":"MATSHITA DVD+/-RW UJ8FB"},'
                        + opticalUsb + ']}'
    var inserted = Devices.parseDevices(opticalInserted)
    check("an inserted optical disc remains a row", inserted.map(function (e) { return e.label }).join(","), "nvme0n1,MATSHITA DVD+/-RW UJ8FB,USB")
    check("an inserted optical disc is still unmounted", inserted[1].mounted, false)
    var opticalMounted = '{"blockdevices":[' + opticalSystem
                       + '{"name":"sr0","path":"/dev/sr0","label":null,"mountpoints":["/run/media/gm/DVD"],"rm":true,"size":0,"type":"rom","fstype":"iso9660","model":"MATSHITA DVD+/-RW UJ8FB"},'
                       + opticalUsb + ']}'
    var mountedOptical = Devices.parseDevices(opticalMounted)
    check("a mounted optical disc remains a row", mountedOptical[1].label, "MATSHITA DVD+/-RW UJ8FB")
    check("a mounted optical disc keeps its published path", mountedOptical[1].path, "/run/media/gm/DVD")

    // Several at once, and the internal disk row is one whatever the box has.
    var many = '{"blockdevices":['
             + '{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G",'
             + '"children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":256060514304,"type":"part","model":null}]},'
             + '{"name":"nvme1n1","path":"/dev/nvme1n1","label":null,"mountpoints":[null],"rm":false,"size":1000000000000,"type":"disk","model":"Second NVMe"},'
             + '{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":124656812032,"type":"disk","model":"USB Flash Disk",'
             + '"children":[{"name":"sda1","path":"/dev/sda1","label":"first","mountpoints":["/run/media/gm/first"],"rm":true,"size":62277025792,"type":"part","model":null},'
             + '{"name":"sda2","path":"/dev/sda2","label":"second","mountpoints":[null],"rm":true,"size":62277025792,"type":"part","model":null,"fstype":"vfat"}]},'
             + '{"name":"sdb","path":"/dev/sdb","label":"CARD","mountpoints":[null],"rm":true,"size":34359738368,"type":"disk","model":"SD Reader","fstype":"vfat"}'
             + ']}'
    var m = Devices.parseDevices(many)
    check("four rows come out of two sticks, the system disk and one bare internal drive", m.length, 4)
    check("only one internal disk row is ever emitted", m[0].label, "nvme0n1")
    check("an internal drive nothing mounted adds no row of its own",
          m.map(function (e) { return e.label }).join(","), "nvme0n1,first,second,CARD")
    check("the machine row carries its device capacity", m[0].size, 256060514304)
    check("both partitions of one stick are rows", m[1].label + "," + m[2].label, "first,second")
    check("an unpartitioned removable disk is a row of its own", m[3].label, "CARD")
    check("an unpartitioned removable disk carries its own device node", m[3].device, "/dev/sdb")

    // The label ladder: filesystem label, then the drive's product name, then the kernel name.
    var noLabel = '{"blockdevices":[{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":124656812032,"type":"disk","model":"USB Flash Disk",'
                + '"children":[{"name":"sda1","path":"/dev/sda1","label":null,"mountpoints":[null],"rm":true,"size":124656812032,"type":"part","model":null,"fstype":"vfat"}]}]}'
    check("an unlabelled volume falls back to the drive's product name", Devices.parseDevices(noLabel)[0].label, "USB Flash Disk")
    var noModel = '{"blockdevices":[{"name":"sdb","path":"/dev/sdb","label":null,"mountpoints":[null],"rm":true,"size":34359738368,"type":"disk","model":null,'
                + '"children":[{"name":"sdb1","path":"/dev/sdb1","label":null,"mountpoints":[null],"rm":true,"size":34359738368,"type":"part","model":null,"fstype":"vfat"}]}]}'
    check("a volume with neither label nor model falls back to the kernel name", Devices.parseDevices(noModel)[0].label, "sdb1")

    // A label is a name off somebody else's filesystem, so it is data: the parser never rewrites it
    // and ui/SidebarRow.qml draws it through Text.PlainText.
    var awkward = '{"blockdevices":[{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":8589934592,"type":"disk","model":null,'
                + '"children":[{"name":"sda1","path":"/dev/sda1","label":"Sauvegarde & Co \\"2026\\" <b>","mountpoints":[null],"rm":true,"size":8589934592,"type":"part","model":null,"fstype":"vfat"}]}]}'
    check("an awkward label survives the parse verbatim", Devices.parseDevices(awkward)[0].label, 'Sauvegarde & Co "2026" <b>')

    var longName = "Photographs and scans of every receipt from two thousand and twenty six, quarter one through quarter four"
    var longLabel = '{"blockdevices":[{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":8589934592,"type":"disk","model":null,'
                  + '"children":[{"name":"sda1","path":"/dev/sda1","label":"' + longName + '","mountpoints":[null],"rm":true,"size":8589934592,"type":"part","model":null,"fstype":"vfat"}]}]}'
    check("a very long label is elided by the row, never truncated by the parser", Devices.parseDevices(longLabel)[0].label, longName)

    // A mountpoint with a space needs no decoding here, and that is measured rather than assumed:
    // the kernel writes /tmp/.../USB\040Drive in /proc/self/mountinfo, and lsblk --json was run
    // against a real vfat mount at that path and printed "/tmp/.../USB Drive" with a literal space.
    var spaced = '{"blockdevices":[{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":8589934592,"type":"disk","model":null,'
               + '"children":[{"name":"sda1","path":"/dev/sda1","label":"USB Drive","mountpoints":["/run/media/gm/USB Drive"],"rm":true,"size":8589934592,"type":"part","model":null,"fstype":"vfat"}]}]}'
    check("a mountpoint with a space is opened verbatim", Devices.parseDevices(spaced)[0].path, "/run/media/gm/USB Drive")

    // A trust boundary: a node with no name would build "/dev/undefined" and hand it to gio.
    var noName = '{"blockdevices":[{"label":"nameless","mountpoints":[null],"rm":true,"size":8589934592,"type":"part","model":null}]}'
    check("a node with no name is not a row", Devices.parseDevices(noName).length, 0)

    // The two listings share the rail but never the parser, so a device body must not read as a
    // network mount: parseMounts anchors Mount() at column zero and lsblk emits no such line.
    check("an lsblk body yields no network mounts", Mounts.parseMounts(live).length, 0)

    // The eject verdict is read off the listing taken after gio exits, never off gio's exit code,
    // which has been 0 over a volume that stayed mounted. It judges the whole disk the device sits on.
    check("a listing that still mounts the ejected volume refuses the verdict", Eject.verdict(live, "/dev/sda1"), "mounted")
    check("a listing with the volume present but unmounted is safe", Eject.verdict(unmounted, "/dev/sda1"), "safe")
    var gone = '{"blockdevices":[{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G"}]}'
    check("a listing the device has vanished from is safe", Eject.verdict(gone, "/dev/sda1"), "safe")
    var mediaOut = '{"blockdevices":[{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":"0B","type":"disk","model":"USB Flash Disk"}]}'
    check("a stick whose media ejected but whose disk node stayed is safe", Eject.verdict(mediaOut, "/dev/sda1"), "safe")
    check("a sibling partition still mounted refuses the verdict for its unmounted neighbour", Eject.verdict(many, "/dev/sda2"), "mounted")
    var crypt = '{"blockdevices":[{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":8589934592,"type":"disk","model":null,'
              + '"children":[{"name":"sda1","path":"/dev/sda1","label":null,"mountpoints":[null],"rm":true,"size":8589934592,"type":"part","model":null,'
              + '"children":[{"name":"luks-vault","path":"/dev/mapper/luks-vault","label":"vault","mountpoints":["/run/media/gm/vault"],"rm":false,"size":8589934592,"type":"crypt","model":null}]}]}]}'
    check("a mounted crypt child under an unmounted partition refuses the verdict", Eject.verdict(crypt, "/dev/sda1"), "mounted")
    var swap = '{"blockdevices":[{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":8589934592,"type":"disk","model":null,'
             + '"children":[{"name":"sda1","path":"/dev/sda1","label":null,"mountpoints":["[SWAP]"],"rm":true,"size":8589934592,"type":"part","model":null}]}]}'
    check("active swap on the stick counts as mounted", Eject.verdict(swap, "/dev/sda1"), "mounted")
    check("garbage in place of a listing is unknown, never safe", Eject.verdict("not json at all\n", "/dev/sda1"), "unknown")
    check("an empty body is unknown, never safe", Eject.verdict("", "/dev/sda1"), "unknown")
    check("json with no blockdevices key is unknown, never safe", Eject.verdict("{}", "/dev/sda1"), "unknown")
    check("a listing with no block devices at all is unknown, never safe", Eject.verdict('{"blockdevices":[]}', "/dev/sda1"), "unknown")
    check("an empty device name is unknown, never safe", Eject.verdict(live, ""), "unknown")

    // Only the safe verdict may say safe; the other two tell the user to leave the stick in.
    check("the safe verdict earns the unplug sentence", Eject.sentence("safe", "128GB").text, "Ejected 128GB, it is safe to unplug.")
    check("the safe verdict is not an error", Eject.sentence("safe", "128GB").isError, false)
    check("the mounted verdict with the row itself still mounted says try again", Eject.sentence("mounted", "128GB", []).text, "128GB is still mounted; close what is using it.")
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

    // Defect 7: the mount wait starts when gio exits 0, never at launch, so a slow polkit
    // prompt is a slow prompt and not a mount that never reported a folder to open.
    check("an exited gio starts the wait", Devices.mountTimerStart(0), true)
    check("a refused gio never starts it", Devices.mountTimerStart(1), false)
    check("a killed gio never starts it", Devices.mountTimerStart(137), false)

    // Defect 9: a power-off unmounts every mounted volume on the disk before stopping it.
    var vols = [{ kind: "volume", device: "/dev/sda1", path: "/run/media/u/a", mounted: true },
                { kind: "volume", device: "/dev/sda2", path: "/run/media/u/b", mounted: true },
                { kind: "volume", device: "/dev/sdb1", path: "/run/media/u/c", mounted: true },
                { kind: "volume", device: "/dev/sda3", path: "", mounted: false }]
    check("both mounted volumes on the disk queue", Devices.powerOffQueue(vols, "/dev/sda").join(","), "/dev/sda1,/dev/sda2")
    check("the other disk queues nothing", Devices.powerOffQueue(vols, "/dev/sdb").join(","), "/dev/sdb1")
    check("no entries queue nothing", Devices.powerOffQueue(null, "/dev/sda").join(","), "")
    check("a mountpoint comes from the last listing", Devices.mountpointOf(vols, "/dev/sda2"), "/run/media/u/b")
    check("an unlisted device has no mountpoint", Devices.mountpointOf(vols, "/dev/sdz9"), "")
    // The eject chain watches the disk's write counter, so a flushing leg restarts its deadline.
    check("a disk path answers its sysfs block name", Devices.sysBase("/dev/sda"), "sda")
    check("an nvme disk answers whole", Devices.sysBase("/dev/nvme0n1"), "nvme0n1")
    check("a non-device answers nothing", Devices.sysBase(""), "")
    check("a stat read answers its written sectors", Devices.writtenSectors("   1 0 2 3 0 0 42 0 0 0 0 0 0 0 0"), "42")
    check("a short read answers nothing", Devices.writtenSectors("1 2 3"), "")
}
