.pragma library

function location(original, home) {
    var cut = String(original).lastIndexOf("/")
    if (cut < 0) return "Unknown"
    var folder = cut === 0 ? "/" : original.slice(0, cut)
    if (home && (folder === home || folder.indexOf(home + "/") === 0))
        return "~" + folder.slice(home.length)
    return folder
}

// Sample input, GIO trash::deletion-date: "2026-09-08T10:00:00" in local time.
function deleted(text, nowMs) {
    // The same guard expired() carries below, and for the same reason: new Date(null) is the epoch,
    // so an item with no date would have been labelled with a day in 1970 instead of Unknown.
    if (typeof text !== "string" || text.length === 0) return "Unknown"
    var date = new Date(text)
    if (!isFinite(date.getTime())) return "Unknown"
    var now = new Date(nowMs)
    var day = new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime()
    var today = new Date(now.getFullYear(), now.getMonth(), now.getDate())
    if (day === today.getTime()) return "today"
    today.setDate(today.getDate() - 1)
    if (day === today.getTime()) return "yesterday"
    var months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
    return months[date.getMonth()] + " " + date.getDate() + (date.getFullYear() === now.getFullYear() ? "" : " " + date.getFullYear())
}

// The 30 day sweep's own two questions. GM's ruling of 2026-09-11: the sweep is off by default and
// opt in, because permanent deletion is outside the undo journal and nobody opted into it by
// installing an update.
var DAY_MS = 24 * 60 * 60 * 1000

// Sample input is deleted()'s own: gio's trash::deletion-date, "2026-09-08T10:00:00" in local time.
// An item whose date cannot be read is NEVER swept: this is the one direction a wrong answer here
// can be taken in, and "leave it alone" is that direction.
function expired(text, nowMs, days) {
    // new Date(null) is the EPOCH, not an invalid date, so a missing value reads as 1970 and would
    // be swept by every run. The type is checked before the parse rather than after it.
    if (typeof text !== "string" || text.length === 0) return false
    var date = new Date(text)
    if (!isFinite(date.getTime())) return false
    return nowMs - date.getTime() >= days * DAY_MS
}

// Which day it is, in whole days since the epoch at local midnight, which is what the sweep records
// so that it runs once a day rather than once per launch.
function dayNumber(nowMs) {
    var now = new Date(nowMs)
    return Math.floor(new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime() / DAY_MS)
}
