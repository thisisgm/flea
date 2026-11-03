.pragma library

// The hosts the network offers, read the way every Linux desktop reads them: avahi's own
// browse, one "= iface proto Name Service Domain" line per resolved service followed by an
// indented "hostname = [host.local]" line, then "--" (sample input in tests/js/dnssd.js,
// captured from a live LAN). The service's own Name labels the row and the hostname beneath it
// is the row's target, because a service name and its host are free to differ. Two service
// types are read from one browse: SMB is the row a host offers when it offers it, and SSH's
// SFTP is the row a host that offers no SMB still gets, because gvfs mounts either root.
//
// The browse covers every service on the wire, so a line for a service this does not read — a
// printer, a speaker — must not leave the previous service pending: its hostname line would
// then be filed under the wrong row. Every "=" line ends the previous service, and only a
// known type opens a new one.

var SMB_LINE = /^=\s*\S+\s+\S+\s+(.+?)\s+Microsoft Windows Network\s+local\s*$/
var SFTP_LINE = /^=\s*\S+\s+\S+\s+(.+?)\s+SFTP File Transfer\s+local\s*$/
var HOST_LINE = /^\s*hostname\s*=\s*\[([^\]]+)\]\s*$/

function parse(text) {
    var out = []
    var seen = ({})
    var pending = ""
    var pendingKind = ""
    var lines = String(text || "").split("\n")
    for (var i = 0; i < lines.length; i++) {
        var line = lines[i]
        var host = line.match(HOST_LINE)
        if (host && pending) {
            // Two services on one host keep one row, and the SMB one wins the target: a host
            // that offers both is reached the way every other desktop reaches it.
            var entry = seen[host[1]]
            if (!entry) {
                entry = { label: pending, host: host[1], kind: pendingKind }
                seen[host[1]] = entry
                out.push(entry)
            } else if (pendingKind === "smb") {
                entry.kind = "smb"
            }
            pending = ""
            pendingKind = ""
            continue
        }
        if (line.charAt(0) === "=" || line.charAt(0) === "+") {
            // Any new service line ends the last one, known or not. "=" is a resolved service
            // and "+" an unresolved one, which still owns the hostname line beneath it.
            pending = ""
            pendingKind = ""
            var smb = line.match(SMB_LINE)
            if (smb) { pending = smb[1]; pendingKind = "smb"; continue }
            var sftp = line.match(SFTP_LINE)
            if (sftp) { pending = sftp[1]; pendingKind = "sftp"; continue }
            continue
        }
        if (line.indexOf("--") === 0) { pending = ""; pendingKind = "" }
    }
    out.sort(function (a, b) { return a.label.localeCompare(b.label) })
    return out
}

// A host row activates on the bare server root, which an open answers by listing its shares;
// an SSH-only host has no shares, so its root is the whole machine and opens directly.
function hostUri(entry) {
    var scheme = entry && entry.kind === "sftp" ? "sftp://" : "smb://"
    var host = entry ? entry.host : ""
    return scheme + String(host || "") + "/"
}

// showmount's answer is one export path per line, each followed by the client spec; the path
// is the whole target and the client column is dropped.
var EXPORT_LINE = /^(\S+)\s+/

function parseExports(text) {
    var out = []
    var lines = String(text || "").split("\n")
    for (var i = 0; i < lines.length; i++) {
        var line = lines[i]
        if (line.indexOf("/") !== 0)
            continue
        var match = line.match(EXPORT_LINE)
        if (match)
            out.push(match[1])
    }
    return out
}
