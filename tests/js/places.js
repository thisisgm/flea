.import "../../ui/js/Places.js" as Places

function run(check) {
    var home = {group: "home", kind: "home", path: "/home/test"}
    var trash = {group: "trash", kind: "trash", path: "trash:///", count: 1}
    var network = {group: "network", kind: "share", uri: "smb://host/data", mounted: false}
    var device = {group: "device", kind: "disk", device: "/dev/nvme0n1", path: "/"}
    var a = {group: "favourite", kind: "favourite", original: {label: "A", path: "/a"}}
    var b = {group: "favourite", kind: "favourite", original: {label: "B", path: "/a"}}
    var oldRail = [home, trash, a, network, device]
    var expandedRail = [home, trash, a, b, network, device]
    check("a Favorite addition preserves the Network target", Places.railCursorAfter(oldRail, expandedRail, 3), 4)
    check("a Favorite removal preserves the Device target", Places.railCursorAfter(expandedRail, oldRail, 5), 4)
    check("reordering Favorites keeps their distinct original labels", Places.railCursorAfter([a, b], [b, a], 0), 1)
    check("an unchanged duplicate occurrence stays selected", Places.railCursorAfter([a, a, b], [b, a, a], 1), 2)
    check("removing an identical duplicate invalidates its ambiguous selection", Places.railCursorAfter([a, a, b], [a, b], 0), -1)
    check("adding an identical duplicate does not guess an occurrence", Places.railCursorAfter([a, b], [a, a, b], 0), -1)
    check("removing the selected row clears the rail cursor", Places.railCursorAfter(oldRail, [home, trash, a, device], 3), -1)
    check("a cleared cursor stays cleared through another refresh", Places.railCursorAfter([], oldRail, -1), -1)
    check("the first arriving rail retains its initial Home cursor", Places.railCursorAfter([], oldRail, 0), 0)
    check("mount state and count changes do not change identity", Places.railCursorAfter([trash, network],
        [{group: "trash", kind: "trash", path: "trash:///", count: 2},
         {group: "network", kind: "share", uri: network.uri, mounted: true, path: "/run/gvfs/data"}], 1), 1)
    check("a moved rename target no longer has its old index", Places.railCursorAfter(oldRail, expandedRail, 3) === 3, false)
    var records = [{ label: "A", path: "/a" }, { label: "Again", path: "/a" }, 17, { label: "", path: "bad" }]
    var stored = Places.storedEntries(records, "/home/test")
    check("Flea keeps duplicate paths", stored.length, 4)
    check("Flea preserves each duplicate label", stored[1].label, "Again")
    check("invalid favourite remains identifiable", stored[2].original, 17)
    check("invalid favourite is marked", stored[2].error.length > 0, true)
    check("an invalid original value keeps its identifying label", stored[2].label, "17")
    check("empty invalid Favorite label follows the ruled spelling", stored[3].label, "Invalid favorite")
    check("invalid Favorite refusal follows the ruled spelling", stored[2].error, "invalid favorite record")
    check("new favourites starts empty", Places.storedEntries([], "/home/test").length, 0)
    check("sidebar width clamps low", Places.sidebarWidth(0), 160)
    check("sidebar width clamps high", Places.sidebarWidth(999), 256)
    check("sidebar width snaps to nearer stop", Places.sidebarWidth(231), 224)
    check("bad width type falls back", Places.sidebarWidth("224"), 192)
    check("tilde expands only for consumption", Places.storedEntries([{label:"Home",path:"~/docs"}], "/home/test")[0].path, "/home/test/docs")
    check("stored tilde stays intact", Places.storedEntries([{label:"Home",path:"~/docs"}], "/home/test")[0].storedPath, "~/docs")

    var dirs = 'XDG_DESKTOP_DIR="$HOME/"\n'
             + 'XDG_DOWNLOAD_DIR="$HOME/Downloads"\n'
             + 'XDG_DOCUMENTS_DIR="$HOME/Documents"\n'
             + '# a comment line\n'
             + 'XDG_PROJECTS_DIR="/srv/projects"\n'
    var got = Places.userDirs(dirs, "/home/gm")
    check("home itself is not a favourite", got.length, 3)
    check("a relative entry expands", got[0].path, "/home/gm/Downloads")
    check("the label is the leaf", got[0].label, "Downloads")
    check("an absolute entry survives", got[2].path, "/srv/projects")

    var marks = 'file:///home/gm/Downloads Downloads\n'
              + 'file:///home/gm/My%20Files\n'
              + 'smb://192.168.1.10/ NAS\n'
    var b = Places.bookmarks(marks)
    check("a non-file bookmark is skipped", b.length, 2)
    check("the trailing label wins", b[0].label, "Downloads")
    check("a percent escape decodes", b[1].path, "/home/gm/My Files")
    check("no label falls back to the leaf", b[1].label, "My Files")

    // The operator's own real NAS bookmark, on a three-line fixture.
    var netFile = 'file:///home/gm/Downloads Downloads\n'
                + 'smb://192.168.1.10/data NAS\n'
                + 'smb://10.0.0.9/backups Backups\n'
    check("only the matched line's label changes", Places.relabel(netFile, "smb://192.168.1.10/data", "Homelab"),
        'file:///home/gm/Downloads Downloads\n'
        + 'smb://192.168.1.10/data Homelab\n'
        + 'smb://10.0.0.9/backups Backups\n')

    // gio's own live-mount uri carries a trailing slash the written bookmark line never had.
    check("a trailing-slash uri still matches the bookmarked line",
        Places.relabel(netFile, "smb://192.168.1.10/data/", "Homelab"),
        'file:///home/gm/Downloads Downloads\n'
        + 'smb://192.168.1.10/data Homelab\n'
        + 'smb://10.0.0.9/backups Backups\n')

    // No existing line for this uri: a mounted-but-never-bookmarked entry gains one on rename.
    check("an unmatched uri is appended rather than dropped",
        Places.relabel(netFile, "smb://192.168.1.10/isos/", "ISOs"),
        netFile + 'smb://192.168.1.10/isos ISOs\n')

    check("appending onto an empty file needs no leading blank line", Places.relabel("", "smb://host/share", "Share"),
        "smb://host/share Share\n")

    check("appending onto a body missing its trailing newline still starts a new line",
        Places.relabel("smb://a/b Existing", "smb://host/share", "Share"),
        "smb://a/b Existing\nsmb://host/share Share\n")

    check("a blank submitted name is a no-op", Places.relabel(netFile, "smb://192.168.1.10/data", "   "), netFile)

    // A hand-edited file can carry two lines for the same normalized uri; both must rewrite.
    var dupes = 'smb://192.168.1.10/data NAS\n'
              + 'file:///home/gm/Downloads Downloads\n'
              + 'smb://192.168.1.10/data/ Old NAS\n'
    // Each line's own uri text survives untouched (line 3 keeps its trailing slash); only the label changes.
    check("every matching line rewrites, not just the first", Places.relabel(dupes, "smb://192.168.1.10/data", "Homelab"),
        'smb://192.168.1.10/data Homelab\n'
        + 'file:///home/gm/Downloads Downloads\n'
        + 'smb://192.168.1.10/data/ Homelab\n')

    // The asymmetry ui/js/Mounts.js "removeBookmark" does not have: it reads each line off the
    // trimmed text, so an indented bookmark is one Remove can drop and Rename could only duplicate.
    check("an indented line is the line that rewrites, not a second line appended",
        Places.relabel("  smb://192.168.1.10/data NAS\n", "smb://192.168.1.10/data", "Homelab"),
        "smb://192.168.1.10/data Homelab\n")

    // relabel is a trust boundary of its own: an embedded newline must not split one line into two.
    check("an embedded newline in the name cannot fork a new line", Places.relabel(netFile, "smb://192.168.1.10/data", "Home\nlab"),
        'file:///home/gm/Downloads Downloads\n'
        + 'smb://192.168.1.10/data Homelab\n'
        + 'smb://10.0.0.9/backups Backups\n')
    // A bookmarks line is arbitrary text, and decodeURIComponent throws on a malformed escape, which
    // would take the whole rail rebuild with it; a parse failure answers a shape instead.
    check("a malformed escape in a bookmark falls back to the raw path",
        Places.bookmarks("file:///home/gm/bad%zz Bad")[0].path, "/home/gm/bad%zz")
    check("a well-formed escape still decodes",
        Places.bookmarks("file:///home/gm/My%20Files Mine")[0].path, "/home/gm/My Files")

    // The FAVORITES merge the rail used to do inline. Home leads whatever either file says, and a
    // path named twice keeps the position it was first given rather than moving to the later one.
    var favDirs = 'XDG_DOWNLOAD_DIR="$HOME/Downloads"\n'
                + 'XDG_DOCUMENTS_DIR="$HOME/Documents"\n'
    var favMarks = 'file:///home/gm/Downloads Grabbed\n'
                 + 'file:///srv/media Media\n'
                 + 'smb://192.168.1.10/data NAS\n'
    var favs = Places.favorites("/home/gm", favDirs, favMarks, function (label) { return "mark:" + label })
    check("Home leads the rail and is never parsed out of a file", favs[0].path + "|" + favs[0].label, "/home/gm|Home")
    check("the XDG dirs follow in file order", favs[1].label + "," + favs[2].label, "Downloads,Documents")
    check("a path already placed keeps its first position and its first label",
          favs.map(function (e) { return e.label }).join(","), "Home,Downloads,Documents,Media")
    check("a non-file bookmark never reaches FAVORITES", favs.length, 4)
    check("every row is tagged as a favourite", favs[3].group + "/" + favs[3].kind, "favorite/favorite")
    check("the mark is resolved by the caller, so the rail keeps its own Icons import", favs[1].glyph, "mark:Downloads")
    check("a box with neither file still gets Home", Places.favorites("/home/gm", "", "", function () { return "m" }).length, 1)
}
