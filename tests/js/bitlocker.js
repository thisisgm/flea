.import "../../ui/js/Devices.js" as Devices
.import "../../ui/js/Unlock.js" as Unlock
.import "../../ui/js/Menu.js" as Menu

function run(check) {
    var types = ["BitLocker", "bitlocker", "bItLoCkEr", "BITLOCKER", "crypto_LUKS", "CRYPTO_luks", "crypto_plain"]
    for (var i = 0; i < types.length; i++) {
        var n = {name: "nvme1n1p3", path: "/dev/nvme1n1p3", label: "Backup", mountpoints: [null],
                 rm: false, size: 500000000000, type: "part", fstype: types[i]}
        var body = JSON.stringify({blockdevices: [n]})
        check(types[i] + " is locked", Devices.isLockedNode(n), true)
        check(types[i] + " container name", Devices.containerName(n), "Backup")
        for (var setting = 0; setting < 2; setting++) {
            var rows = Devices.parseDevices(body, !!setting)
            check(types[i] + " row, setting=" + setting, rows.length, 1)
            if (rows.length) {
                check(types[i] + " locked rail row", rows[0].locked, true)
                check(types[i] + " label", rows[0].label, "Backup")
                check(types[i] + " size", rows[0].size, 500000000000)
                rows[0].group = "device"
                check(types[i] + " menu offers Unlock", Menu.availableRail({id: "unlockVolume"}, rows[0]), true)
                check(types[i] + " menu never offers Mount", Menu.availableRail({id: "mountVolume"}, rows[0]), false)
            }
        }
        n.parttype = "de94bba4-06d1-4d40-a16a-bfd50179d6ac"
        check(types[i] + " hidden exception", Devices.hidden(n), false)
        delete n.parttype
        n.mountpoints = ["/media/direct"]
        check(types[i] + " mounted container is not locked", Devices.isLockedNode(n), false)
        n.mountpoints = [null]
        n.children = [{name: "cleartext", path: "/dev/mapper/cleartext", type: "crypt", label: "Child label",
                       fstype: "ntfs", mountpoints: ["/media/Backup"]}]
        body = JSON.stringify({blockdevices: [n]})
        rows = Devices.parseDevices(body, true)
        check(types[i] + " child only", rows.length, 1)
        check(types[i] + " child label", rows[0].label, "Backup")
        check(types[i] + " child not locked", rows[0].locked, false)
        rows[0].group = "device"
        check(types[i] + " child never offers Unlock", Menu.availableRail({id: "unlockVolume"}, rows[0]), false)
        check(types[i] + " backing", Unlock.backingFor(body, "/dev/mapper/cleartext"), "/dev/nvme1n1p3")
        if (i < 4)
            check(types[i] + " card name", Unlock.cryptoName(types[i], "2"), "BitLocker")
    }
    check("LUKS2 unchanged", Unlock.cryptoName("crypto_LUKS", "2"), "LUKS2")
    check("other crypto unchanged", Unlock.cryptoName("crypto_plain", ""), "PLAIN")
    for (var j = 0; j < 5; j++) {
        var plain = {fstype: ["ntfs", "", "BitLocker2", "preBitLocker", "crypto"][j]}
        check("plain classification " + j, Devices.isLockedNode(plain), false)
        check("plain holder " + j, Devices.containerName(plain), "")
    }
}
