.pragma library

// The words the chooser sends to assistive tools. QML owns when to speak them; this file keeps
// labels, urgency and progress throttling testable without a window or an accessibility bus.

function renameLabel(name) {
    return name.length > 0 ? "Rename " + name : "Rename item"
}

function notice(text, isError) {
    var line = text || ""
    return { text: line, assertive: line.length > 0 && isError === true }
}

function emptyProgress() {
    return { key: "", step: 0, text: "" }
}

// Announce the start and quarter boundaries. Transfer samples arrive every 150 ms, so reading each
// one would queue stale speech. The terminal message comes through notice(), not a 100% update.
function progress(previous, key, line, fraction) {
    if (!key || !line)
        return emptyProgress()
    var before = previous || emptyProgress()
    var value = Number(fraction)
    if (!isFinite(value))
        value = 0
    value = Math.max(0, Math.min(1, value))
    var step = Math.min(3, Math.floor(value * 4))
    if (before.key !== key)
        return { key: key, step: step, text: line }
    if (step <= before.step)
        return { key: key, step: before.step, text: "" }
    return { key: key, step: step, text: line + ", " + (step * 25) + "% complete" }
}
