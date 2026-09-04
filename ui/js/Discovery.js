.pragma library

.import "Mounts.js" as Mounts

var TYPES = {
    "_ssh._tcp": { scheme: "sftp", defaultPort: 22 },
    "_smb._tcp": { scheme: "smb", defaultPort: 445 },
    "_nfs._tcp": { scheme: "nfs", defaultPort: 2049 },
    "_webdav._tcp": { scheme: "dav", defaultPort: 80 },
    "_webdavs._tcp": { scheme: "davs", defaultPort: 443 }
}

// avahi-browse -artp emits semicolon-separated resolved rows beginning with "=". Duplicate rows
// collapse by normalized URI; unresolved, removal, malformed, and unsupported rows are ignored.
function parse(body) {
    var lines = String(body || "").split("\n")
    var out = []
    var seen = {}
    for (var i = 0; i < lines.length; i++) {
        var fields = splitLine(lines[i])
        if (fields.length < 9 || fields[0] !== "=")
            continue
        var spec = TYPES[fields[4]]
        if (!spec)
            continue
        var host = cleanHost(fields[6] || fields[7])
        if (host.length === 0)
            continue
        var port = /^\d+$/.test(fields[8]) ? Number(fields[8]) : spec.defaultPort
        var uri = spec.scheme + "://" + hostForUri(host)
        if (port !== spec.defaultPort)
            uri += ":" + port
        uri = Mounts.normalize(uri + "/")
        if (seen[uri])
            continue
        seen[uri] = true
        out.push({ path: "", label: fields[3] || host, group: "network", kind: "discovered",
                   uri: uri, mounted: false, glyph: "server", origin: "lan", health: "online",
                   address: host, taildrop: false, mac: txtMac(fields.slice(9)) })
    }
    out.sort(function (a, b) { return a.label.localeCompare(b.label) })
    return out
}

function splitLine(line) {
    var fields = String(line || "").split(";")
    for (var i = 0; i < fields.length; i++)
        fields[i] = unescapeField(fields[i])
    return fields
}

function unescapeField(value) {
    return String(value || "").replace(/\\(\d{3})/g, function (_, digits) {
        return String.fromCharCode(Number(digits))
    }).replace(/\\\\/g, "\\")
}

function cleanHost(value) {
    var host = String(value || "").trim().replace(/\.$/, "")
    return /^[A-Za-z0-9._:%-]+$/.test(host) ? host : ""
}

function hostForUri(host) {
    return host.indexOf(":") >= 0 && host.charAt(0) !== "[" ? "[" + host + "]" : host
}

function txtMac(fields) {
    var text = fields.join(" ")
    var m = text.match(/(?:^|[ "'])mac=([0-9a-f]{2}(?::[0-9a-f]{2}){5})(?:$|[ "'])/i)
    return m ? m[1].toLowerCase() : ""
}
