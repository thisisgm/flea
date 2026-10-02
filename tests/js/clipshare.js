.import "../../ui/js/ClipShare.js" as ClipShare

function run(check) {
    // The live payload a real copy left on this box's clipboard on 2026-09-25: one verb line, one
    // file:// URI per line, no trailing newline, under x-special/gnome-copied-files.
    var real = "copy\nfile:///mnt/terachad/Emulators/EmuDeck/roms_mid/psp"
    var got = ClipShare.parse(real)
    check("a real Nautilus copy parses to one path", got.paths.join(","), "/mnt/terachad/Emulators/EmuDeck/roms_mid/psp")
    check("and a copy is not a move", got.moving, false)
    check("a cut carries the move", ClipShare.parse("cut\nfile:///a").moving, true)

    // The round trip is the bridge's whole loop guard: what payload writes, parse reads back, and
    // key answers the same string for both sides of it.
    var clip = { paths: ["/home/gm/a b.png", "/home/gm/c#d?e"], moving: false }
    var wire = ClipShare.payload(clip)
    check("a space is escaped the way GLib writes it", wire.indexOf("file:///home/gm/a%20b.png") > 0, true)
    check("the two bytes encodeURI leaves literal are escaped by hand",
          wire.indexOf("file:///home/gm/c%23d%3Fe") > 0, true)
    var back = ClipShare.parse(wire)
    check("the payload round-trips its own paths", back.paths.join("|"), clip.paths.join("|"))
    check("and the key agrees on both sides", ClipShare.key(back), ClipShare.key(clip))

    // The payload is untrusted text, and one refused line refuses the whole of it: a batch the
    // operator saw as N items must never arrive as fewer.
    check("no verb is no clipboard", ClipShare.parse("file:///a"), null)
    check("a verb that is neither cut nor copy is refused", ClipShare.parse("link\nfile:///a"), null)
    check("a foreign scheme refuses the whole payload", ClipShare.parse("copy\nfile:///a\nsmb://nas/b"), null)
    check("a foreign authority is another machine and refuses it", ClipShare.parse("copy\nfile://nas/a"), null)
    check("a control character in the decoded path refuses it", ClipShare.parse("copy\nfile:///a%0ab"), null)
    check("a malformed escape refuses it", ClipShare.parse("copy\nfile:///a%zz"), null)
    check("a verb with no uri at all is no clipboard", ClipShare.parse("copy\n"), null)
    check("empty text is no clipboard", ClipShare.parse(""), null)
    check("and CRLF line ends still parse, because text/uri-list writers use them",
          ClipShare.parse("copy\r\nfile:///a\r\n").paths.join(","), "/a")

    // The key: "" for anything empty, so a publish and a read that both mean nothing compare equal.
    check("an empty clipboard keys to nothing", ClipShare.key({ paths: [], moving: false }), "")
    check("no clipboard at all keys to nothing", ClipShare.key(null), "")
    check("a cut and a copy of one path do not compare equal",
          ClipShare.key({ paths: ["/a"], moving: true }) === ClipShare.key({ paths: ["/a"], moving: false }), false)
}
