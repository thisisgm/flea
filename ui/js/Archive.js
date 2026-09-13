.pragma library

// Which rows get an Extract, and what a new archive is called. The backend never sends an is-archive
// flag: the client is given the icon name and never the MIME type, so this is the whole mechanism.

// Longest form first, so ".tar.gz" is matched before ".gz" could be. Flea reads rar and cannot write
// one, so .rar is here, where Extract is decided, and never in the compress submenu, which is the
// table src/backend/archive.rs probed.
var EXTENSIONS = [".tar.zst", ".tar.bz2", ".tar.gz", ".tar.xz", ".tgz", ".tar", ".zip", ".7z", ".rar"]

function isArchive(name) {
    var lower = String(name).toLowerCase()
    for (var i = 0; i < EXTENSIONS.length; i++) {
        if (lower.length > EXTENSIONS[i].length && lower.indexOf(EXTENSIONS[i], lower.length - EXTENSIONS[i].length) !== -1) {
            return true
        }
    }
    return false
}

// The name an extract unpacks into: the archive's own, with every archive extension taken off.
function extractDir(name) {
    var text = String(name)
    var lower = text.toLowerCase()
    for (var i = 0; i < EXTENSIONS.length; i++) {
        if (lower.length > EXTENSIONS[i].length && lower.indexOf(EXTENSIONS[i], lower.length - EXTENSIONS[i].length) !== -1) {
            return text.substring(0, text.length - EXTENSIONS[i].length)
        }
    }
    return text
}

// One row compresses under its own name; several compress under the directory holding them.
function archiveStem(names, parentLeaf) {
    if (names.length === 1) {
        return stripExtension(names[0])
    }
    return parentLeaf.length > 0 ? parentLeaf : "archive"
}

function stripExtension(name) {
    var text = String(name)
    var cut = text.lastIndexOf(".")
    return cut > 0 ? text.substring(0, cut) : text
}

// The compress submenu is exactly the table the backend probed, never a fixed list, so a box with
// no 7zip installed simply never offers .7z.
function formatEntries(formats) {
    var out = []
    for (var i = 0; i < formats.length; i++) {
        out.push({ id: formats[i], label: "." + formats[i] })
    }
    return out
}
