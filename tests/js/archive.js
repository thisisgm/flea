.import "../../ui/js/Archive.js" as Archive
.import "../../ui/js/Convert.js" as Convert

function run(check) {
    check("the longest extension is matched first, so tar.gz is not read as gz",
          Archive.isArchive("backup.tar.gz") + "|" + Archive.extractDir("backup.tar.gz"),
          "true|backup")
    check("every form the table names is an archive",
          [".zip", ".7z", ".tgz", ".tar", ".tar.xz", ".tar.zst", ".tar.bz2"].map(function (e) {
              return Archive.isArchive("x" + e)
          }).join(","),
          "true,true,true,true,true,true,true")
    // Flea reads rar and writes none: the row offers Extract, and src/backend/archive.rs never puts
    // rar in the compress submenu because nothing in the Arch repositories writes one.
    check("a rar is an archive, whatever case it is spelled in",
          Archive.isArchive("holiday.rar") + "|" + Archive.isArchive("HOLIDAY.RAR"), "true|true")
    check("and an extract unpacks it under its own name", Archive.extractDir("holiday.rar"), "holiday")
    check("and nothing else is",
          Archive.isArchive("notes.txt") + "|" + Archive.isArchive("x.zipper") + "|" + Archive.isArchive("zip"),
          "false|false|false")
    check("an extract unpacks into the archive's own name with the extension taken off",
          Archive.extractDir("photos.zip") + "|" + Archive.extractDir("backup.tar.zst"),
          "photos|backup")
    check("a name with no archive extension is its own directory name",
          Archive.extractDir("plain"), "plain")

    check("one row compresses under its own name",
          Archive.archiveStem(["notes.txt"], "claude"), "notes")
    check("several compress under the directory holding them",
          Archive.archiveStem(["a.txt", "b.txt"], "claude"), "claude")
    check("and with no directory name to use, under a plain one",
          Archive.archiveStem(["a.txt", "b.txt"], ""), "archive")

    // The submenu is exactly what the backend probed, so a box with no 7zip never offers .7z.
    check("the submenu is the probed table and never a fixed list",
          Archive.formatEntries(["zip", "tar.zst"]).map(function (e) { return e.id + "=" + e.label }).join("|"),
          "zip=.zip|tar.zst=.tar.zst")
    check("an empty table offers nothing at all",
          Archive.formatEntries([]).length, 0)

    // Convert never writes over the file it was given, whichever of the two words it uses.
    check("a different format is a conversion and the name says so",
          Convert.destName("shot.png", "jpg"), "shot (converted).jpg")
    check("the same format is a strip and the name says that instead",
          Convert.destName("shot.png", "png"), "shot (stripped).png")
    check("a leading dot on the format is accepted",
          Convert.destName("shot.png", ".webp"), "shot (converted).webp")
    check("case does not decide which of the two words it is",
          Convert.destName("shot.PNG", "png"), "shot (stripped).png")
    check("a name with no extension still converts",
          Convert.destName("shot", "jpg"), "shot (converted).jpg")
    check("the format that starts picked is never the one the file already is",
          Convert.defaultFormat("shot.jpg") + "|" + Convert.defaultFormat("shot.png"),
          "png|jpg")
    var source = {path: "/pictures/album.with.dots/photo.png", name: "photo.png", menuId: 17}
    check("the output keeps the captured full source directory",
          Convert.destination(source, "jpg"), "/pictures/album.with.dots/photo (converted).jpg")
    check("a root source has one output separator",
          Convert.destination({path: "/photo.png", name: "photo.png"}, "webp"), "/photo (converted).webp")
    check("an attributed reply matches the captured source",
          Convert.matchesReply(source, 5, {requestId: 5, source: source.path}), true)
    check("an older format probe cannot replace the current result",
          Convert.matchesReply(source, 6, {requestId: 5, source: source.path}), false)
    check("another source cannot complete this dialog even with the same request id",
          Convert.matchesReply(source, 5, {requestId: 5, source: "/elsewhere/photo.png"}), false)
}
