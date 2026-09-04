.import "../../ui/js/Format.js" as Format

// The whole Format suite against one check function: sizes, dates, permissions and
// path parts. The date pins read America/New_York's wall clock, which js.sh
// supplies, so they catch both UTC rendering and DST boundary regressions.
function run(check) {
    // The home prefix reads as the user writes it; the window chrome and the search strip share this.
    check("a path under home comes back with a tilde",
          Format.tilde("/home/gm/Documents/claude", "/home/gm"), "~/Documents/claude")
    check("home itself is just the tilde",
          Format.tilde("/home/gm", "/home/gm"), "~")
    check("a path outside home is left alone",
          Format.tilde("/usr/share/omarchy", "/home/gm"), "/usr/share/omarchy")
    check("an unknown home leaves every path alone",
          Format.tilde("/home/gm/x", ""), "/home/gm/x")

    // The chrome draws the directory's own name at full contrast and everything above it muted.
    check("the parent half keeps its trailing separator",
          Format.parentPart("~/Documents/claude"), "~/Documents/")
    check("and the leaf is the directory's own name",
          Format.leafPart("~/Documents/claude"), "claude")
    check("a bare name is all leaf and no parent",
          Format.parentPart("claude") + "|" + Format.leafPart("claude"), "|claude")
    check("the root is its own label with no parent half",
          Format.parentPart("/") + "|" + Format.leafPart("/"), "|/")
    check("a tilde alone is its own leaf",
          Format.parentPart("~") + "|" + Format.leafPart("~"), "|~")

    check("zero bytes", Format.size(0), "0 B")
    check("just under a kilobyte", Format.size(999), "999 B")
    check("exactly a kilobyte", Format.size(1000), "1.0 kB")
    check("the old kibibyte boundary is not special", Format.size(1024), "1.0 kB")
    check("a megabyte and a half", Format.size(1500000), "1.5 MB")
    // Finder renders this exact number as 26.95 GB; real GLib on this box prints 26.9 GB, see AGENTS.md.
    check("Finder's own example, at GLib's precision", Format.size(26950000000), "26.9 GB")
    check("a terabyte", Format.size(1000000000000), "1.0 TB")

    // 2026-08-27 00:27 UTC, the instant the old suite already pinned. In New York
    // this is 20:27 on 26 August, so a regression to UTC getters reddens everywhere.
    var FIXED = 1787790423
    var NOW = 1787790423000
    check("today shows the local time", Format.date(FIXED, NOW), "Today, 20:27")
    check("yesterday is named", Format.date(FIXED - 86400, NOW), "Yesterday, 20:27")
    check("this year omits the year", Format.date(FIXED - 30 * 86400, NOW), "27 Jul, 20:27")
    check("a past year carries it and drops the time", Format.date(FIXED - 400 * 86400, NOW), "22 Jul 2025")

    // Spring forward: the event uses EST while noon uses EDT. Reusing noon's offset
    // moves the previous evening across midnight and incorrectly calls it Today.
    var SPRING_NOW = 1772985600
    check("the hour before spring forward is Today at 01:30",
          Format.date(1772951400, SPRING_NOW * 1000), "Today, 01:30")
    check("the hour after the skip is Today at 03:30",
          Format.date(1772955000, SPRING_NOW * 1000), "Today, 03:30")
    check("the evening before spring forward is Yesterday at 23:30",
          Format.date(1772944200, SPRING_NOW * 1000), "Yesterday, 23:30")

    // Fall back: 01:30 occurs once in EDT and once in EST. The 00:30 case catches
    // a fixed EST offset, which would shift that still-current day into yesterday.
    var FALL_NOW = 1793552400
    check("the hour before fall back stays Today at 00:30",
          Format.date(1793507400, FALL_NOW * 1000), "Today, 00:30")
    check("the first pass of the repeated hour is Today at 01:30",
          Format.date(1793511000, FALL_NOW * 1000), "Today, 01:30")
    check("the second pass of the repeated hour is Today at 01:30 too",
          Format.date(1793514600, FALL_NOW * 1000), "Today, 01:30")
    check("the evening before fall back is Yesterday at 23:30",
          Format.date(1793503800, FALL_NOW * 1000), "Yesterday, 23:30")

    check("a regular file 644", Format.permissions(33188), "rw-r--r--")
    check("a directory 755", Format.permissions(16877), "rwxr-xr-x")
    check("a symlink 777", Format.permissions(41471), "rwxrwxrwx")
    check("no permissions at all", Format.permissions(32768), "---------")

    check("a symlink is a symlink", Format.isSymlink(41471), true)
    check("a file is not a symlink", Format.isSymlink(33188), false)
    check("mode 755 is executable", Format.isExecutable(33261), true)
    check("mode 644 is not executable", Format.isExecutable(33188), false)
    check("a vanished row is not executable", Format.isExecutable(0), false)
}
