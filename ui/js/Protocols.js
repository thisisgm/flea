.pragma library

// The Add-network-place form is a form over a gvfs URI, never a second mount system: a protocol
// picks the scheme, prefills the port and swaps the field set, and the Mounts-as line always shows
// the exact URI the dialog will hand to gio mount.

// The five the canvas draws, in the order it draws them.
var PROTOCOLS = ["SMB", "SFTP", "FTPS", "WebDAV", "NFS"]

// Default ports, from the canvas: SMB 445, SFTP 22, FTPS 21, WebDAV 443, NFS 2049.
var PORTS = { "SMB": 445, "SFTP": 22, "FTPS": 21, "WebDAV": 443, "NFS": 2049 }

// Which fields each protocol asks for. "path" is the label the canvas gives that row, which differs
// per protocol because the thing it names differs: a share, a remote path, an export.
var FIELDS = {
    "SMB":    { pathLabel: "Share",  credentials: true,  domain: true,  tls: false },
    "SFTP":   { pathLabel: "Path",   credentials: true,  domain: false, tls: false },
    "FTPS":   { pathLabel: "Path",   credentials: true,  domain: false, tls: true },
    "WebDAV": { pathLabel: "Path",   credentials: true,  domain: false, tls: true },
    // NFS has no credentials at all: the export is authorised by the server, not by a password.
    "NFS":    { pathLabel: "Export", credentials: false, domain: false, tls: false }
}

// The TLS box flips two schemes and defaults on, which is what the canvas draws ticked.
function scheme(protocol, tls) {
    switch (protocol) {
    case "SMB": return "smb"
    case "SFTP": return "sftp"
    case "FTPS": return tls ? "ftps" : "ftp"
    case "WebDAV": return tls ? "davs" : "dav"
    case "NFS": return "nfs"
    }
    return ""
}

function fieldsFor(protocol) {
    return FIELDS[protocol] || FIELDS["SMB"]
}

function defaultPort(protocol) {
    return PORTS[protocol] || 0
}

// The exact URI gio mount will be handed. Empty when there is not enough to build one, so the
// Mounts-as line shows nothing rather than a half-formed address.
function uri(form) {
    var host = String(form.host || "").trim()
    if (host.length === 0) {
        return ""
    }
    var spec = fieldsFor(form.protocol)
    var head = scheme(form.protocol, form.tls) + "://"
    if (spec.credentials) {
        head += userPart(form, spec)
    }
    head += host
    var port = String(form.port || "").trim()
    if (port.length > 0 && Number(port) !== defaultPort(form.protocol)) {
        head += ":" + port
    } else if (port.length > 0) {
        head += ":" + port
    }
    return head + pathPart(form.path)
}

// smb://DOMAIN;user@host/share is the form a domain takes; without one it is just user@host.
function userPart(form, spec) {
    var user = String(form.user || "").trim()
    if (user.length === 0) {
        return ""
    }
    var domain = spec.domain ? String(form.domain || "").trim() : ""
    return (domain.length > 0 ? domain + ";" : "") + user + "@"
}

function pathPart(path) {
    var text = String(path || "").trim()
    if (text.length === 0) {
        return "/"
    }
    return text.charAt(0) === "/" ? text : "/" + text
}

// The label the sidebar row will carry: what the operator typed, or the last part of the path.
function label(form) {
    var given = String(form.label || "").trim()
    if (given.length > 0) {
        return given
    }
    var path = String(form.path || "").replace(/\/+$/, "")
    var cut = path.lastIndexOf("/")
    var leaf = cut >= 0 ? path.substring(cut + 1) : path
    return leaf.length > 0 ? leaf : String(form.host || "").trim()
}

// A form with nothing to mount cannot be saved, which is what greys the Save row out.
function complete(form) {
    return String(form.host || "").trim().length > 0
}

function protocolFor(schemeName) {
    switch (String(schemeName || "").toLowerCase()) {
    case "smb": return "SMB"
    case "sftp": return "SFTP"
    case "ftp":
    case "ftps": return "FTPS"
    case "dav":
    case "davs": return "WebDAV"
    case "nfs": return "NFS"
    }
    return ""
}

// Inverse of uri(): a bookmark line's address becomes the form fields that would rebuild it.
// corner: a host with a colon that is not a port (IPv6 without brackets) is refused rather than
// split, because the form has no place to round-trip that spelling.
function parse(uri) {
    var raw = String(uri || "").trim()
    var m = raw.match(/^([a-z][a-z0-9+.-]*):\/\/(.*)$/i)
    if (!m)
        return null
    var schemeName = m[1].toLowerCase()
    var protocol = protocolFor(schemeName)
    if (!protocol)
        return null
    var rest = m[2]
    var user = ""
    var domain = ""
    var at = rest.lastIndexOf("@")
    if (at >= 0) {
        var creds = rest.substring(0, at)
        rest = rest.substring(at + 1)
        var semi = creds.indexOf(";")
        if (semi >= 0) {
            domain = creds.substring(0, semi)
            user = creds.substring(semi + 1)
        } else {
            user = creds
        }
    }
    var path = "/"
    var slash = rest.indexOf("/")
    if (slash >= 0) {
        path = rest.substring(slash)
        rest = rest.substring(0, slash)
    }
    var host = rest
    var port = ""
    var colon = rest.lastIndexOf(":")
    if (colon >= 0) {
        var maybePort = rest.substring(colon + 1)
        if (!/^\d+$/.test(maybePort))
            return null
        host = rest.substring(0, colon)
        port = maybePort
    }
    if (host.length === 0)
        return null
    if (port.length === 0)
        port = String(defaultPort(protocol))
    return {
        protocol: protocol,
        host: host,
        port: port,
        path: path === "/" ? "" : path.replace(/^\/+/, ""),
        domain: domain,
        user: user,
        tls: schemeName === "davs" || schemeName === "ftps"
    }
}
