.pragma library

.import "Format.js" as Format

// The updater's row state and every word it shows, pure so tests/js/update.js drives it without a window; ui/UpdateCheck.qml runs the processes.

// The background poll's period, which is also how long a finished check stands before Settings > About asks again.
var INTERVAL_MS = 6 * 60 * 60 * 1000
// The first background look, a minute after the window opens: off the launch path, and early enough for the menu row.
var FIRST_CHECK_MS = 60 * 1000

// Every word the About row, its note line and the footer use, in one table so a restyle touches nothing else.
var WORDS = {
    check: "Check",
    checking: "Checking",
    available: "%1 available",
    current: "Up to date · %1",
    failed: "Could not check",
    launched: "Updating in terminal",
    source: "Built from source",
    noteLaunched: "Restart Flea when the update finishes",
    noteFailed: "Offline: Omarchy update still opens",
    noteSource: "Built locally: rebuild from the checkout",
    opening: "Opening Omarchy update · restart Flea when it finishes",
    launchFailed: "Omarchy's updater could not be started."
}

// The four words `flea --update check` prints first, see src/update.rs; anything else is a check that failed.
var ANSWERS = ["available", "current", "failed", "unchecked"]

function idle() {
    return { state: "idle", kind: "", installed: "", latest: "", checkedAt: 0 }
}

function copy(status, changes) {
    return Object.assign({}, status, changes)
}

function checking(status) {
    return copy(status, { state: "checking" })
}

// Sample input: "available opr 0.3.2-1 0.3.3-1\n", the one line src/update.rs prints.
function answered(status, text, now) {
    // A launch is final until Flea restarts: a check that was already running when it began changes nothing.
    if (status.state === "launched")
        return status
    var words = String(text || "").trim().split(" ")
    var known = words.length === 4 && ANSWERS.indexOf(words[0]) >= 0
    return copy(status, { state: known ? words[0] : "failed", kind: known ? words[1] : "",
                          installed: known && words[2] !== "-" ? words[2] : "",
                          latest: known && words[3] !== "-" ? words[3] : "", checkedAt: now })
}

// A launch hands the machine to Omarchy until Flea restarts; one that failed leaves the row as it was.
function launchedFrom(status, started) {
    return started ? copy(status, { state: "launched" }) : status
}

// What the About row's Enter does: check, launch Omarchy's updater, or nothing while a check or an update runs.
function onActivate(status) {
    switch (status.state) {
    case "idle": case "current": return "check"
    case "available": case "failed": return "launch"
    }
    return ""
}

// The chevron: drawn exactly when Enter does something.
function acts(status) {
    return onActivate(status) !== ""
}

// A local build has nothing to check or launch, so its row is a fact.
function isFact(status) {
    return status.state === "unchecked"
}

// About opening asks again unless the switch is off, a check or an update is running, or an answer from the last period stands.
function due(status, now, automatic) {
    if (!automatic || status.state === "checking" || status.state === "launched")
        return false
    var settled = status.state === "available" || status.state === "current" || status.state === "unchecked"
    return !settled || now - status.checkedAt >= INTERVAL_MS
}

// pkgver alone, unless that is what is installed and only the package release moved.
function shownVersion(latest, installed) {
    var version = latest.split("-")[0]
    return version === installed.split("-")[0] ? latest : version
}

function value(status) {
    switch (status.state) {
    case "checking": return WORDS.checking
    case "available": return Format.fill(WORDS.available, shownVersion(status.latest, status.installed))
    case "current": return Format.fill(WORDS.current, status.installed.split("-")[0])
    case "failed": return WORDS.failed
    case "launched": return WORDS.launched
    case "unchecked": return WORDS.source
    }
    return WORDS.check
}

// The value's colour role in ui/SettingsRow.qml: accent for an update, muted while checking, foreground otherwise.
function role(status) {
    if (status.state === "available")
        return "accent"
    return status.state === "checking" ? "" : "live"
}

// The line under the Updates group, as [words, role], or null in the states that need no explanation.
function note(status) {
    switch (status.state) {
    case "launched": return [WORDS.noteLaunched, "accent"]
    case "failed": return [WORDS.noteFailed, "foreground"]
    case "unchecked": return [WORDS.noteSource, "foreground"]
    }
    return null
}

// The background menu's row exists only while a newer build is known, and its hint is that version.
function menuVersion(status) {
    return status.state === "available" ? shownVersion(status.latest, status.installed) : ""
}

// What the footer says after a launch, as [sentence, isError].
function launchSentence(started) {
    return started ? [WORDS.opening, false] : [WORDS.launchFailed, true]
}
