.import "../../ui/js/ShareUrl.js" as ShareUrl

// A typed share URL is mounted at its root and walked at the rest, with no copy and no prompt, so
// the split and the sentences the legs answer with are asserted here with no gio and no window.

function run(check) {
    // smb: the share is the mount, the rest is the path inside it.
    var dir = ShareUrl.split("smb://nas/isos/linux/")
    check("an smb share root is the mount", dir.root, "smb://nas/isos")
    check("and the rest is the path inside it", dir.rest, "/linux/")
    check("and the scheme is smb", dir.scheme, "smb")
    check("and nothing is refused", dir.reason, "")
    var file = ShareUrl.split("smb://nas/isos/linux/arch.iso")
    check("an smb file keeps its share root", file.root, "smb://nas/isos")
    check("and its rest names the file", file.rest, "/linux/arch.iso")
    check("a share alone has no rest", ShareUrl.split("smb://nas/isos").rest, "")
    check("a share with a slash has no rest either", ShareUrl.split("smb://nas/isos/").rest, "")
    check("a server root names no share", ShareUrl.split("smb://nas").reason, "share")
    check("and neither does a server root with a slash", ShareUrl.split("smb://nas/").root, "")
    check("a user in the URL stays in the root", ShareUrl.split("smb://gm@nas/isos/x").root, "smb://gm@nas/isos")
    check("a default port is dropped from the root", ShareUrl.split("smb://nas:445/isos").root, "smb://nas/isos")

    // sftp: the host is the mount, the whole path is the rest, and ssh is the same thing.
    var sftp = ShareUrl.split("sftp://gm@box/home/gm/notes.txt")
    check("an sftp host is the mount", sftp.root, "sftp://gm@box/")
    check("and the whole path is the rest", sftp.rest, "/home/gm/notes.txt")
    check("an sftp host alone is a root", ShareUrl.split("sftp://box").root, "sftp://box/")
    check("and asks for nothing", ShareUrl.split("sftp://box").reason, "")
    check("ssh is sftp", ShareUrl.split("ssh://box/etc").scheme, "sftp")
    check("and mounts as sftp", ShareUrl.split("ssh://box/etc").root, "sftp://box/")
    check("a line with no host is refused", ShareUrl.split("smb://").reason, "host")

    // The rest is what gvfs shows, so it is decoded, and a query is not a name.
    check("percent escapes decode", ShareUrl.split("smb://nas/isos/a%20b/c%2Bd").rest, "/a b/c+d")
    check("a broken escape stays as typed", ShareUrl.split("smb://nas/isos/100%").rest, "/100%")
    check("a query is cut", ShareUrl.split("smb://nas/isos/x?dir=1").rest, "/x")
    check("a fragment is cut", ShareUrl.split("sftp://box/x#top").rest, "/x")
    check("a share name is read before the query", ShareUrl.split("smb://nas/isos?x").root, "smb://nas/isos")

    // The legs as gio is run.
    check("an anonymous smb mount", ShareUrl.mountCommand("smb://nas/isos").join(" "), "gio mount --anonymous smb://nas/isos")
    check("a user in the URL asks gio", ShareUrl.mountCommand("smb://gm@nas/isos").join(" "), "gio mount smb://gm@nas/isos")
    check("sftp asks gio", ShareUrl.mountCommand("sftp://box/").join(" "), "gio mount sftp://box/")
    check("info reads the root", ShareUrl.infoCommand("smb://nas/isos").join(" "), "gio info smb://nas/isos")

    // The line the window walks once the FUSE path is known.
    check("the rest joins the FUSE path", ShareUrl.localLine("/run/user/1000/gvfs/smb-share:server=nas,share=isos", "/linux/arch.iso"),
          "/run/user/1000/gvfs/smb-share:server=nas,share=isos/linux/arch.iso")
    check("a bare root is walked as a directory", ShareUrl.localLine("/gvfs/isos", ""), "/gvfs/isos/")
    check("a trailing slash on the path is not doubled", ShareUrl.localLine("/gvfs/isos/", "/x/"), "/gvfs/isos/x/")

    // Why there is no folder, judged after the info leg.
    check("a mount that said nothing wrong has no folder", ShareUrl.failure(false, ""), ShareUrl.NO_FOLDER)
    check("a permission refusal needs the rail's password", ShareUrl.failure(true, "gio: smb://nas/x/: Failed to mount Windows share: Permission denied\n"), ShareUrl.NEEDS_PASSWORD)
    check("a cancelled password prompt needs it too", ShareUrl.failure(true, "gio: sftp://box/: Password dialog cancelled\n"), ShareUrl.NEEDS_PASSWORD)
    check("any other refusal is a refusal", ShareUrl.failure(true, "gio: smb://nas/x/: Failed to mount Windows share: No such file or directory\n"), ShareUrl.REFUSED)
    check("the footer names the root while connecting", ShareUrl.connecting("smb://nas/isos"), "Connecting to smb://nas/isos")
}
