.import "../../ui/js/Format.js" as Format

function run(check) {
    check("pending directory size is not empty", Format.directorySize(null), "·")
    check("incomplete zero walk is unknown", Format.directorySize({partial: true, bytes: 0}), "Unknown")
    check("complete empty walk really is zero", Format.directorySize({partial: false, bytes: 0}), "0 B")
    check("partial positive walk is a lower bound", Format.directorySize({partial: true, bytes: 1000}), ">1.0 kB")
    check("a populated cloud directory's zero entry size is unknown, not empty",
          Format.propertySize({directory: true, bytes: 0}), "Not calculated")
    check("a local directory's nonzero entry size is not its contents either",
          Format.propertySize({directory: true, bytes: 4096}), "Not calculated")
    check("a genuine empty file still has an exact zero size",
          Format.propertySize({directory: false, bytes: 0}), "0 B (0 bytes)")
    check("older file replies retain their exact size",
          Format.propertySize({bytes: 12}), "12 B (12 bytes)")
    check("rclone directs users to live mount telemetry",
          JSON.stringify(Format.storageFacts({filesystem: "fuse.rclone"})),
          JSON.stringify([["Storage", "rclone mount"], ["Upload status", "See the cloud status bar. A completed copy may still be uploading."]]))
    check("generic fuse does not imply rclone", Format.storageFacts({filesystem: "fuse"}).length, 1)
    check("missing mount information adds no claim", Format.storageFacts({}).length, 0)
    // One grouping rule for every count the product prints: the search's scan and the filter's scope.
    check("a short count is not grouped", Format.count(653), "653")
    check("a thousand takes one separator", Format.count(4120), "4,120")
    check("a million takes two", Format.count(1234567), "1,234,567")

    // The home prefix reads as the user writes it; the window chrome and the search strip share this.
    check("a path under home comes back with a tilde",
          Format.tilde("/home/gm/Documents/claude", "/home/gm"), "~/Documents/claude")
    check("home itself is just the tilde",
          Format.tilde("/home/gm", "/home/gm"), "~")
    // Issue 95, nixfred: a bare prefix test made a sibling directory wear home's name.
    check("a sibling whose name starts with home's keeps its own",
          Format.tilde("/home/gmx", "/home/gm"), "/home/gmx")
    check("and so does everything under it",
          Format.tilde("/home/gmx/Work", "/home/gm"), "/home/gmx/Work")
    check("a path outside home is left alone",
          Format.tilde("/usr/share/omarchy", "/home/gm"), "/usr/share/omarchy")
    check("an unknown home leaves every path alone",
          Format.tilde("/home/gm/x", ""), "/home/gm/x")

    // ui/js/Tabs.js "label" names a tab after the directory it stands in, which is this and nothing else.
    check("the leaf is the directory's own name",
          Format.leafPart("~/Documents/claude"), "claude")
    check("a bare name is all leaf", Format.leafPart("claude"), "claude")
    check("the root is its own label", Format.leafPart("/"), "/")
    check("a tilde alone is its own leaf", Format.leafPart("~"), "~")

    check("zero bytes", Format.size(0), "0 B")
    check("just under a kilobyte", Format.size(999), "999 B")
    check("exactly a kilobyte", Format.size(1000), "1.0 kB")
    check("the old kibibyte boundary is not special", Format.size(1024), "1.0 kB")
    check("a megabyte and a half", Format.size(1500000), "1.5 MB")
    // Finder renders this exact number as 26.95 GB; real GLib on this box prints 26.9 GB, see AGENTS.md.
    check("Finder's own example, at GLib's precision", Format.size(26950000000), "26.9 GB")
    check("a terabyte", Format.size(1000000000000), "1.0 TB")

    // Local constructors keep these wall-clock expectations valid in every non-UTC test zone.
    check("the stamp is the local wall clock",
          Format.date(new Date(2026, 0, 1, 0, 10).getTime() / 1000), "2026-01-01 00:10")
    check("the minute before local midnight keeps its own day",
          Format.date(new Date(2025, 11, 31, 23, 55).getTime() / 1000), "2025-12-31 23:55")
    check("an older instant is the same form, never a shorter one",
          Format.date(new Date(2025, 11, 30, 22, 0).getTime() / 1000), "2025-12-30 22:00")
    check("this year is the same form too",
          Format.date(new Date(2026, 6, 28, 0, 27).getTime() / 1000), "2026-07-28 00:27")
    // Single-digit months and days pad, which is what makes the column sortable as text.
    check("a single-digit month and day both pad",
          Format.date(new Date(2026, 8, 5, 9, 4).getTime() / 1000), "2026-09-05 09:04")
    // ui/Theme.qml sizes column.date at dateChars, so no instant may be wider than that.
    check("the stamp is always the sixteen characters the column is cut for",
          Format.date(new Date(2026, 8, 5, 9, 4).getTime() / 1000).length, 16)

    // The send picker's column holds about ten characters, so its date drops the time and keeps the
    // date; Preview board, "One function, four surfaces". Fixed instants throughout, never Date.now().
    check("the picker's date is the stamp without its clock",
          Format.compactDate(new Date(2026, 7, 21, 11, 32).getTime() / 1000), "2026-08-21")
    check("and two instants on the same day read the same",
          Format.compactDate(new Date(2026, 7, 21, 23, 59).getTime() / 1000), "2026-08-21")
    // The local-day rollover: one minute either side of local midnight, which is where this breaks.
    check("a minute after local midnight is already the next day",
          Format.compactDate(new Date(2026, 7, 22, 0, 1).getTime() / 1000), "2026-08-22")
    // Two Augusts must not read as one string, which is the whole reason the year survives the trim.
    check("last August carries its year",
          Format.compactDate(new Date(2025, 7, 21, 10, 0).getTime() / 1000), "2025-08-21")
    check("and the August before it carries a different one",
          Format.compactDate(new Date(2024, 7, 21, 10, 0).getTime() / 1000), "2024-08-21")
    // ui/Theme.qml sizes column.pickerDate at ten characters of this face, so nothing here may elide.
    check("the compact form is always the ten characters the column is cut for",
          Format.compactDate(new Date(2005, 7, 21, 10, 0).getTime() / 1000).length, 10)

    // The window keeps its own form for the same instant; the picker column is the narrower one.
    check("the window's stamp is not the picker's compact form",
          Format.date(new Date(2025, 7, 21, 10, 0).getTime() / 1000), "2025-08-21 10:00")

    check("a regular file 644", Format.permissions(33188), "rw-r--r--")
    check("a directory 755", Format.permissions(16877), "rwxr-xr-x")
    check("a symlink 777", Format.permissions(41471), "rwxrwxrwx")
    check("no permissions at all", Format.permissions(32768), "---------")

    check("a symlink is a symlink", Format.isSymlink(41471), true)
    check("a file is not a symlink", Format.isSymlink(33188), false)
    check("mode 755 is executable", Format.isExecutable(33261), true)
    check("mode 644 is not executable", Format.isExecutable(33188), false)
    check("a vanished row is not executable", Format.isExecutable(0), false)

    // Issue 67, jesedv: the yanked path is quoted unless a shell reads every character as itself.
    check("a path a shell reads as one word is handed over as it is",
          Format.shellQuoted("/home/gm/Work-2.0_final"), "/home/gm/Work-2.0_final")
    check("a tilde is not one of those characters, because a shell expands it",
          Format.shellQuoted("~/Work"), "'~/Work'")
    check("a path holding a space is quoted whole",
          Format.shellQuoted("/home/gm/directory two"), "'/home/gm/directory two'")
    check("a tab is quoted the same way", Format.shellQuoted("/home/gm/one\ttwo"), "'/home/gm/one\ttwo'")
    // Every one byte for byte inside the quotes, because an ends-only check passes a dropped character.
    for (var dangerous of ["$HOME", "`id`", "a;b", "a&b", "a|b", "a*b", "a?b", "a(b)", "a\nb", "a!b", 'a"b',
                           "a b", "a\\b", "a<b", "a>b", "a#b", "a{b}", "a[b]", "a^b"]) {
        check("a path holding " + JSON.stringify(dangerous) + " is quoted whole",
              Format.shellQuoted("/home/gm/" + dangerous), "'/home/gm/" + dangerous + "'")
    }
    check("and a quote inside the path closes and reopens around itself",
          Format.shellQuoted("/home/gm/a'b"), "'/home/gm/a'\\''b'")
}
