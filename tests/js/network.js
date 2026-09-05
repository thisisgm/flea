.import "../../ui/js/Mounts.js" as Mounts
.import "../../ui/js/Protocols.js" as Protocols

// Issue #36 (@janoguerra): every network decision resolves from the mount's own URI, or from gio
// output ui/NetworkMounts.qml pins to the C locale, and never from a sentence gvfsd translated.
function run(check) {
    // gio mount -l, the live 2026-08-31 shape ui/js/Mounts.js "parseMounts" records, with gvfsd's
    // own label in Spanish: the daemon composed that label, so no client locale can change it.
    var spanish = 'Mount(0): isos en 192.168.1.10 -> smb://192.168.1.10/isos/\n'
    check("a translated mount label still names the share", Mounts.parseMounts(spanish)[0].label, "isos")
    var english = 'Mount(0): isos on 192.168.1.10 -> smb://192.168.1.10/isos/\n'
    check("and the English one still names it too", Mounts.parseMounts(english)[0].label, "isos")
    var french = 'Mount(0): isos sur nas -> smb://nas/isos/\n'
    check("and so does a third language", Mounts.parseMounts(french)[0].label, "isos")

    // The control the old rule failed: it cut at the English word "on" without ever testing that the
    // tail was this URI's host, so a mount whose own name carries that word lost half of itself.
    var onTuesday = 'Mount(0): backup on tuesday -> smb://nas/backup%20on%20tuesday/\n'
    check("a label that merely contains the word on keeps all of it",
          Mounts.parseMounts(onTuesday)[0].label, "backup on tuesday")
    var onTuesdayOnNas = 'Mount(0): backup on tuesday on nas -> smb://nas/backup%20on%20tuesday/\n'
    check("and loses only the host when it really is share-on-host",
          Mounts.parseMounts(onTuesdayOnNas)[0].label, "backup on tuesday")
    var phone = 'Mount(0): Pixel 7 -> mtp://Google_Pixel_7_1A2B/\n'
    check("a label that is not share-on-host keeps the name gio gave it",
          Mounts.parseMounts(phone)[0].label, "Pixel 7")
    var bareRoot = 'Mount(0): nas -> smb://nas/\n'
    check("a server root's own name is never cut down to nothing",
          Mounts.parseMounts(bareRoot)[0].label, "nas")
    var sameName = 'Mount(0): nas on nas -> smb://nas/nas/\n'
    check("a share named after its own host still loses only the host",
          Mounts.parseMounts(sameName)[0].label, "nas")
    // Two more the fix round found in the fix: a tail that only happens to be the host with no
    // separator at all, and a label whose separator word has nothing in front of it.
    var glued = 'Mount(0): isosnas -> smb://nas/isos/\n'
    check("a label that merely ends in the host, with no separator, is not cut",
          Mounts.parseMounts(glued)[0].label, "isosnas")
    var nothingBefore = 'Mount(0): on nas -> smb://nas/on/\n'
    check("and one with nothing before the separator keeps every character",
          Mounts.parseMounts(nothingBefore)[0].label, "on nas")

    // The host the label is measured against comes out of the URI's authority, which stops at the
    // first slash: a "@" or a ":" further along belongs to the path and is not a host or a port.
    check("the authority stops at the first slash, so a path never supplies a host",
          Protocols.hostOf("sftp://u@h:22/inbox@2026"), "h")
    check("a bracketed IPv6 literal survives whole", Protocols.hostOf("smb://[fe80::1]:445/isos/"), "[fe80::1]")
    check("a domain-qualified SMB user is not the host", Protocols.hostOf("smb://WORK;gm@nas/isos/"), "nas")
    check("a uri with no authority has no host at all", Protocols.hostOf("file:///home/gm"), "")
    check("no uri at all has no host rather than throwing", Protocols.hostOf(null), "")

    // Sample input: gio info under the C locale ui/NetworkMounts.qml pins, one resolver for the
    // product and for this suite, so the wording lives in exactly one place.
    var info = 'uri: smb://192.168.1.10/isos/\n'
             + 'local path: /run/user/1000/gvfs/smb-share:server=192.168.1.10,share=isos\n'
             + 'unix mount: gvfsd-fuse\n'
    check("the FUSE path comes out of gio's own C-locale line",
          Mounts.localPath(info), "/run/user/1000/gvfs/smb-share:server=192.168.1.10,share=isos")
    check("a location with no FUSE path answers nothing rather than undefined",
          Mounts.localPath('uri: smb://nas/\ntype: directory\n'), "")
    check("no gio output at all answers nothing rather than throwing", Mounts.localPath(""), "")

    // PR #21 (@TomFaulkner): one rail row per share, however the port is spelled. "gio mount -l"
    // never reports a scheme's default port, while the add form spells out the one it prefilled.
    check("a default port is not part of the canonical form",
          Mounts.normalize("smb://nas:445/isos/"), "smb://nas/isos")
    check("so a bookmark and a live mount of one share resolve to one rail row",
          Mounts.normalize("smb://nas:445/isos/") === Mounts.normalize("smb://nas/isos/"), true)
    check("a port the scheme would not have used stays",
          Mounts.normalize("smb://nas:4450/isos/"), "smb://nas:4450/isos")
    check("a bare root keeps its own single slash after the port goes",
          Mounts.normalize("smb://nas:445"), "smb://nas/")
    // The defect the contribution itself introduced: a lastIndexOf("@") over the whole URI reads a
    // path's own "@" as the authority, leaves the :22 standing, and answers two rows for one share.
    check("an at sign further along the path is never read as the authority",
          Mounts.normalize("sftp://u@h:22/inbox@2026"), "sftp://u@h/inbox@2026")

    // PR #21: the rail takes live mounts first and skips a bookmark whose key it already has, so the
    // bookmark's own label was thrown away. A rename typed on a mounted share was written to the file
    // and reverted on the next five second poll with nothing said; the name the operator gave wins now.
    var bookmarked = Mounts.nonFileBookmarks("smb://nas:445/isos NAS isos\nsmb://other/data Other\n")
    check("the name the operator gave a place wins on the row its live mount draws",
          Mounts.railLabel({ label: "isos", uri: "smb://nas/isos/" }, bookmarked), "NAS isos")
    check("and it matches however the two spell the port, the way the rail's own dedup does",
          Mounts.railLabel({ label: "isos", uri: "smb://nas:445/isos" }, bookmarked), "NAS isos")
    check("a live mount nothing has bookmarked keeps the name gio gave it",
          Mounts.railLabel({ label: "isos", uri: "smb://third/isos/" }, bookmarked), "isos")
    check("and with no saved places at all it keeps that name too, rather than nothing",
          Mounts.railLabel({ label: "isos", uri: "smb://nas/isos/" }, []), "isos")

    check("a bracketed IPv6 host whose own digits end in the port keeps them",
          Protocols.stripDefaultPort("smb://[fe80::445]/isos/"), "smb://[fe80::445]/isos/")
    check("but a real port after the bracket goes",
          Protocols.stripDefaultPort("smb://[fe80::1]:445/isos/"), "smb://[fe80::1]/isos/")
    // Every port here is the one ui/NetworkForm.qml prefills for the protocol that builds that
    // scheme, not a second table's idea of it: the old dav row asked about ":80", a port the form
    // has no way to write, so the one URI it does write went on carrying its port into the rail.
    check("every scheme the form can build knows its own default",
          [Protocols.stripDefaultPort("sftp://h:22/x"), Protocols.stripDefaultPort("ftp://h:21/x"),
           Protocols.stripDefaultPort("ftps://h:21/x"), Protocols.stripDefaultPort("dav://h:443/x"),
           Protocols.stripDefaultPort("davs://h:443/x"), Protocols.stripDefaultPort("nfs://h:2049/x")].join("|"),
          "sftp://h/x|ftp://h/x|ftps://h/x|dav://h/x|davs://h/x|nfs://h/x")
    // Built by the form's own uri(), so the check cannot drift from what the dialog writes: WebDAV
    // picked, TLS unticked, the prefilled port left alone, which is the shape the operator gets.
    function formUri(tls) {
        return Protocols.uri({ protocol: "WebDAV", host: "nas.local", path: "/dav", user: "",
                               domain: "", tls: tls, port: String(Protocols.defaultPort("WebDAV")) })
    }
    check("the form spells the port it prefilled, whichever way the TLS box is set",
          formUri(false) + "|" + formUri(true), "dav://nas.local:443/dav|davs://nas.local:443/dav")
    check("so plain WebDAV's bookmark and gio's own report of it are one rail row",
          Mounts.normalize(formUri(false)) === Mounts.normalize("dav://nas.local/dav/"), true)
    check("and so are TLS WebDAV's, which was already the case",
          Mounts.normalize(formUri(true)) === Mounts.normalize("davs://nas.local/dav/"), true)
    check("and never drops another scheme's default", Protocols.stripDefaultPort("sftp://h:445/x"), "sftp://h:445/x")
    check("text that is not a uri at all is left alone", Protocols.stripDefaultPort("/home/gm"), "/home/gm")

    // The rail menu the right click opens, which is not the release verdict ui/js/Eject.js reads:
    // Ctrl+E must still refuse a row with nothing mounted rather than starting an editor on it.
    var mounted = { path: "", label: "isos", group: "network", kind: "share", uri: "smb://nas/isos/", mounted: true }
    var saved = { path: "", label: "NAS", group: "network", kind: "share", uri: "smb://nas/", mounted: false }
    var volume = { path: "/run/media/gm/128GB", label: "128GB", group: "device", kind: "volume", device: "/dev/sda1", mounted: true }
    var favourite = { path: "/home/gm", label: "Home", group: "favorite", kind: "favorite", mounted: false }
    function labels(rows) { return rows.map(function (r) { return r.label }).join("|") }
    check("a mounted share releases first, then offers the two the place itself owns",
          labels(Mounts.rowMenu(mounted)), "Unmount|Rename|Remove")
    check("and Ctrl+E still reads the release row alone", Mounts.railMenu(mounted)[0].action, "unmount")
    check("a bookmark nothing has mounted offers the two that need no mount",
          labels(Mounts.rowMenu(saved)), "Rename|Remove")
    check("so it opens a menu where it used to open an empty one", Mounts.rowMenu(saved).length, 2)
    check("but it has nothing to release, so Ctrl+E still says so", Mounts.railMenu(saved).length, 0)
    check("a removable volume's menu is untouched", labels(Mounts.rowMenu(volume)), "Eject")
    check("a favourite still opens no menu at all", Mounts.rowMenu(favourite).length, 0)
    check("no entry at all offers nothing rather than throwing", Mounts.rowMenu(null).length, 0)
    check("Remove draws the minus mark, because forgetting a place trashes nothing",
          Mounts.rowMenu(saved)[1].glyph, "minus")

    // A chosen row arrives as its key, never its position: the rail rebuilds on a five second poll.
    function chose(action, key, entries) {
        var log = []
        var sidebar = { favoriteEntries: [favourite], networkEntries: entries, deviceEntries: [volume],
                        startRename: function (i) { log.push("rename" + i) } }
        var mounts = { unmount: function (i) { log.push("unmount" + i) },
                       forget: function (uri) { log.push("forget " + uri) } }
        var devices = { eject: function (i) { log.push("eject" + i) } }
        Mounts.release(action, key, devices, mounts, sidebar)
        return log.join(",")
    }
    check("Rename starts the rail's own editor on the row the key names, past the favourites",
          chose("rename", "smb://nas/", [mounted, saved]), "rename2")
    check("Remove forgets the place by uri and never by position",
          chose("remove", "smb://nas/", [mounted, saved]), "forget smb://nas/")
    check("a key that no longer names a row does nothing at all",
          chose("remove", "smb://gone/", [mounted, saved]), "")
    check("Unmount still resolves through the network Service",
          chose("unmount", "smb://nas/isos/", [mounted, saved]), "unmount0")
    check("Eject still resolves through the device Service",
          chose("eject", "/dev/sda1", [mounted, saved]), "eject0")

    // Sample input: the operator's own bookmarks file, favourites and places in one list.
    var body = "file:///home/gm/Downloads Downloads\nsmb://nas:445/isos NAS isos\nsmb://other/data Other\n"
    check("the place's own line goes and every other byte stays",
          Mounts.removeBookmark(body, "smb://nas/isos/"),
          "file:///home/gm/Downloads Downloads\nsmb://other/data Other\n")
    check("a uri no line carries changes nothing", Mounts.removeBookmark(body, "smb://gone/x"), body)
    check("an empty uri never empties the file", Mounts.removeBookmark(body, ""), body)
    check("no body at all answers nothing rather than throwing", Mounts.removeBookmark("", "smb://nas/isos/"), "")
}
