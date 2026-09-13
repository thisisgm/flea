.import "../../ui/js/Icons.js" as Icons

function run(check) {
    check("a directory", Icons.glyphFor("folder"), "folder")
    check("plain text", Icons.glyphFor("text-x-generic"), "file-text")
    check("an image", Icons.glyphFor("image-x-generic"), "image")
    check("a video", Icons.glyphFor("video-x-generic"), "film")
    check("audio", Icons.glyphFor("audio-x-generic"), "music")
    check("an archive", Icons.glyphFor("package-x-generic"), "archive")
    check("a program", Icons.glyphFor("application-x-executable"), "terminal")
    check("a script", Icons.glyphFor("text-x-script"), "code")
    check("markup", Icons.glyphFor("text-html"), "code")
    check("xml", Icons.glyphFor("application-xml"), "code")
    check("plain text is still a document", Icons.glyphFor("text-plain"), "file-text")
    check("an office document", Icons.glyphFor("x-office-document"), "file-text")

    // FleaWindow.html and GridView.html both draw a symlink row with the link mark, and the backend
    // resolves a link's icon to its target's, so the mode is what decides and the icon name cannot.
    var LNK = 0o120777
    var REG = 0o100644
    check("a symlink to a directory draws the link mark, not the folder its icon names",
          Icons.glyphForRow("folder", LNK), "symlink")
    check("a symlink to a file draws it too", Icons.glyphForRow("text-x-generic", LNK), "symlink")
    check("a real directory is untouched", Icons.glyphForRow("folder", REG), "folder")
    check("and so is every other row", Icons.glyphForRow("image-x-generic", REG), "image")
    check("a row with no mode at all falls back to the icon name",
          Icons.glyphForRow("text-x-generic", 0), "file-text")

    // GM ruled the brand marks are reproduced from the official artwork, not recut, so neither is a
    // cut glyph and neither belongs in PATHS. The recut strings must not come back.
    check("tailscale is not a cut glyph", Icons.PATHS["tailscale"] === undefined, true)
    check("dropbox is not a cut glyph", Icons.PATHS["dropbox"] === undefined, true)
    check("a spreadsheet", Icons.glyphFor("x-office-spreadsheet"), "table")
    check("a presentation", Icons.glyphFor("x-office-presentation"), "presentation")
    check("a font", Icons.glyphFor("font-x-generic"), "type")
    check("the 190 typeless types", Icons.glyphFor("application-x-generic"), "file")
    check("a name the map has never seen", Icons.glyphFor("x-office-spreadsheet-template"), "file")
    check("an empty name still answers", Icons.glyphFor(""), "file")

    // A name missing from PATHS resolves to the file mark in silence, so "has path data" is not
    // enough: every name but "file" itself must also come back different from that fallback.
    function drawsItsOwnMark(name) {
        check("path data for " + name, Icons.pathFor(name).length > 0, true)
        if (name !== "file") {
            check(name + " is its own mark, not the silent file fallback", Icons.pathFor(name) === Icons.pathFor("file"), false)
        }
    }

    // Every glyph name the two maps can produce must resolve to real path data, or the row draws nothing.
    var rowNames = ["folder", "file-text", "code", "image", "film", "music", "archive", "type", "terminal", "file", "table", "presentation"]
    for (var i = 0; i < rowNames.length; i++) {
        drawsItsOwnMark(rowNames[i])
    }

    // The sidebar's own map, keyed on the favourite's label rather than a freedesktop icon name.
    check("home", Icons.sidebarGlyphFor("Home"), "house")
    check("downloads", Icons.sidebarGlyphFor("Downloads"), "download")
    check("documents", Icons.sidebarGlyphFor("Documents"), "file-text")
    check("pictures", Icons.sidebarGlyphFor("Pictures"), "image")
    check("videos", Icons.sidebarGlyphFor("Videos"), "film")
    check("music folder", Icons.sidebarGlyphFor("Music"), "music")
    check("projects", Icons.sidebarGlyphFor("Projects"), "folder-git-2")
    check("an unknown bookmark falls back to folder", Icons.sidebarGlyphFor("NAS"), "folder")
    check("an empty label still answers", Icons.sidebarGlyphFor(""), "folder")

    // The Devices group's own mark. A name missing from PATHS falls back to "file" in silence,
    // which on the rail reads as a row of documents where the disks should be.
    drawsItsOwnMark("drive")

    // The picker rail's Recent mark, which reaches ui/PickerPlaces.qml by name and falls into the
    // same silent trap if PATHS never learns it.
    drawsItsOwnMark("history")
    check("Preview keeps the board's right-hand column", Icons.pathFor("preview"), "M3 3h18v18H3z M15 3v18")
    check("Folders first keeps the board's stacked rules", Icons.pathFor("folders-first"), "M2 12V4h6l2 2h12v6H2z M2 17h20 M2 21h20")

    var sidebarNames = ["house", "download", "file-text", "image", "film", "music", "folder-git-2", "folder"]
    for (var j = 0; j < sidebarNames.length; j++) {
        drawsItsOwnMark(sidebarNames[j])
    }

    // ui/StateMessage.qml draws these two, and Locked is the newer one: the lock reached a call site
    // before it reached PATHS, which is the silent-fallback trap the drive mark already fell into.
    drawsItsOwnMark("alert")
    drawsItsOwnMark("lock")
    check("lock is the padlock geometry, not some other mark",
          Icons.pathFor("lock"), "M4 11h16v10H4z M8 11V7a4 4 0 0 1 8 0v4 M12 15h.01")

    // ui/ContextMenu.qml's Open mark. Both its forms are hand-drawn, so the grid rule that settled
    // the other seven does not reach it and internal consistency with the closed folder decides it.
    drawsItsOwnMark("folder-open")
    var backPanel = Icons.pathFor("folder").replace("v14H2z", "")
    check("folder-open opens on the closed folder's own back panel",
          Icons.pathFor("folder-open").indexOf(backPanel) === 0, true)
    check("folder-open is the canvas geometry",
          Icons.pathFor("folder-open"), "M2 20V3h6l2 3h12v3 M22 11l-2.5 9H2l2.5-9z")

    // ui/ContextMenu.qml's rail rows draw this one, the shelf's "for: unmount", so both Eject on a
    // removable volume and Unmount on a network share take it.
    drawsItsOwnMark("eject")
    check("eject is the recut lucide geometry, not the canvas's own hand variant",
          Icons.pathFor("eject"), "M12 2 22 13H2z M3 17h18v4H3z")

    // The background menu's Sort by row. IconSets.html names this mark and Menus.html draws it, and
    // its short segments are the reason that board caps the optional corner radius at 0.8.
    drawsItsOwnMark("sort")
    check("sort is the board's own geometry, three rules over a bare down arrow",
          Icons.pathFor("sort"), "M11 5h10 M11 9h7 M11 13h4 M7 5v14 M7 19l-3-3 M7 19l3-3")

    // ui/ContextMenu.qml's New Folder row. The mark is drawn on Main.dc.html's specimen sheet with
    // no consumer, and GM's ruling is that recut lucide geometry wins over the board's hand drawing.
    drawsItsOwnMark("folder-plus")
    check("folder-plus keeps the closed folder's own body, so the two marks cannot drift",
          Icons.pathFor("folder-plus").indexOf(Icons.pathFor("folder")) === 0, true)
    check("folder-plus is the recut lucide plus, not the board's 5 unit one",
          Icons.pathFor("folder-plus"), "M2 20V3h6l2 3h12v14H2z M12 10v6 M9 13h6")
}
