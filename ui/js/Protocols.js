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
    // The port is always spelled out, its own default included, because that is what the Mounts-as
    // line promises to show; ui/js/Mounts.js normalize is what takes a default back off for the rail.
    var port = String(form.port || "").trim()
    if (port.length > 0) {
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

// Scheme to default port, derived from the two tables the form itself uses rather than copied into
// a third that can disagree with them, and one of them did: PORTS is keyed by the protocol the form
// picked, "scheme()" above is every scheme that protocol can build, and a bookmarked or gio-reported
// URI only ever carries the scheme, so it is measured against the port the dialog would have written.
function defaultPortFor(uriScheme) {
    var want = String(uriScheme || "").toLowerCase()
    for (var i = 0; i < PROTOCOLS.length; i++) {
        if (scheme(PROTOCOLS[i], true) === want || scheme(PROTOCOLS[i], false) === want)
            return PORTS[PROTOCOLS[i]]
    }
    return 0
}

// "gio mount -l" never reports a scheme's default port and the add form spells out the one it
// prefilled, so the same share arrives spelled two ways and must resolve to one rail row. The port
// is found in the authority alone: a ":" or an "@" further along belongs to the path.
function stripDefaultPort(uri) {
    var text = String(uri || "")
    var auth = authority(text)
    var tail = ":" + defaultPortFor(schemeOf(text))
    // A bracketed IPv6 literal ends in "]", so its own trailing digits can never match this tail.
    if (auth.length === 0 || tail === ":0" || auth.substring(auth.length - tail.length) !== tail)
        return text
    var mark = text.indexOf("://") + 3
    return text.substring(0, mark) + auth.substring(0, auth.length - tail.length)
        + text.substring(mark + auth.length)
}

// The scheme a built or bookmarked URI carries, lowercased; "" when the text is not a URI at all.
function schemeOf(uri) {
    var text = String(uri || "")
    var mark = text.indexOf("://")
    return mark < 0 ? "" : text.substring(0, mark).toLowerCase()
}

// Everything between "://" and the next "/", which is the only place a userinfo, a host or a port
// can live: a scan over the whole URI would read a "@" or a ":" inside the path as one of those.
function authority(uri) {
    var text = String(uri || "")
    var mark = text.indexOf("://")
    if (mark < 0)
        return ""
    var rest = text.substring(mark + 3)
    var slash = rest.indexOf("/")
    return slash < 0 ? rest : rest.substring(0, slash)
}

// The host alone: userinfo dropped at the authority's own last "@", a bracketed IPv6 literal kept
// whole because its colons are not a port separator, and any real port dropped.
function hostOf(uri) {
    var auth = authority(uri)
    var at = auth.lastIndexOf("@")
    var hostPort = at < 0 ? auth : auth.substring(at + 1)
    if (hostPort.charAt(0) === "[") {
        var close = hostPort.indexOf("]")
        return close < 0 ? hostPort : hostPort.substring(0, close + 1)
    }
    var colon = hostPort.indexOf(":")
    return colon < 0 ? hostPort : hostPort.substring(0, colon)
}

// gvfsd renders a network mount as "<share> <word> <host>" and translates that word, so issue #36's
// rule reads the URI instead: the tail that gets cut is this URI's own host, in any language, and a
// label that does not end in it is left exactly as gio gave it.
function shareName(rawLabel, uri) {
    var text = String(rawLabel || "")
    var host = hostOf(uri)
    var head = text.substring(0, text.length - host.length)
    if (host.length === 0 || head.length === 0 || text.substring(head.length) !== host)
        return text
    // Whatever language it is in, exactly one whitespace-delimited word separates the two halves, so
    // a head this does not shorten was never a "<share> <word> <host>" render and is left whole.
    var name = head.replace(/\s+\S+\s+$/, "")
    return name.length > 0 && name !== head ? name : text
}
