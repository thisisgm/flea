.pragma library

.import "Mounts.js" as Mounts

var LIMIT = 8

function parse(body) {
    return parseState(body).recent
}

function parseState(body) {
    if (String(body || "").trim().length === 0)
        return { recent: [], profiles: [] }
    var value
    try { value = JSON.parse(String(body)) } catch (e) { return { recent: [], profiles: [] } }
    if (!value)
        return { recent: [], profiles: [] }
    var out = []
    var source = Array.isArray(value.recent) ? value.recent : []
    for (var i = 0; i < source.length && out.length < LIMIT; i++) {
        var row = clean(source[i])
        if (row)
            out.push(row)
    }
    var profiles = []
    var rawProfiles = Array.isArray(value.profiles) ? value.profiles : []
    for (var p = 0; p < rawProfiles.length; p++) {
        var profile = clean(rawProfiles[p])
        if (profile && profile.mac.length > 0)
            profiles.push(profile)
    }
    return { recent: dedupe(out), profiles: dedupe(profiles) }
}

// Called only after gio resolved a browsable local path. A failure never reaches this function.
function record(list, uri, label, usedAt, mac) {
    var row = clean({ uri: sanitizeUri(uri), label: label, usedAt: usedAt, mac: mac })
    if (!row)
        return (list || []).slice(0, LIMIT)
    return dedupe([row].concat(list || [])).slice(0, LIMIT)
}

function serialize(list) {
    return serializeState(list, [])
}

function serializeState(list, profiles) {
    return JSON.stringify({ version: 1, recent: (list || []).slice(0, LIMIT), profiles: profiles || [] }, null, 2) + "\n"
}

function rememberProfile(list, oldUri, uri, label, mac) {
    var oldKey = Mounts.normalize(oldUri)
    var newKey = Mounts.normalize(uri)
    var kept = (list || []).filter(function (entry) {
        var key = Mounts.normalize(entry.uri)
        return key !== oldKey && key !== newKey
    })
    var row = clean({ uri: uri, label: label, usedAt: 0, mac: mac })
    if (!row || row.mac.length === 0)
        return kept
    return dedupe([row].concat(kept))
}

function remove(list, uri) {
    var key = Mounts.normalize(uri)
    return (list || []).filter(function (entry) { return Mounts.normalize(entry.uri) !== key })
}

function profileEntries(list) {
    var rows = entries(list)
    for (var i = 0; i < rows.length; i++) {
        rows[i].kind = "profile"
        rows[i].origin = "lan"
        rows[i].glyph = "server"
    }
    return rows
}

function entries(list) {
    var out = []
    for (var i = 0; i < (list || []).length; i++) {
        var row = list[i]
        out.push({ path: "", label: row.label, group: "network", kind: "recent", uri: row.uri,
                   mounted: false, glyph: "history", origin: "recent", health: "unknown",
                   address: authority(row.uri), taildrop: false, mac: row.mac })
    }
    return out
}

function clean(row) {
    if (!row)
        return null
    var uri = sanitizeUri(row.uri)
    if (uri.length === 0)
        return null
    var label = String(row.label || "").replace(/[\r\n]/g, "").trim() || authority(uri)
    var stamp = Number(row.usedAt)
    return { uri: uri, label: label, usedAt: isFinite(stamp) ? stamp : 0, mac: normalizeMac(row.mac) }
}

function sanitizeUri(uri) {
    var raw = String(uri || "").trim().replace(/[?#].*$/, "")
    var m = raw.match(/^([a-z][a-z0-9+.-]*:\/\/)([^\/]*)(.*)$/i)
    if (!m)
        return ""
    var auth = m[2]
    var at = auth.lastIndexOf("@")
    if (at >= 0) {
        var user = auth.substring(0, at).split(":")[0]
        auth = (user.length > 0 ? user + "@" : "") + auth.substring(at + 1)
    }
    return Mounts.normalize(m[1] + auth + m[3])
}

function authority(uri) {
    var m = String(uri || "").match(/^[a-z][a-z0-9+.-]*:\/\/([^\/]+)/i)
    return m ? m[1].replace(/^.*@/, "") : ""
}

function normalizeMac(value) {
    var compact = String(value || "").replace(/[^0-9a-f]/gi, "").toLowerCase()
    return compact.length === 12 ? compact.match(/../g).join(":") : ""
}

function dedupe(list) {
    var out = []
    var seen = {}
    for (var i = 0; i < list.length; i++) {
        var key = Mounts.normalize(list[i].uri)
        if (seen[key])
            continue
        seen[key] = true
        out.push(list[i])
    }
    return out
}
