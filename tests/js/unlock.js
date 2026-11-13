.import "../../ui/js/Unlock.js" as Unlock
.import "../../ui/js/Devices.js" as Devices
.import "../../ui/js/Mounts.js" as Mounts
.import "../../ui/js/Eject.js" as Eject
.import "sourcefixture.js" as Source

// Unlock (U039): locked LUKS rows, the dialog's own sentences, and the udisks output the dialog parses.
function run(check) {
    check("the dialog titles itself for the drive", Unlock.title("Backup"), "Unlock Backup")
    check("unlocking names the drive with an ellipsis", Unlock.unlockingNote("Backup"), "Unlocking Backup…")
    check("a wrong passphrase names the drive", Unlock.wrongNote("Backup"), "That passphrase did not unlock Backup.")
    check("success names the drive", Unlock.unlockedNote("Backup"), "Unlocked and mounted Backup.")
    check("a lock that fails names the drive", Unlock.lockNote("Backup"), "Backup was unmounted but could not be locked; close what is using it.")
    check("the mapped device is read off the unlock line",
        Unlock.parseUnlocked("Unlocked /dev/sda1 as /dev/dm-0."), "/dev/dm-0")
    check("the mountpoint is read off the mount line",
        Unlock.parseMounted("Mounted /dev/dm-0 at /run/media/gm/Backup."), "/run/media/gm/Backup")
    check("a label with a space keeps its mountpoint",
        Unlock.parseMounted("Mounted /dev/dm-0 at /run/media/gm/My Backup."), "/run/media/gm/My Backup")
    check("an unlock without a mount names the drive", Unlock.mountNote("Backup"), "Backup was unlocked but could not be mounted.")
    check("a refused unlock names the drive", Unlock.unlockFailedNote("Backup"), "Backup could not be unlocked.")
    check("the name row joins all three parts", Unlock.subtitle("Backup", "500.0 GB", "LUKS2"), "Backup · 500.0 GB · LUKS2")
    check("the name row without a size joins two", Unlock.subtitle("Backup", "", "LUKS2"), "Backup · LUKS2")
    check("the name row alone is the label", Unlock.subtitle("Backup", "", ""), "Backup")
    check("a LUKS2 container names its version", Unlock.cryptoName("crypto_LUKS", "2"), "LUKS2")
    check("a LUKS1 container names its version", Unlock.cryptoName("crypto_LUKS", "1"), "LUKS1")
    check("a LUKS container without a version reads as LUKS", Unlock.cryptoName("crypto_LUKS", ""), "LUKS")
    check("another crypto type keeps its name", Unlock.cryptoName("crypto_plain", "2"), "PLAIN")
    check("the card names BitLocker as Windows does", Unlock.cryptoName("BitLocker", "2"), "BitLocker")
    check("the D-Bus error name is read off the daemon line",
        Unlock.parseDbusError("Error unlocking /dev/sdb1: GDBus.Error:org.freedesktop.UDisks2.Error.Failed: x"), "org.freedesktop.UDisks2.Error.Failed")
    check("translated prose around the name changes nothing",
        Unlock.parseDbusError("Fehler beim Entsperren: GDBus.Error:org.freedesktop.UDisks2.Error.Failed: y"), "org.freedesktop.UDisks2.Error.Failed")
    check("another error name is answered as itself",
        Unlock.parseDbusError("GDBus.Error:org.freedesktop.UDisks2.Error.NotAuthorized: z"), "org.freedesktop.UDisks2.Error.NotAuthorized")
    check("output without a name answers empty", Unlock.parseDbusError("boom"), "")
    check("the key reaches udisksctl through /dev/stdin, since it opens --key-file as a path",
        Unlock.unlockArgv("/dev/sdb1").join(" "), "udisksctl unlock --block-device /dev/sdb1 --key-file /dev/stdin")
    check("only the daemon's Failed reads as a wrong passphrase",
        Unlock.refusalNote("Error unlocking /dev/sda1: GDBus.Error:org.freedesktop.UDisks2.Error.Failed: Incorrect passphrase.", "Backup"),
        "That passphrase did not unlock Backup.")
    check("a local failure with no daemon error is never called a wrong passphrase",
        Unlock.refusalNote("Error unlocking /dev/sda1: Failed to open file \u201c-\u201d: No such file or directory", "Backup"),
        "Backup could not be unlocked.")
    check("a refused authorization could not unlock",
        Unlock.refusalNote("GDBus.Error:org.freedesktop.UDisks2.Error.NotAuthorized: z", "Backup"), "Backup could not be unlocked.")

    // Sample lsblk body: one internal disk plus a locked LUKS stick beside it.
    var locked = '{"blockdevices":['
        + '{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G",'
        + '"children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":256060514304,"type":"part","model":null}]},'
        + '{"name":"sdb","path":"/dev/sdb","label":null,"mountpoints":[null],"rm":true,"size":500000000000,"type":"disk","model":"Vault Drive",'
        + '"children":[{"name":"sdb1","path":"/dev/sdb1","label":"Backup","mountpoints":[null],"rm":true,"size":500000000000,"type":"part","fstype":"crypto_LUKS","fsver":"2","model":null}]}'
        + ']}'
    var rows = Devices.parseDevices(locked)
    check("a locked container on a stick is a rail row", rows.length, 2)
    check("the locked row carries its label", rows[1].label, "Backup")
    check("the locked row carries its device node", rows[1].device, "/dev/sdb1")
    check("the locked row reads as locked", rows[1].locked, true)
    check("the locked row carries lsblk's FSVER", rows[1].fsver, "2")
    check("the locked row is not mounted", rows[1].mounted, false)

    // A BitLocker partition on an internal drive: lsblk reports fstype BitLocker, not crypto_*.
    var bitlocker = '{"blockdevices":[{"name":"nvme1n1","path":"/dev/nvme1n1","label":null,"mountpoints":[null],"rm":false,"size":4000787030016,"type":"disk","model":"Samsung SSD 990 PRO 4TB",'
        + '"children":[{"name":"nvme1n1p3","path":"/dev/nvme1n1p3","label":"Windows","mountpoints":[],"rm":false,"size":242872221696,"type":"part","model":null,'
        + '"fstype":"BitLocker","fsver":"2","parttypename":"Microsoft basic data","parttype":"ebd0a0a2-b9e5-4433-87c0-68b6b72699c7"}]}]}'
    var bitRows = Devices.parseDevices(bitlocker).filter(function (r) { return r.device === "/dev/nvme1n1p3" })
    check("a BitLocker partition reads as locked", bitRows.length === 1 && bitRows[0].locked, true)

    // tests/ui-captures-unlock.sh's lsblk shim: the lane's loop container as cap-loop0 with rm set, since any loop* name is skipped.
    var loopLocked = '{"blockdevices":[{"name":"%N","path":"/dev/loop0","label":null,"mountpoints":[null],"rm":%R,'
        + '"tran":null,"size":33554432,"type":"loop","model":null,"fstype":"crypto_LUKS"}]}'
    rows = Devices.parseDevices(loopLocked.replace("%N", "cap-loop0").replace("%R", "true"))
    check("the shimmed loop container is one locked row", rows.length === 1 && rows[0].locked, true)
    check("the shimmed loop row keeps the real device node", rows[0].device, "/dev/loop0")
    check("the shimmed loop row is labelled by its shimmed name", rows[0].label, "cap-loop0")
    check("a loop name stays no row even when removable", Devices.parseDevices(loopLocked.replace("%N", "loop0").replace("%R", "true")).length, 0)

    // Unlock critic F1: the board's container keeps its name once unlocked, so the row and the footer say one name.
    var named = '{"blockdevices":[{"name":"sdb","path":"/dev/sdb","label":null,"mountpoints":[null],"rm":true,"size":500000000000,"type":"disk","model":"Vault Drive",'
        + '"children":[{"name":"sdb1","path":"/dev/sdb1","label":"Backup","mountpoints":[null],"rm":true,"size":500000000000,"type":"part","fstype":"crypto_LUKS",'
        + '"children":[{"name":"luks-1","path":"/dev/mapper/luks-1","label":"S1LUKS","mountpoints":[%M],"rm":false,"size":499983810560,"type":"crypt","fstype":"ext4"}]}]}]}'
    var unlocked = Devices.parseDevices(named.replace("%M", '"/run/media/gm/S1LUKS"'))
    check("an unlocked named container keeps the locked row's name and opens its own folder",
        unlocked.map(function (r) { return r.label + "|" + r.path + "|" + r.locked }).join(","), "Backup|/run/media/gm/S1LUKS|false")
    check("mid-unlock, before the mount, the row already reads the container's name",
        Devices.parseDevices(named.replace("%M", "null"), true).map(function (r) { return r.label }).join(","), "Backup")
    check("an unnamed container's unlocked volume reads its own label, as it did",
        Devices.parseDevices(named.replace('"label":"Backup"', '"label":null').replace("%M", '"/run/media/gm/S1LUKS"'))[0].label, "S1LUKS")

    // Unlock critic F3: flea --unlock names the card from the rail's locked row for that device, once the rail lists it.
    var rail = [{ label: "Backup", device: "/dev/sdb1", locked: false, size: 1 }, { label: "Backup", device: "/dev/loop0", locked: true, size: 33554432, fstype: "crypto_LUKS" }]
    check("the CLI device finds its locked row", Unlock.lockedRow(rail, "/dev/loop0").size, 33554432)
    check("an unlocked row of the same device is never the card's name", Unlock.lockedRow([rail[0]], "/dev/sdb1"), null)
    check("a rail that has not listed the device yet names nothing", Unlock.lockedRow([], "/dev/loop0"), null)
    var host = Source.source("ui/UnlockHost.qml")
    check("the host names the CLI card on each rail rebuild",
        Source.slice(host, "target: root.sidebar", "    }").indexOf("root.nameDevice()") >= 0, true)

    // Unlock critic F8 (ui:cap_unlock proves it natively): Tab walks field, Cancel, Unlock. No property keeps the passphrase (tests/unlock-stdin.qml).
    var dialog = Source.source("ui/UnlockDialog.qml")
    check("a card the user has typed in keeps its name", Source.slice(host, "    function cardTouched()", "    // Only an untouched card").indexOf("unlockDialog.masked") >= 0
        && Source.slice(host, "    function nameDevice()", "    // After an unlock").indexOf("root.cardTouched()") >= 0, true)
    check("the field's Tab steps the card's own order", /onTabbed: function \(from, back\) \{ root\.stepTab\(back\) \}/.test(dialog), true)
    check("the order is field, Cancel, Unlock", dialog.indexOf("[field.input, cancelButton, unlockButton]") >= 0, true)
    check("while unlocking, Cancel is the one stop", dialog.indexOf('root.phase === "unlocking" ? [cancelButton] :') >= 0, true)
    check("Unlock drops out with the field while unlocking", Source.slice(dialog, "                        id: unlockButton", "                    }").indexOf('enabled: root.phase !== "unlocking"') >= 0, true)

    // Cancel takes the keyboard while Unlocking, and no button activates on a held key's repeats.
    var submit = Source.slice(dialog, "    function submit()", "    function _startQueued()")
    var cancelSlice = Source.slice(dialog, "                        id: cancelButton", "                    }")
    check("submit moves the keyboard to Cancel while Unlocking",
        submit.indexOf("cancelButton.forceActiveFocus()") >= 0 && cancelSlice.indexOf("focused: activeFocus ||") === -1, true)
    check("the Unlocking ring is Cancel's own focus", cancelSlice.indexOf("focused: activeFocus") >= 0, true)
    var button = Source.source("ui/DialogButton.qml")
    var activate = Source.slice(button, "    function keyActivate(event)", "\n")
    check("a button ignores a held key's repeats", activate.indexOf("!event.isAutoRepeat) root.activated()") >= 0, true)
    check("Return, Enter and Space all go through keyActivate", ["Keys.onReturnPressed: function(event) { root.keyActivate(event) }",
        "Keys.onEnterPressed: function(event) { root.keyActivate(event) }", "Keys.onSpacePressed: function(event) { root.keyActivate(event) }"]
        .every(function (line) { return button.indexOf(line) >= 0 }), true)

    // Submit moved the keyboard to Cancel, so every refusal returns it to the field.
    var refusal = Source.slice(dialog, "    function showRefusal(note)", "    // A dialog that ends")
    check("a refusal returns the keyboard to the field", refusal.indexOf("field.takeFocus()") >= 0, true)
    var refuseFn = Source.slice(dialog, "    function refuse(note)", "    function showRefusal(note)")
    check("a refusal goes through the field refocus", refuseFn.indexOf("root.showRefusal(note)") >= 0, true)
    check("the wrong phase is set in one place only, beside the refocus",
        dialog.split('root.phase = "wrong"').length - 1 === 1, true)

    // A hidden rail still names the card from the fallback device list.
    var hiddenFallback = [{ label: "cap-loop0", device: "/dev/loop0", locked: true, size: 33554432, fstype: "crypto_LUKS", fsver: "2" }]
    var hiddenRow = typeof Unlock.namingRow === "function" ? Unlock.namingRow([], hiddenFallback, "/dev/loop0") : "missing"
    check("a hidden rail finds the locked row in the fallback list", hiddenRow && hiddenRow.label, "cap-loop0")
    check("the hidden-rail row carries its size", hiddenRow && hiddenRow.size, 33554432)
    check("the hidden-rail row reads its LUKS version", hiddenRow ? Unlock.cryptoName(hiddenRow.fstype, hiddenRow.fsver) : "missing", "LUKS2")
    var railRow = { label: "Backup", device: "/dev/sdb1", locked: true, size: 1, fstype: "crypto_LUKS", fsver: "2" }
    var railWins = typeof Unlock.namingRow === "function" ? Unlock.namingRow([railRow], hiddenFallback, "/dev/sdb1") : "missing"
    check("the rail row still wins when both lists name the device", railWins && railWins.label, "Backup")
    // The naming decision lives in Unlock.namingRow (behavioural checks above); the host only wires the poller to it.
    var hostBody = Source.source("ui/UnlockHost.qml")
    var pollerAt = hostBody.indexOf("target: namePoller.item")
    var loaderAt = pollerAt >= 0 ? hostBody.indexOf("    Loader {", pollerAt) : -1
    var wiredAt = pollerAt >= 0 ? hostBody.indexOf("root.nameDevice()", pollerAt) : -1
    check("the fallback poller renames the card on every listing",
        pollerAt >= 0 && loaderAt > pollerAt && wiredAt > pollerAt && wiredAt < loaderAt, true)
    check("the fallback poller is live while a name is pending, never hard off",
        Source.slice(hostBody, "id: namePoller", "sourceComponent:").indexOf("active: false") === -1
        && Source.slice(hostBody, "id: namePoller", "sourceComponent:").indexOf("unnamedDevice") >= 0, true)
    check("naming reads the rail first and the fallback list next",
        Source.slice(hostBody, "    function nameDevice()", "    // After an unlock").indexOf("lookupRow") >= 0
        && Source.slice(hostBody, "    function lookupRow(", "    // A hidden rail").indexOf("Unlock.namingRow") >= 0, true)

    // The name poller is one-shot, ending with its first listing, and never starts on a touched card.
    var pollerHead = Source.slice(hostBody, "id: namePoller", "sourceComponent:")
    check("the name poller ends with its first listing", pollerHead.indexOf("_namePollDone") >= 0, true)
    check("the name poller never starts on a touched card", pollerHead.indexOf("cardTouched") >= 0, true)
    var naming = Source.slice(hostBody, "    function nameDevice()", "    // After an unlock")
    check("a touched card ends the pending name instead of waiting it out",
        /if \(root\.cardTouched\(\)\) \{[^]*?root\.unnamedDevice = ""[^]*?return/.test(naming), true)
    check("a listing without the row still ends the one-shot wait", naming.indexOf("_namePollDone") >= 0, true)
    check("the poller's first answer latches the one-shot shut",
        hostBody.indexOf("onFirstAnsweredChanged") >= 0 && hostBody.indexOf("_namePollDone = true") >= 0, true)
    check("a reopened card gets a fresh one-shot poll",
        Source.slice(hostBody, "    function openForDevice(", "    // Only an untouched").indexOf("_namePollDone = false") >= 0, true)

    // The card reads its LUKS version off the remembered rail, not the call's argument.
    check("the version read uses the remembered rail",
        Source.slice(hostBody, "    function openFor(", "    // The rail row carries").indexOf("root.versionFor(root.sidebar,") >= 0, true)

    // The first edit of a retry clears the error frame and note; no secret survives the submit, so nothing drops dots.
    var editedSpan = Source.slice(dialog, "onEdited: {", "                Row {")
    check("an edit clears the error frame", editedSpan.indexOf("field.error = false") >= 0, true)
    check("an edit clears the note", editedSpan.indexOf('root.noteText = ""') >= 0, true)
    check("an edit clears the note's error role", editedSpan.indexOf("root.noteIsError = false") >= 0, true)
    check("an edit keeps no refused passphrase", editedSpan.indexOf("retryText") === -1 && editedSpan.indexOf("_refused") === -1, true)

    // The board draws the masked dots through Unlocking and Wrong from a held count, never the passphrase.
    var openBody = Source.slice(dialog, "    function openFor(", "    // The name row")
    var closeBody = Source.slice(dialog, "    function close() {", "    // Ends an attempt")
    var landedBody = Source.slice(dialog, "var note = Unlock.unlockedNote(label)", "root.unlocked(folder, label, note)")
    var dotsField = Source.source("ui/DialogField.qml")
    check("submit holds only the submitted length", submit.indexOf("field.heldMask = field.text.length") >= 0, true)
    check("submit still empties the field", submit.indexOf('field.text = ""') >= 0, true)
    check("open, close and success clear the held count",
        [openBody, closeBody, landedBody].every(function (body) { return body.indexOf("field.heldMask = 0") >= 0 }), true)
    check("a held card still reads as masked", dialog.indexOf("field.text.length > 0 || field.heldMask > 0") >= 0, true)
    check("the held dots are the input's own mask character", dotsField.indexOf("field.passwordCharacter.repeat") >= 0, true)
    var heldAssign = /field\.heldMask\s*=\s*field\.text(?![\w.])/
    check("the held count is an int, never the passphrase", !heldAssign.test(submit), true)
    check("the held-count guard goes red on the regressed line",
        heldAssign.test(submit.replace("field.heldMask = field.text.length", "field.heldMask = field.text;")), true)
    check("the held-count guard goes red before a trailing comment",
        heldAssign.test(submit.replace("field.heldMask = field.text.length", "field.heldMask = field.text; // keep")), true)
    var loopOpen = '{"blockdevices":[{"name":"cap-loop0","path":"/dev/loop0","mountpoints":[null],"rm":true,"type":"loop","fstype":"crypto_LUKS",'
        + '"children":[{"name":"luks-6f1c","path":"/dev/mapper/luks-6f1c","label":"S1LUKS","mountpoints":["/run/media/gm/S1LUKS"],'
        + '"rm":true,"type":"crypt","fstype":"vfat"}]}]}'
    rows = Devices.parseDevices(loopOpen)
    check("the shimmed unlocked leaf is the one row", rows.length === 1 && rows[0].path, "/run/media/gm/S1LUKS")
    check("the shimmed unlocked leaf locks its loop container", Unlock.backingFor(loopOpen, "/dev/mapper/luks-6f1c"), "/dev/loop0")
    check("an eject of the shimmed leaf judges its loop disk", Eject.diskOf(loopOpen, "/dev/mapper/luks-6f1c").path, "/dev/loop0")

    // An internal locked container gets a row too, behind the same switch.
    var internal = '{"blockdevices":['
        + '{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G",'
        + '"children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":256060514304,"type":"part","model":null},'
        + '{"name":"nvme0n1p2","path":"/dev/nvme0n1p2","label":"Backup","mountpoints":[null],"rm":false,"size":500000000000,"type":"part","fstype":"crypto_LUKS","model":null}]}'
        + ']}'
    var inner = Devices.parseDevices(internal)
    check("an internal locked container is a rail row too", inner.length, 2)
    check("the internal locked row reads as locked", inner[1].locked, true)

    // A locked row opens on Unlock; a removable one also offers Flash image in its own group, as any writable removable drive does.
    var menu = Mounts.railMenu({ label: "Backup", group: "device", kind: "volume", device: "/dev/sdb1", mounted: false, removable: true, locked: true })
    check("a locked removable row offers Unlock, then Flash image", menu.map(function (e) { return e.separator ? "|" : e.label }).join(","), "Unlock,|,Flash image")
    var fixed = Mounts.railMenu({ label: "Backup", group: "device", kind: "volume", device: "/dev/nvme0n1p2", mounted: false, removable: false, locked: true })
    check("a locked internal row offers Unlock alone", fixed.length, 1)
    check("and that row is Unlock", menu[0].label, "Unlock")
    check("the Unlock row carries the unlock action", menu[0].action, "unlockVolume")
    check("the Unlock row draws the lock mark", menu[0].glyph, "lock")

    // An unlocked crypt row offers Open, Unmount and Eject, and never Unlock.
    var cryptMenu = Mounts.railMenu({ label: "Backup", group: "device", kind: "volume", device: "/dev/dm-0", path: "/run/media/gm/Backup", mounted: true, removable: true, volumeMenu: true, locked: false })
    check("an unlocked row offers open, unmount and eject",
        cryptMenu.filter(function (r) { return r.separator !== true }).map(function (r) { return r.label + ":" + r.action }).join(","), "Open:open,Unmount:unmountVolume,Eject:eject")
    // Its mapper names no drive the flasher lists, so neither name of an open crypt offers Flash image.
    var mapperMenu = Mounts.railMenu({ label: "vault", group: "device", kind: "volume", device: "/dev/mapper/luks-vault", path: "/run/media/gm/vault", mounted: true, removable: true, volumeMenu: true, locked: false })
    check("an open crypt named by its mapper offers no Flash image", mapperMenu.some(function (r) { return r.action === "flashImage" || r.label === "Flash image" }), false)
    var plainMenu = Mounts.railMenu({ label: "vault", group: "device", kind: "volume", device: "/dev/sdc1", path: "/run/media/gm/vault", mounted: true, removable: true, volumeMenu: true, locked: false })
    check("while the same row on a partition offers it", plainMenu.some(function (r) { return r.action === "flashImage" && r.label === "Flash image" }), true)

    // Sample lsblk body: an unlocked crypt on a stick, and a plain stick beside it.
    var open = '{"blockdevices":['
        + '{"name":"sdb","path":"/dev/sdb","type":"disk","rm":true,"mountpoints":[null],'
        + '"children":[{"name":"sdb1","path":"/dev/sdb1","type":"part","fstype":"crypto_LUKS","mountpoints":[null],'
        + '"children":[{"name":"luks-vault","path":"/dev/mapper/luks-vault","type":"crypt","mountpoints":["/run/media/gm/vault"]}]}]},'
        + '{"name":"sdc","path":"/dev/sdc","type":"disk","rm":true,"mountpoints":[null],'
        + '"children":[{"name":"sdc1","path":"/dev/sdc1","type":"part","fstype":"vfat","mountpoints":["/run/media/gm/STICK"]}]}'
        + ']}'
    check("the container behind an unlocked crypt is what lock takes", Unlock.backingFor(open, "/dev/mapper/luks-vault"), "/dev/sdb1")
    check("a plain partition has no container to lock", Unlock.backingFor(open, "/dev/sdc1"), "")
    check("the locked container itself has none either", Unlock.backingFor(open, "/dev/sdb1"), "")
    check("an empty device locks nothing", Unlock.backingFor(open, ""), "")
    check("a body that does not parse locks nothing", Unlock.backingFor("not json", "/dev/mapper/luks-vault"), "")

    // The eject verdict reads the device before the chain state clears it, or the lock never fires.
    var verdictBody = Source.slice(Source.source("ui/DeviceMounts.qml"), "    function reportEject(", "    Timer {")
    check("the eject lock runs before the device is cleared",
        verdictBody.indexOf("root.lockCrypt(root._ejectDevice") >= 0
            && verdictBody.indexOf("root.lockCrypt(root._ejectDevice") < verdictBody.indexOf('root._ejectDevice = ""'), true)
    runEjectLock(check, open)
}

// Sample input: the listing before the eject (the crypt unlocked) and the one gio's exit was judged on (the drive gone).
function ejectLockFrom(stale, judged, unmount) {
    var mounts = Source.source("ui/DeviceMounts.qml")
    var body = Source.slice(mounts, "    function lockCrypt(", "    function eject(")
        + Source.slice(mounts, "    function judgeEject(", "    Timer {")
    var handed = []
    var attempts = 0
    var root = { _listing: stale, _ejectDevice: "/dev/mapper/luks-vault", _ejectLabel: "vault", _verdictFromListing: 1, _listingsStarted: 1,
        _lastVerdict: "", forgetMessage: function () {}, message: function () {},
        deviceLock: { lockCryptAfter: function (listing, device) { attempts += 1; handed.push(Unlock.backingFor(listing, device)); return true } } }
    var stop = { stop: function () {} }
    new Function("root", "Eject", "powerOffTimeout", "ejectVerdictTimeout", "judged", "deviceLock",
        body + "; root.lockCrypt = lockCrypt; root.reportEject = reportEject; root.judgeEject = judgeEject; " + (unmount ? "root.lockCrypt(root._ejectDevice, root._ejectLabel);" : "root.judgeEject(judged);"))(root, Eject, stop, stop, judged, root.deviceLock)
    // One attempt refused by backingFor reads "1:", never reaching lockCrypt reads "0:".
    return attempts + ":" + handed.join("|")
}

// UL4: gvfs ejecting a LUKS stick locks and powers it off itself, so only a crypt the judged listing still shows gets a lock.
function runEjectLock(check, open) {
    var gone = '{"blockdevices":[{"name":"sdc","path":"/dev/sdc","type":"disk","rm":true,"mountpoints":[null],'
        + '"children":[{"name":"sdc1","path":"/dev/sdc1","type":"part","fstype":"vfat","mountpoints":[null]}]}]}'
    var parked = '{"blockdevices":[{"name":"sdb","path":"/dev/sdb","type":"disk","rm":true,"mountpoints":[null],'
        + '"children":[{"name":"sdb1","path":"/dev/sdb1","type":"part","fstype":"crypto_LUKS","mountpoints":[null],'
        + '"children":[{"name":"luks-vault","path":"/dev/mapper/luks-vault","type":"crypt","mountpoints":[null]}]}]}]}'
    var relocked = '{"blockdevices":[{"name":"sdb","path":"/dev/sdb","type":"disk","rm":true,"mountpoints":[null],'
        + '"children":[{"name":"sdb1","path":"/dev/sdb1","type":"part","fstype":"crypto_LUKS","mountpoints":[null]}]}]}'
    check("a drive gvfs already powered off gets one refused attempt", ejectLockFrom(open, gone, false), "1:")
    check("a container gvfs already locked gets one refused attempt", ejectLockFrom(open, relocked, false), "1:")
    check("a crypt the judged listing still shows is locked", ejectLockFrom(open, parked, false), "1:/dev/sdb1")
    check("an unmount locks from the listing it already holds", ejectLockFrom(open, gone, true), "1:/dev/sdb1")
}
