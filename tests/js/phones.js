.import "../../ui/js/Phones.js" as Phones
.import "../../ui/js/Mounts.js" as Mounts

function run(check) {
    // Real gio mount -li output, captured on the box with a Samsung phone on USB (2026-09-11),
    // cut to the lines the parser reads plus the noise it must survive.
    var unplugged = 'Drive(0): CT2000T700SSD3\n'
                  + '  Type: GProxyDrive (GProxyVolumeMonitorUDisks2)\n'
    check("a box with no phone parses to nothing", Phones.parsePhones(unplugged).length, 0)
    check("empty gio output parses to nothing", Phones.parsePhones("").length, 0)
    check("garbage gio output parses to nothing", Phones.parsePhones("not gio output at all\n").length, 0)

    var idle = 'Volume(0): SAMSUNG Android\n'
             + '  Type: GProxyVolume (GProxyVolumeMonitorMTP)\n'
             + '  ids:\n'
             + "   unix-device: '/dev/bus/usb/001/013'\n"
             + '  activation_root=mtp://SAMSUNG_SAMSUNG_Android_RQGL705T0NR/\n'
             + '  themed icons:  [multimedia-player]\n'
             + '  can_mount=1\n'
             + '  can_eject=0\n'
             + '  should_automount=1\n'
    var rows = Phones.parsePhones(idle)
    check("a plugged phone is one row", rows.length, 1)
    check("the row keeps the volume's own label", rows[0].label, "SAMSUNG Android")
    check("the row's uri is the activation root", rows[0].uri, "mtp://SAMSUNG_SAMSUNG_Android_RQGL705T0NR/")
    check("an unmounted phone reads as unmounted", rows[0].mounted, false)
    check("a phone rides the DEVICES group", rows[0].group + "|" + rows[0].kind, "device|phone")
    check("the path stays empty until the open resolves it", rows[0].path, "")

    // Mounted: gio adds the volume's own indented Mount() inside the block, and a top-level
    // shadow Mount() after it, which is parseMounts's to skip and not a second phone.
    var live = idle
             + '  Mount(0): SAMSUNG Android -> mtp://SAMSUNG_SAMSUNG_Android_RQGL705T0NR/\n'
             + '    Type: GProxyShadowMount (GProxyVolumeMonitorMTP)\n'
             + '    can_unmount=1\n'
             + 'Mount(1): mtp -> mtp://SAMSUNG_SAMSUNG_Android_RQGL705T0NR/\n'
             + '  Type: GDaemonMount\n'
             + '  is_shadowed=1\n'
    var mounted = Phones.parsePhones(live)
    check("the mounted phone is still one row", mounted.length, 1)
    check("the indented Mount() flips it to mounted", mounted[0].mounted, true)
    check("the shadow mount is not a network row either", Mounts.parseMounts(live).length, 0)

    // A udisks volume prints indented under its Drive() block; even hoisted to column zero its
    // Type line is the wrong monitor, so both guards hold on their own.
    var stick = 'Drive(4): WD_BLACK SN850X 4000GB\n'
              + '  Type: GProxyDrive (GProxyVolumeMonitorUDisks2)\n'
              + '  Volume(0): STEAM\n'
              + '    Type: GProxyVolume (GProxyVolumeMonitorUDisks2)\n'
              + '    can_mount=0\n'
              + '    Mount(0): STEAM -> file:///run/media/gm/STEAM\n'
    check("a udisks volume under its drive is not a phone", Phones.parsePhones(stick).length, 0)
    var hoisted = 'Volume(0): STEAM\n'
                + '  Type: GProxyVolume (GProxyVolumeMonitorUDisks2)\n'
                + '  activation_root=file:///run/media/gm/STEAM\n'
                + '  can_mount=1\n'
    check("a top-level udisks volume is still the wrong monitor", Phones.parsePhones(hoisted).length, 0)

    // A camera speaks PTP through the GPhoto2 monitor, the other volume kind with no block device.
    var camera = 'Volume(0): Canon Digital Camera\n'
               + '  Type: GProxyVolume (GProxyVolumeMonitorGPhoto2)\n'
               + '  activation_root=gphoto2://%5Busb%3A001%2C010%5D/\n'
               + '  can_mount=1\n'
    var cam = Phones.parsePhones(camera)
    check("a gphoto2 camera is a row too", cam.length, 1)
    check("its uri is the gphoto2 activation root", cam[0].uri, "gphoto2://%5Busb%3A001%2C010%5D/")
    check("a gphoto2 mount is not a network row", Mounts.parseMounts('Mount(0): camera -> gphoto2://%5Busb%3A001%2C010%5D/\n').length, 0)

    // A volume that refuses mounting, or names no root to mount, is not a row anyone can act on.
    var refusing = 'Volume(0): SAMSUNG Android\n'
                 + '  Type: GProxyVolume (GProxyVolumeMonitorMTP)\n'
                 + '  activation_root=mtp://SAMSUNG_SAMSUNG_Android_RQGL705T0NR/\n'
                 + '  can_mount=0\n'
    check("can_mount=0 is not a row", Phones.parsePhones(refusing).length, 0)
    var rootless = 'Volume(0): SAMSUNG Android\n'
                 + '  Type: GProxyVolume (GProxyVolumeMonitorMTP)\n'
                 + '  can_mount=1\n'
    check("no activation root is not a row", Phones.parsePhones(rootless).length, 0)

    // Two phones are two rows, and the block walk does not bleed one's fields into the other.
    var two = idle
            + 'Volume(1): Pixel 7\n'
            + '  Type: GProxyVolume (GProxyVolumeMonitorMTP)\n'
            + '  activation_root=mtp://Google_Pixel_7_1A2B/\n'
            + '  can_mount=1\n'
            + '  Mount(0): Pixel 7 -> mtp://Google_Pixel_7_1A2B/\n'
    var pair = Phones.parsePhones(two)
    check("two phones are two rows", pair.length, 2)
    check("the first stays unmounted", pair[0].mounted, false)
    check("the second's mount is its own", pair[1].mounted + "|" + pair[1].uri, "true|mtp://Google_Pixel_7_1A2B/")

    // The rail's shared plumbing: menu, key and release, the same contract volumes and shares hold.
    check("a mounted phone offers Unmount and only Unmount",
          Mounts.railMenu(mounted[0]).map(function (r) { return r.action }).join(","), "unmountPhone")
    check("an unmounted phone offers nothing", Mounts.railMenu(rows[0]).length, 0)
    check("a phone's key is its uri", Mounts.railKey(mounted[0]), "mtp://SAMSUNG_SAMSUNG_Android_RQGL705T0NR/")
    check("rowMenu adds no share rows to a phone",
          Mounts.rowMenu(mounted[0]).map(function (r) { return r.action }).join(","), "unmountPhone")

    // Release resolves through the sidebar, which owns the phone Service; a stale key does nothing.
    var releasedKey = ""
    var sidebar = { deviceEntries: [mounted[0]], releasePhone: function (key) { releasedKey = key } }
    Mounts.release("unmountPhone", "mtp://SAMSUNG_SAMSUNG_Android_RQGL705T0NR/", null, null, sidebar)
    check("unmountPhone hands the key to the sidebar", releasedKey, "mtp://SAMSUNG_SAMSUNG_Android_RQGL705T0NR/")

    // An unchanged poll must not assign, and a mount-state flip must: the two sides of sameEntries.
    check("an unchanged poll compares equal", Mounts.sameEntries(rows, Phones.parsePhones(idle)), true)
    check("a mount-state flip does not", Mounts.sameEntries(rows, mounted), false)
}
