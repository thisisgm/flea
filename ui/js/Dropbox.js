.pragma library

// Sample input: {"personal":{"path":"/home/gm/Dropbox"}}; personal precedes business in the OEM helper.
function account(raw, error) {
    if (error) return { path: "", reason: error }
    if (!raw) return { path: "", reason: "Dropbox is signed out" }
    var data
    try { data = JSON.parse(raw) } catch (e) { return { path: "", reason: "Dropbox account metadata is invalid" } }
    if (!data || typeof data !== "object" || Array.isArray(data))
        return { path: "", reason: "Dropbox account metadata is invalid" }
    for (var i = 0, keys = ["personal", "business"]; i < keys.length; i++) {
        var value = data[keys[i]]
        if (!value || typeof value !== "object" || Array.isArray(value)) continue
        var path = value.path
        if (typeof path !== "string" || path.charAt(0) !== "/" || path === "/" || path.indexOf("\u0000") >= 0)
            return { path: "", reason: "Dropbox account folder is invalid" }
        return { path: path.replace(/\/+$/, ""), reason: "" }
    }
    return { path: "", reason: "Dropbox is signed out" }
}

function contains(root, path) {
    return typeof root === "string" && root.length > 1 && root.charAt(0) === "/"
        && typeof path === "string" && (path === root || path.indexOf(root + "/") === 0)
}

// Sample output: dropbox-cli status prints "Up to date" or "Dropbox isn't running!" even with exit status zero.
function status(output, exitCode, error) {
    var text = String(output || "").trim(), detail = String(error || "").trim()
    if (exitCode !== 0) return detail || text || "Dropbox status failed"
    if (!text) return detail || "Dropbox returned empty status"
    if (detail) return detail
    var lower = text.toLowerCase()
    if (lower.indexOf("not running") >= 0 || lower.indexOf("isn't running") >= 0 || lower === "stopped"
            || text.indexOf("Couldn't get status:") === 0 || text === "Dropbox isn't responding!"
            || text === "Dropbox daemon stopped.")
        return text
    return ""
}
