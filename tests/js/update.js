.import "../../ui/js/Update.js" as Update

// The updater's row state: what each line `flea --update check` prints turns into, what Enter does next, and when an automatic trigger asks.

var HOUR = 60 * 60 * 1000

function after(line) {
    return Update.answered(Update.checking(Update.idle()), line, 1000)
}

var LINES = {
    available: "available opr 0.3.3-1 0.3.4-1\n",
    current: "current opr 0.3.3-1 -",
    offline: "failed opr 0.3.3-1 -",
    other: "unchecked git - -",
    source: "unchecked local 0.3.3-1 -"
}

function run(check) {
    runReading(check)
    runActing(check)
    runAutomatic(check)
    runMenu(check)
}

// The board's eight states, each as value, value role and whether a chevron is drawn.
function runReading(check) {
    function drawn(status) { return Update.value(status) + "|" + Update.role(status) + "|" + Update.acts(status) }
    check("idle offers the check", drawn(Update.idle()), "Check|live|true")
    check("checking is muted with no chevron", drawn(Update.checking(Update.idle())), "Checking||false")
    check("current names the installed release", drawn(after(LINES.current)), "Up to date · 0.3.3|live|true")
    check("available takes the accent", drawn(after(LINES.available)), "0.3.4 available|accent|true")
    check("launched has no chevron", drawn(Update.launchedFrom(after(LINES.available), true)), "Updating in terminal|live|false")
    check("offline still acts", drawn(after(LINES.offline)), "Could not check|live|true")
    check("any unchecked kind reads as built from source", drawn(after(LINES.other)), "Built from source|live|false")
    check("a local build is built from source", drawn(after(LINES.source)), "Built from source|live|false")
    check("and so is a binary no package owns", Update.value(after("unchecked unowned - -")), "Built from source")
    check("only the unchecked kinds are facts, a binary no package owns among them",
          [Update.idle(), after(LINES.current), after(LINES.other), after(LINES.source), after("unchecked unowned - -")]
              .map(function (status) { return Update.isFact(status) + "/" + Update.acts(status) }).join(","),
          "false/true,false/true,true/false,true/false,true/false")
    check("a rebuild of the installed release names the package release, or it would read as the same version",
          Update.value(after("available opr 0.3.3-1 0.3.3-2")), "0.3.3-2 available")
    check("a line this build does not know is a failed check", Update.value(after("flea: unknown flag --update")), "Could not check")
    check("so is no line at all, which a binary that died prints", Update.value(after("")), "Could not check")
    check("a dash is an empty field, never a version", after(LINES.current).latest, "")
    check("the answer records when it came", after(LINES.current).checkedAt, 1000)
}

function runActing(check) {
    function next(status) { return Update.onActivate(status) || "nothing" }
    check("idle and current check",
          next(Update.idle()) + "|" + next(after(LINES.current)), "check|check")
    check("available and offline open Omarchy's updater",
          next(after(LINES.available)) + "|" + next(after(LINES.offline)), "launch|launch")
    check("Enter does nothing while a check or an update runs, or on a fact",
          [Update.checking(Update.idle()), Update.launchedFrom(after(LINES.available), true), after(LINES.other), after(LINES.source)]
              .map(next).join(","), "nothing,nothing,nothing,nothing")
    check("a launch that failed leaves the row as it was",
          Update.value(Update.launchedFrom(after(LINES.available), false)), "0.3.4 available")
    check("a launch from a failed check is final too, and the row then reads as launched",
          Update.value(Update.launchedFrom(after(LINES.offline), true)) + "|" + Update.due(Update.launchedFrom(after(LINES.offline), true), 7 * HOUR, true),
          "Updating in terminal|false")
    check("an answer landing after a launch leaves the launch standing",
          Update.value(Update.answered(Update.launchedFrom(after(LINES.available), true), LINES.current, 2000)), "Updating in terminal")
    check("the footer says what a launch did", Update.launchSentence(true).join("|"), "Opening Omarchy update · restart Flea when it finishes|false")
    check("and says a refused one as an error", Update.launchSentence(false).join("|"), "Omarchy's updater could not be started.|true")
    function noted(status) { var n = Update.note(status); return n === null ? "none" : n.join("|") }
    check("the note asks for a restart in the accent after a launch",
          noted(Update.launchedFrom(after(LINES.available), true)), "Restart Flea when the update finishes|accent")
    check("and explains offline and all unchecked builds in the foreground",
          [LINES.offline, LINES.other, LINES.source].map(function (line) { return noted(after(line)) }).join(", "),
          "Offline: Omarchy update still opens|foreground, Built locally: rebuild from the checkout|foreground, "
          + "Built locally: rebuild from the checkout|foreground")
    check("and is absent everywhere else",
          [Update.idle(), Update.checking(Update.idle()), after(LINES.current), after(LINES.available)].map(noted).join(","),
          "none,none,none,none")
}

function runAutomatic(check) {
    check("the poll runs every six hours", Update.INTERVAL_MS, 6 * HOUR)
    check("the first background look waits a minute, well inside the first poll", Update.FIRST_CHECK_MS === 60 * 1000 && Update.FIRST_CHECK_MS < Update.INTERVAL_MS, true)
    check("About asks when nothing has been asked yet", Update.due(Update.idle(), 5 * HOUR, true), true)
    check("and never with the switch off", Update.due(Update.idle(), 5 * HOUR, false), false)
    check("or while a check is already running", Update.due(Update.checking(Update.idle()), 5 * HOUR, true), false)
    check("or once an update has been launched", Update.due(Update.launchedFrom(after(LINES.available), true), 7 * HOUR, true), false)
    var answer = after(LINES.current)
    check("an answer inside the period stands", Update.due(answer, 1000 + 6 * HOUR - 1, true), false)
    check("and one a whole period old does not", Update.due(answer, 1000 + 6 * HOUR, true), true)
    check("an available or unchecked answer inside the period stands too, so About does not ask pacman again",
          [after(LINES.available), after(LINES.other)].map(function (status) { return Update.due(status, 1000 + 6 * HOUR - 1, true) }).join(","),
          "false,false")
    check("and either is asked again once a whole period old",
          [after(LINES.available), after(LINES.other)].map(function (status) { return Update.due(status, 1000 + 6 * HOUR, true) }).join(","),
          "true,true")
    check("a failed check never stands, so About asks again", Update.due(after(LINES.offline), 2000, true), true)
}

function runMenu(check) {
    check("the menu row exists only while a newer build is known",
          [Update.idle(), Update.checking(Update.idle()), after(LINES.current), after(LINES.offline), after(LINES.other),
           after(LINES.source), Update.launchedFrom(after(LINES.available), true)]
              .map(function (status) { return JSON.stringify(Update.menuVersion(status)) }).join(","), '"","","","","","",""')
    check("and its hint is that version", Update.menuVersion(after(LINES.available)), "0.3.4")
    check("or the package release, when only that moved", Update.menuVersion(after("available opr 0.3.3-1 0.3.3-2")), "0.3.3-2")
}
