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
}
