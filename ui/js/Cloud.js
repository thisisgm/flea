.pragma library
.import "Format.js" as Format

// Only the native read-only probe supplies states. Never infer upload completion from file sizes.
function empty(state) { return {state: state || "local", mount: "", reason: ""} }
function navigating(previous, path) {
    var mount = previous.mount
    if (mount && (path === mount || path.indexOf(mount + "/") === 0))
        return {state: "checking", mount: mount, reason: ""}
    return empty()
}
function decode(text, path) {
    try {
        var value = JSON.parse(text)
        var states = ["local", "unavailable", "checking", "idle", "pending", "uploading", "retrying", "error"]
        if (value.path !== path || states.indexOf(value.state) < 0 || typeof value.mount !== "string")
            return empty("unavailable")
        return value
    } catch (e) { return empty("unavailable") }
}
function accepts(generation, requestedGeneration, path, requestedPath) {
    return generation === requestedGeneration && path === requestedPath
}
function interval(value) {
    if (value.state === "local") return 30000
    if (value.state === "idle" || value.state === "unavailable") return 5000
    return 1000
}
function line(value) {
    if (value.state === "local") return ""
    var prefix = (value.mount ? Format.leafPart(value.mount) : "Cloud") + " (whole mount) · "
    if (value.state === "unavailable") return prefix + (value.reason || "Upload status unavailable")
    if (value.state === "checking") return prefix + "Checking upload queue…"
    if (value.state === "idle") return prefix + "No pending uploads"
    if (value.state === "error") return prefix + (value.reason || "Upload cache error")
        + (value.uploading > 0 ? " · " + value.uploading + " uploading" : "")
        + (value.queued > 0 ? " · " + value.queued + " queued" : "")
    if (value.state === "retrying") return prefix + "Retrying " + value.retrying + " upload(s)"
    if (value.state === "pending") return prefix + "Waiting to upload · " + value.queued + " queued"
    var progress = value.progressKnown
        ? " · " + Format.size(value.bytes) + " / " + Format.size(value.total)
          + (value.speed > 0 ? " · " + Format.size(Math.round(value.speed)) + "/s" : "") : ""
    return prefix + "Uploading " + value.uploading + " file(s)" + progress
        + (value.queued > 0 ? " · " + value.queued + " queued" : "")
}
