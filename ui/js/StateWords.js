.pragma library

.import "Format.js" as Format

// Permissions refusals keep their short state words; unavailable app rows use error ink alone.
var WORDS = {
    permissionsLink: "symlink",
    permissionsSpecial: "special file",
    permissionsNotYours: "not yours"
}

// A link names itself; any other kind that is neither a file nor a folder is special, and a mode never read names nothing.
function permissions(mode) {
    var kind = (Number(mode) || 0) & Format.S_IFMT
    if (kind === 0) return ""
    return kind === Format.S_IFLNK ? WORDS.permissionsLink : WORDS.permissionsSpecial
}
