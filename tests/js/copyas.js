.import "../../ui/js/CopyAs.js" as CopyAs

function run(check) {
    check("quoted wraps paths without special characters too",
        CopyAs.lines(["/home/gm/a.txt"], "quoted"), "'/home/gm/a.txt'")
    check("path keeps a space whole on its own line",
        CopyAs.lines(["/home/gm/directory two/a.txt"], "path"), "/home/gm/directory two/a.txt")
    check("name takes the leaf",
        CopyAs.lines(["/home/gm/directory two/a.txt"], "name"), "a.txt")
    check("stem drops the last extension",
        CopyAs.lines(["/home/gm/archive.tar.gz"], "stem"), "archive.tar")
    check("stem keeps a dotfile whole",
        CopyAs.lines(["/home/gm/.bashrc"], "stem"), ".bashrc")
    check("folder path takes the directory",
        CopyAs.lines(["/home/gm/directory two/a.txt"], "dirpath"), "/home/gm/directory two")
    check("uri encodes a space",
        CopyAs.lines(["/home/gm/directory two/a.txt"], "uri"), "file:///home/gm/directory%20two/a.txt")
    check("uri encodes hash and query bytes",
        CopyAs.lines(["/home/gm/a#b?c.txt"], "uri"), "file:///home/gm/a%23b%3Fc.txt")
    check("quoted wraps a space",
        CopyAs.lines(["/home/gm/directory two"], "quoted"), "'/home/gm/directory two'")
    check("quoted escapes an embedded quote",
        CopyAs.lines(["/home/gm/a'b"], "quoted"), "'/home/gm/a'\\''b'")
    check("quoted wraps a newline",
        CopyAs.lines(["/home/gm/a\nb"], "quoted"), "'/home/gm/a\nb'")
    check("quoted keeps a lossy non-UTF-8 name intact",
        CopyAs.lines(["/home/gm/a�b"], "quoted"), "'/home/gm/a�b'")
    check("every variant covers the whole selection one per line",
        CopyAs.lines(["/a/one.txt", "/a/two.txt"], "path"), "/a/one.txt\n/a/two.txt")
    check("path stays absolute when a listing base is given",
        CopyAs.lines(["/home/gm/proj/a.txt"], "path", "/home/gm/proj"), "/home/gm/proj/a.txt")
    check("relative takes the folders under the listed directory",
        CopyAs.lines(["/home/gm/proj/src/a.rs"], "relative", "/home/gm/proj"), "src/a.rs")
    check("relative of a file in the listing is the name",
        CopyAs.lines(["/home/gm/proj/a.txt"], "relative", "/home/gm/proj"), "a.txt")
    check("relative of the listed directory itself is a dot",
        CopyAs.lines(["/home/gm/proj"], "relative", "/home/gm/proj"), ".")
    check("relative from root drops the leading slash",
        CopyAs.lines(["/etc/hostname"], "relative", "/"), "etc/hostname")
    check("relative walks up when the file sits outside the listing",
        CopyAs.lines(["/home/gm/other/a.txt"], "relative", "/home/gm/proj"), "../other/a.txt")
    check("relative without a listing keeps the absolute path",
        CopyAs.lines(["/home/gm/a.txt"], "relative"), "/home/gm/a.txt")
    check("relative covers the whole selection one per line",
        CopyAs.lines(["/d/one.txt", "/d/sub/two.txt"], "relative", "/d"), "one.txt\nsub/two.txt")
}
