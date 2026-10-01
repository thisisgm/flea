.import "../../ui/js/Devices.js" as Devices

// A loop-mounted .iso in the DEVICES rail: the one volume whose lsblk "rm" flag is never true
// even though it is as browsable as a stick's. tests/js/devices.js keeps the removable and
// system-disk cases.

function run(check) {
    // A .iso opened from the file list: gio loop-mounts it, giving it a partition table lsblk
    // reports the same way it would a real disk's, except "rm" is false throughout and the loop
    // device's own node is never itself mounted, only a partition under it is. Captured live from
    // lsblk after opening an Omarchy installer .iso, with the mount path made generic.
    var mountedIso = '{"blockdevices":['
                    + '{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G",'
                    + '"children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":256060514304,"type":"part","model":null}]},'
                    + '{"name":"loop0","path":"/dev/loop0","label":null,"mountpoints":[null],"rm":false,"size":6224838656,"type":"loop","model":null,'
                    + '"children":[{"name":"loop0p1","path":"/dev/loop0p1","label":"OMARCHY_202608","mountpoints":["/run/media/user/OMARCHY_202608"],"rm":false,"size":6224838656,"type":"part","model":null},'
                    + '{"name":"loop0p2","path":"/dev/loop0p2","label":null,"mountpoints":[null],"rm":false,"size":24117248,"type":"part","model":null}]}'
                    + ']}'
    var iso = Devices.parseDevices(mountedIso)
    check("the mounted loop partition is a device row despite rm being false", iso.length, 2)
    check("the mounted loop partition takes the filesystem label", iso[1].label, "OMARCHY_202608")
    check("the mounted loop partition carries its device node for gio", iso[1].device, "/dev/loop0p1")
    check("the mounted loop partition carries the mountpoint lsblk reported", iso[1].path, "/run/media/user/OMARCHY_202608")
    check("the mounted loop partition reads as mounted", iso[1].mounted, true)
    check("the loop device's own row is not duplicated", iso.map(function (e) { return e.label }).join(","), "nvme0n1,OMARCHY_202608")
    check("the loop device's unmounted second partition is not a row", iso.some(function (e) { return e.device === "/dev/loop0p2" }), false)
    check("a mounted loop partition is not marked removable, since rm never goes true for one", iso[1].removable, false)
    // DeviceMounts.qml's eject() reads this to route a loop row to gio mount -u instead of -e:
    // -e hangs indefinitely on a loop volume (measured live), because it never carries a Drive
    // object the way a real disk's partition does, and gio's eject path waits on one regardless.
    check("the mounted loop partition is marked loop for eject to route around", iso[1].loop, true)

    // An entirely unmounted loop device (systemd, some package manager, ...) stays out, same as
    // before: "mounted" is the only signal that tells a browsable loop apart from one that has
    // nothing to do with anyone browsing files.
    var bareLoop = '{"blockdevices":['
                  + '{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G",'
                  + '"children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":256060514304,"type":"part","model":null}]},'
                  + '{"name":"loop0","path":"/dev/loop0","label":"FLEATEST","mountpoints":[null],"rm":false,"size":67108864,"type":"loop","model":null}'
                  + ']}'
    check("an unmounted bare loop device is still not a device row", Devices.parseDevices(bareLoop).length, 1)
}
