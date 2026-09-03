.pragma library

.import "Mounts.js" as Mounts
.import "Protocols.js" as Protocols

// Human shorthand becomes one explicit GIO URI. A colon path means SFTP, a slash means SMB, and a
// bare host means SFTP. An explicit supported scheme always wins over those conveniences.
function parse(input) {
    var raw = String(input || "").trim()
    if (raw.length === 0 || /[\r\n?#]/.test(raw))
        return null
    if (/^[a-z][a-z0-9+.-]*:\/\//i.test(raw))
        return explicitUri(raw)
    var colon = raw.match(/^((?:[A-Za-z0-9._-]+@)?(?:\[[0-9A-Fa-f:]+\]|[A-Za-z0-9._-]+)):(\/.*)$/)
    if (colon)
        return answer("sftp://" + colon[1] + cleanPath(colon[2]), "SFTP")
    var slash = raw.match(/^([A-Za-z0-9._-]+)\/(.+)$/)
    if (slash)
        return answer("smb://" + slash[1] + cleanPath(slash[2]), "SMB")
    if (/^(?:[A-Za-z0-9._-]+|\[[0-9A-Fa-f:]+\])$/.test(raw))
        return answer("sftp://" + raw + "/", "SFTP")
    return null
}

function explicitUri(raw) {
    var parsed = Protocols.parse(raw)
    if (!parsed || passwordEmbedded(raw) || /[?#]/.test(raw))
        return null
    return answer(raw, parsed.protocol)
}

function passwordEmbedded(uri) {
    var m = String(uri).match(/^[a-z][a-z0-9+.-]*:\/\/([^\/@]*)@/i)
    return !!(m && m[1].indexOf(":") >= 0)
}

function cleanPath(path) {
    var value = String(path || "").replace(/^\/+/, "")
    return value.length > 0 ? "/" + value : "/"
}

function answer(uri, protocol) {
    var normalized = Mounts.normalize(uri)
    var form = Protocols.parse(normalized)
    return form ? { uri: normalized, protocol: protocol, form: form } : null
}
