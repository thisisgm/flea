.pragma library

function isRemotePath(path) {
    return remoteRoot(path).length > 0
}

function remoteRoot(path) {
    var match = String(path || "").match(/^(\/run\/user\/\d+\/gvfs\/[^\/]+)(?:\/|$)/)
    return match ? match[1] : ""
}

function transferKind(paths, destination) {
    var destinationRoot = remoteRoot(destination)
    if (destinationRoot.length === 0)
        return "local"
    var list = paths || []
    if (list.length === 0)
        return "local-to-remote"
    for (var i = 0; i < list.length; i++) {
        var sourceRoot = remoteRoot(list[i])
        if (sourceRoot.length === 0)
            return "local-to-remote"
        if (sourceRoot === destinationRoot)
            return "same-remote"
    }
    return "remote-to-remote"
}

function transferPrefix(kind, moving) {
    var verb = moving ? "Moving" : "Copying"
    return kind === "remote-to-remote" ? verb + " between remote hosts" : verb
}

function menu(entry) {
    if (!entry || entry.group !== "network" || entry.kind === "dropbox" || entry.kind === "status")
        return []
    var rows = []
    if (entry.mounted)
        rows.push(item("Unmount", "unmount", "eject"))
    if (entry.health === "failed" || entry.health === "offline")
        rows.push(item("Reconnect", "reconnect", "refresh-cw"))
    rows.push(item("Copy address", "copy-address", "file-text"))
    if (sshTarget(entry.uri).length > 0 && entry.health !== "offline")
        rows.push(item("Open terminal over SSH", "open-ssh", "terminal"))
    if (entry.taildrop && entry.health === "online")
        rows.push(item("Send with Taildrop", "taildrop-peer", "network"))
    if (entry.origin === "lan" && validMac(entry.mac))
        rows.push(item("Wake", "wake", "power"))
    if (entry.kind !== "peer" && entry.kind !== "discovered") {
        rows.push(item("Edit", "edit", "rename"))
        rows.push(item("Remove", "remove", "trash"))
    }
    return rows
}

function item(label, action, glyph) { return { label: label, action: action, glyph: glyph } }

function sshTarget(uri) {
    var spec = sshSpec(uri)
    return spec ? spec.target : ""
}

function terminalArgv(entry) {
    var spec = sshSpec(entry && entry.uri)
    if (!spec)
        return []
    var out = ["omarchy-launch-terminal", "ssh"]
    if (spec.port.length > 0)
        out = out.concat(["-p", spec.port])
    out.push(spec.target)
    return out
}

function sshSpec(uri) {
    var m = String(uri || "").match(/^(?:sftp|ssh):\/\/((?:[A-Za-z0-9._%-]+@)?)(\[[0-9A-Fa-f:%._-]+\]|[A-Za-z0-9._-]+)(?::(\d+))?(?:\/.*)?$/i)
    if (!m)
        return null
    var port = m[3] || ""
    if (port.length > 0 && (Number(port) < 1 || Number(port) > 65535))
        return null
    var host = m[2].charAt(0) === "[" ? m[2].slice(1, -1) : m[2]
    return { target: m[1] + host, port: port }
}

function validMac(value) {
    return /^[0-9a-f]{2}(?::[0-9a-f]{2}){5}$/i.test(String(value || ""))
}
