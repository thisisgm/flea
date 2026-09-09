.pragma library
.import "Mounts.js" as Mounts

// What a typed smb, sftp or ssh URL is to gvfs: the root that gio mount takes and the path inside
// the mount that is then walked on its FUSE path, the way the Network rail opens a share and none
// of the copying a download needs. Pure, so tests/js/shareurl.js drives all of it and
// ui/ShareResolve.qml only runs the two gio legs it names.

var NAME_SHARE = "Name a share"
var NEEDS_PASSWORD = "Needs a password; add it under Network first"
var SAVE_LOCAL = "Save needs a local path"
var REFUSED = "Connect failed: network location was refused"
var NO_FOLDER = "Connect failed: location has no browsable folder"
var TIMEOUT = "Connect failed: host did not respond"
var BUSY = "Another location is still connecting"

// gio's own wording for a mount that wanted a credential it had nobody to ask for, under the C
// locale ui/ShareResolve.qml pins: the smb refusal, and the sftp prompt that met a closed stdin.
var AUTH = /permission denied|password|not authori[sz]ed|authentication|access denied/i

// The path decoded to the names gvfs shows on the FUSE mount, a lone slash read as nothing; a
// stray percent sign stays as typed rather than throwing.
function decode(path) {
    if (path === "/") {
        return ""
    }
    try {
        return decodeURIComponent(path)
    } catch (e) {
        return path
    }
}

// The mount root and the rest. An smb share is the mount, so the root is scheme://host/share and
// a server root alone names nothing gvfs can hand a folder for; an sftp host is the mount, so the
// root is scheme://host/ and the whole path is the rest. The root is Mounts.normalize's spelling,
// the one the rail mounts, so a share the rail already opened is found mounted rather than twice.
function split(url) {
    var m = /^([a-z][a-z0-9+.-]*):\/\/([^\/]+)(\/.*)?$/i.exec(String(url || ""))
    if (m === null) {
        return { scheme: "", root: "", rest: "", reason: "host" }
    }
    var scheme = m[1].toLowerCase()
    var authority = m[2]
    // A query or a fragment is not a name, so it is cut before the share is read.
    var path = (m[3] || "").replace(/[?#].*$/, "")
    if (scheme === "ssh") {
        scheme = "sftp"
    }
    if (scheme === "sftp") {
        return { scheme: scheme, root: Mounts.normalize(scheme + "://" + authority + "/"),
                 rest: decode(path), reason: "" }
    }
    var share = /^\/([^\/]+)(\/.*)?$/.exec(path)
    if (share === null) {
        return { scheme: scheme, root: "", rest: "", reason: "share" }
    }
    return { scheme: scheme, root: Mounts.normalize(scheme + "://" + authority + "/" + share[1]),
             rest: decode(share[2] || ""), reason: "" }
}

// The two legs as gio is run. An smb root is mounted anonymously, the rail's own rule for a URL
// with no user in it; a user in the URL asks gio, which has no stdin here and fails into the
// password message, unless the rail already mounted it, which the info leg then finds.
function mountCommand(root) {
    if (/^smb:\/\/[^\/@]+\//i.test(root)) {
        return ["gio", "mount", "--anonymous", root]
    }
    return ["gio", "mount", root]
}

function infoCommand(root) {
    return ["gio", "info", root]
}

// The line the window walks once the mount answered its FUSE path: the rest joined on, and a bare
// root asked for as a directory, which is what a share is. ui/js/PickerEntry.js reads the result as
// a typed local path, so dot segments and the trailing slash mean what they mean there.
function localLine(fusePath, rest) {
    var base = String(fusePath || "").replace(/\/+$/, "")
    return base + (rest.length > 0 ? rest : "/")
}

// Why the mount gave no folder. The info leg is the judge, as it is on the rail: a mount that
// failed with gio's credential wording needs the rail's password form, any other failure was a
// refusal, and a mount that said nothing wrong and still has no path has no browsable folder.
function failure(mountFailed, mountStderr) {
    if (!mountFailed) {
        return NO_FOLDER
    }
    return AUTH.test(String(mountStderr || "")) ? NEEDS_PASSWORD : REFUSED
}

function connecting(root) {
    return "Connecting to " + root
}
