.pragma library

// Tailscale supplies reachability and identity, never a filesystem. These peers become ordinary
// SFTP locations (or a protocol the operator later chooses) and keep Taildrop as a separate action.
function parseResult(exitCode, stdout, stderr) {
    var error = String(stderr || "").toLowerCase()
    if (exitCode !== 0) {
        if (/not found|no such file|failed to start/.test(error))
            return result("missing", "Tailscale is unavailable; install its CLI to discover tailnet peers.", [])
        if (/tailscaled|failed to connect|not running/.test(error))
            return result("daemon-stopped", "Tailscale is stopped; run sudo systemctl enable --now tailscaled.", [])
        if (/logged out|needs login|not logged in/.test(error))
            return result("logged-out", "Tailscale is logged out; run sudo tailscale up in a terminal.", [])
        return result("failed", "Tailscale status could not be read.", [])
    }
    var data
    try {
        data = JSON.parse(String(stdout || ""))
    } catch (e) {
        return result("failed", "Tailscale returned an unreadable status.", [])
    }
    var state = String((data && data.BackendState) || "")
    if (state === "NeedsLogin" || state === "NoState")
        return result("logged-out", "Tailscale is logged out; run sudo tailscale up in a terminal.", [])
    if (state === "Stopped")
        return result("daemon-stopped", "Tailscale is stopped; run sudo systemctl enable --now tailscaled.", [])
    if (state !== "Running")
        return result("failed", "Tailscale is not ready (" + (state || "unknown state") + ").", [])
    var peers = parsePeers(data)
    return peers.length > 0
        ? result("ready", "Tailscale peers are ready.", peers)
        : result("no-peers", "No machines were found on this tailnet.", [])
}

function result(state, message, peers) {
    return { state: state, message: message, peers: peers }
}

function parsePeers(data) {
    var raw = (data && data.Peer) || {}
    var selfUserId = String((data && data.Self && data.Self.UserID) || "")
    var out = []
    for (var id in raw) {
        var peer = raw[id] || {}
        if (isMullvad(peer))
            continue
        var address = peerAddress(peer)
        if (address.length === 0)
            continue
        out.push({ id: id, label: displayHostName(peer.HostName, peer.DNSName), address: address,
                   online: !!peer.Online, taildrop: isTaildropTarget(peer, selfUserId), origin: "tailnet" })
    }
    out.sort(function (a, b) {
        if (a.online !== b.online)
            return a.online ? -1 : 1
        return a.label.localeCompare(b.label)
    })
    return out
}

function entries(peers, user) {
    var out = []
    var login = safeUser(user)
    for (var i = 0; i < (peers || []).length; i++) {
        var peer = peers[i]
        var authority = hostForUri(peer.address)
        if (login.length > 0)
            authority = login + "@" + authority
        out.push({ path: "", label: peer.label, group: "network", kind: "peer",
                   uri: "sftp://" + authority + "/", mounted: false, glyph: "network",
                   origin: "tailnet", health: peer.online ? "online" : "offline",
                   address: peer.address, peerId: peer.id, taildrop: !!peer.taildrop, mac: "" })
    }
    return out
}

function safeUser(user) {
    var value = String(user || "").trim()
    return /^[A-Za-z0-9._-]+$/.test(value) ? value : ""
}

function hostForUri(host) {
    var value = String(host || "")
    return value.indexOf(":") >= 0 && value.charAt(0) !== "[" ? "[" + value + "]" : value
}

function cleanDnsName(name) {
    var value = String(name || "")
    return value.charAt(value.length - 1) === "." ? value.slice(0, -1) : value
}

function displayHostName(hostName, dnsName) {
    var host = String(hostName || "")
    if (host !== "" && host.toLowerCase() !== "localhost")
        return host
    var clean = cleanDnsName(dnsName)
    return (clean === "" ? "" : clean.split(".")[0]) || host || "Unknown"
}

function isMullvad(peer) {
    var dns = cleanDnsName((peer && peer.DNSName) || "").toLowerCase()
    var host = String((peer && peer.HostName) || "").toLowerCase()
    return endsWith(dns, ".mullvad.ts.net") || endsWith(host, ".mullvad.ts.net")
}

function endsWith(value, suffix) {
    return value.length > suffix.length && value.lastIndexOf(suffix) === value.length - suffix.length
}

function peerAddress(peer) {
    if (!peer)
        return ""
    if (peer.DNSName)
        return cleanDnsName(peer.DNSName)
    if (peer.HostName)
        return String(peer.HostName)
    var ips = peer.TailscaleIPs || []
    for (var i = 0; i < ips.length; i++) {
        if (/^100\./.test(String(ips[i])))
            return String(ips[i])
    }
    return ""
}

function isTaildropTarget(peer, selfUserId) {
    var target = peer && peer.TaildropTarget
    if (typeof target === "number" && target !== 0)
        return target === 1
    var owner = String((peer && peer.UserID) || "")
    return owner !== "" && owner === String(selfUserId || "")
}
