.pragma library

// Adapted verbatim from the OEM tailscale plugin's own Model.js, read at
// /usr/share/omarchy/shell/plugins/panels/tailscale/Model.js: isTaildropTarget,
// displayHostName/shortDnsName/cleanDnsName and the Online/Mullvad filter in parseStatus.

// Sample input: `tailscale status --json`, trimmed to the fields this file reads.
// {"Self":{"UserID":1000000000000001},"Peer":{"nodekey:...":{"HostName":"laptop",
// "DNSName":"laptop.example-net.ts.net.","Online":true,"TaildropTarget":1,
// "TailscaleIPs":["100.1.2.3"],"UserID":1000000000000001}}}
function parsePeers(raw) {
    var text = String(raw || "").trim()
    if (text === "")
        return []
    var data
    try {
        data = JSON.parse(text)
    } catch (e) {
        return []
    }
    if (!data || typeof data !== "object" || Array.isArray(data)) return []
    var selfUserId = String((data.Self && data.Self.UserID) || "")
    var rawPeers = (data && data.Peer) || {}
    var out = []
    for (var id in rawPeers) {
        var peer = rawPeers[id] || {}
        if (!peer.Online)
            continue
        if (isMullvad(peer))
            continue
        if (!isTaildropTarget(peer, selfUserId))
            continue
        var address = peerAddress(peer)
        if (address === "")
            continue
        out.push({ id: id, label: displayHostName(peer.HostName, peer.DNSName), address: address })
    }
    out.sort(function (a, b) { return a.label.localeCompare(b.label) })
    return out
}

// Sample input: {"BackendState":"Running","Self":{"CapMap":{"https://tailscale.com/cap/file-sharing":[]}},"Peer":{}}.
function status(raw, exitCode, error) {
    if (exitCode !== 0) return { peers: [], reason: String(error || "Tailscale status failed.").trim() }
    var data
    try { data = JSON.parse(raw) } catch (e) { return { peers: [], reason: "invalid status" } }
    if (!data || typeof data !== "object") return { peers: [], reason: "invalid status" }
    if (data.BackendState === "NeedsLogin") return { peers: [], reason: "signed out" }
    if (data.BackendState !== "Running") return { peers: [], reason: String(data.BackendState || "unavailable").toLowerCase() }
    var self = data.Self || {}, capability = "https://tailscale.com/cap/file-sharing"
    var capabilities = Array.isArray(self.Capabilities) ? self.Capabilities : []
    if (!(self.CapMap && self.CapMap[capability] !== undefined)
            && capabilities.indexOf(capability) < 0)
        return { peers: [], reason: "disabled for this account" }
    var peers = parsePeers(raw)
    return { peers: peers, reason: peers.length ? "" : "no peers" }
}

// A specific TaildropTarget wins outright; 0 (unset) falls back to "do we own it too".
function isTaildropTarget(peer, selfUserId) {
    var target = peer && peer.TaildropTarget
    if (typeof target === "number" && target !== 0)
        return target === 1
    var owner = String((peer && peer.UserID) || "")
    return owner !== "" && owner === String(selfUserId || "")
}

function cleanDnsName(name) {
    var value = String(name || "")
    return value.charAt(value.length - 1) === "." ? value.slice(0, -1) : value
}

function shortDnsName(name) {
    var clean = cleanDnsName(name)
    return clean === "" ? "" : (clean.split(".")[0] || clean)
}

function displayHostName(hostName, dnsName) {
    var host = String(hostName || "")
    if (host !== "" && host.toLowerCase() !== "localhost")
        return host
    return shortDnsName(dnsName) || host || "Unknown"
}

// An exit-node relay peer, never a real send target; see the OEM's own isMullvadPeer.
function isMullvad(peer) {
    var suffix = ".mullvad.ts.net"
    var dnsName = cleanDnsName((peer && peer.DNSName) || "").toLowerCase()
    var hostName = String((peer && peer.HostName) || "").toLowerCase()
    return endsWith(dnsName, suffix) || endsWith(hostName, suffix)
}

function endsWith(value, suffix) {
    return value.length > suffix.length && value.lastIndexOf(suffix) === value.length - suffix.length
}

// omarchy-tailscale-send resolves its own "$machine" argument the same three ways.
function peerAddress(peer) {
    if (!peer)
        return ""
    if (peer.DNSName)
        return cleanDnsName(peer.DNSName)
    if (peer.HostName)
        return String(peer.HostName)
    var ips = filterIPv4(peer.TailscaleIPs || [])
    return ips.length > 0 ? ips[0] : ""
}

function filterIPv4(ips) {
    var result = []
    if (!ips || typeof ips.length !== "number")
        return result
    for (var i = 0; i < ips.length; i++) {
        var ip = String(ips[i] || "")
        if (/^100\./.test(ip))
            result.push(ip)
    }
    return result
}

function byId(peers, id) {
    for (var i = 0; i < peers.length; i++) {
        if (peers[i].id === id)
            return peers[i]
    }
    return null
}
