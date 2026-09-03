.import "../../ui/js/Places.js" as Places

function run(check) {
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

    check("replace rewrites the matched URI and label",
        Places.replace(netFile, "smb://192.168.1.10/data", "sftp://tom@nas:22/home/tom", "omv"),
        'file:///home/gm/Downloads Downloads\n'
        + 'sftp://tom@nas:22/home/tom omv\n'
        + 'smb://10.0.0.9/backups Backups\n')
    check("a trailing-slash live uri still finds the written line",
        Places.replace(netFile, "smb://192.168.1.10/data/", "smb://192.168.1.10/isos", "ISOs"),
        'file:///home/gm/Downloads Downloads\n'
        + 'smb://192.168.1.10/isos ISOs\n'
        + 'smb://10.0.0.9/backups Backups\n')
    check("an empty old uri appends, which is the add dialog",
        Places.replace(netFile, "", "smb://host/share", "Share"),
        netFile + "smb://host/share Share\n")
    check("an unmatched old uri appends rather than dropping the edit",
        Places.replace(netFile, "smb://missing/x", "smb://host/share", "Share"),
        netFile + "smb://host/share Share\n")
    check("an empty new uri is a no-op", Places.replace(netFile, "smb://192.168.1.10/data", "", "X"), netFile)
    check("a blank label falls back to the path leaf",
        Places.replace("", "smb://h/data", "smb://h/data", "  "), "smb://h/data data\n")
    check("an embedded newline in the new label cannot fork a line",
        Places.replace(netFile, "smb://192.168.1.10/data", "smb://192.168.1.10/data", "Home\nlab"),
        'file:///home/gm/Downloads Downloads\n'
        + 'smb://192.168.1.10/data Homelab\n'
        + 'smb://10.0.0.9/backups Backups\n')
    check("every matching duplicate rewrites to the new uri",
        Places.replace(dupes, "smb://192.168.1.10/data", "sftp://nas/data", "Homelab"),
        'sftp://nas/data Homelab\n'
        + 'file:///home/gm/Downloads Downloads\n'
        + 'sftp://nas/data Homelab\n')

    check("remove drops the matched line and keeps the rest",
        Places.remove(netFile, "smb://192.168.1.10/data"),
        'file:///home/gm/Downloads Downloads\n'
        + 'smb://10.0.0.9/backups Backups\n')
    check("a trailing-slash live uri still finds the written line to drop",
        Places.remove(netFile, "smb://192.168.1.10/data/"),
        'file:///home/gm/Downloads Downloads\n'
        + 'smb://10.0.0.9/backups Backups\n')
    check("an empty uri is a no-op", Places.remove(netFile, ""), netFile)
    check("every matching duplicate is dropped",
        Places.remove(dupes, "smb://192.168.1.10/data"),
        'file:///home/gm/Downloads Downloads\n')
}
