.import "../../ui/js/Settings.js" as Settings
.import "../../ui/js/Update.js" as Update
.import "../../ui/js/MakeDefault.js" as MakeDefault
.import "../../ui/js/SettingsAbout.js" as SettingsAbout
.import "sourcefixture.js" as Source

// The About section's Updates group: the Update Flea row, the switch that governs the automatic checks, and the note line; then This computer's Make Flea the default row.

function find(rows, id) {
    return rows.filter(function (row) { return row.id === id })[0] || {}
}

function about(update, data) {
    return Settings.rows("about", { about: { update: update }, data: data || {} })
}

function answered(line) {
    return Update.answered(Update.checking(Update.idle()), line, 1000)
}

// The rows between the Updates heading and the next one, as kind:label, for a status.
function group(update) {
    var rows = about(update)
    var labels = rows.map(function (row) { return row.kind + ":" + row.label })
    var from = labels.indexOf("group:Updates") + 1
    return labels.slice(from, labels.indexOf("group:This computer")).join(",")
}

function run(check) {
    var rows = about(undefined)
    check("Updates holds Update Flea then Check automatically, and nothing else while idle",
          group(undefined), "action:Update Flea,check:Check automatically")
    check("the static Update owner fact is gone", rows.filter(function (row) { return row.label === "Update owner" }).length, 0)
    var row = find(rows, "updateFlea")
    check("the row wears the download mark, the one the old fact wore", row.glyph, "download")
    check("idle offers the check in the foreground, with a chevron", row.value + "|" + row.role + "|" + row.inert, "Check|live|false")
    check("and is a stop the cursor rests on", Settings.focusable(row), true)
    var available = find(about(answered("available opr 0.3.3-1 0.3.4-1")), "updateFlea")
    check("an update reads in the accent", available.value + "|" + available.role, "0.3.4 available|accent")
    var checking = find(about(Update.checking(Update.idle())), "updateFlea")
    check("a running check is muted, inert, and still a stop so the cursor does not jump",
          checking.value + "|" + checking.role + "|" + checking.inert + "|" + Settings.focusable(checking), "Checking||true|true")
    var local = find(about(answered("unchecked git - -")), "updateFlea")
    check("an unchecked build is a local fact the cursor steps over", local.kind + "|" + local.value + "|" + Settings.focusable(local),
          "fact|Built from source|false")

    var launched = Update.launchedFrom(answered("available opr 0.3.3-1 0.3.4-1"), true)
    check("after a launch the group ends on the restart note",
          group(launched), "action:Update Flea,check:Check automatically,hint:Restart Flea when the update finishes")
    var note = about(launched).filter(function (row) { return row.kind === "hint" && row.label.indexOf("Restart") === 0 })[0] || {}
    check("which is in the accent and takes no cursor", note.role + "|" + Settings.focusable(note), "accent|false")
    check("offline explains itself in the foreground",
          about(answered("failed opr 0.3.3-1 -")).filter(function (row) { return row.kind === "hint" })
              .map(function (row) { return row.label + "|" + row.role }).join(""), "Offline: Omarchy update still opens|foreground")

    var auto = find(rows, "updates.autoCheck")
    check("the switch carries no mark and names both triggers in its caption",
          String(auto.glyph) + "|" + auto.caption, "undefined|when About opens, every 6 h")
    check("it ships on", auto.on, true)
    check("and reads a stored false as off", find(about(undefined, { updates: { autoCheck: false } }), "updates.autoCheck").on, false)
    check("it is a check the cursor stops on, which ui/SettingsPanel.qml writes by its own id",
          Settings.focusable(auto) + "|" + auto.kind, "true|check")

    check("the two web rows carry their own destinations",
          find(rows, "reportIssue").url + "|" + find(rows, "support").url,
          "https://github.com/thisisgm/flea/issues|https://github.com/sponsors/thisisgm")
    check("and no other row does", rows.filter(function (r) { return r.url !== undefined }).length, 2)
    runDefault(check)
    runInstalled(check)
}

var FLEA = "com.thisisgm.flea.desktop"
var NAUTILUS = "org.gnome.Nautilus.desktop"
var CLAIM = ["--default"]
var RELEASE = ["--default", "off"]
// The number the run's own re-read carries, one past the read About opened with.
var REREAD = 2

// The one "flea: " sentence a Rust function prints on stderr, its {} filled, with the newline eprintln! adds; "" unless there is exactly one.
// Sample input: eprintln!("flea: no portal backend is installed, so the file chooser step was skipped");
function spoken(text, header, fill) {
    var start = text.indexOf(header)
    var body = start < 0 ? "" : text.substring(start, text.indexOf("\n}\n", start))
    var found = body.match(/eprintln!\(\s*"flea: [^"]*"/g) || []
    return found.length === 1 ? found[0].substring(found[0].indexOf("\"") + 1, found[0].length - 1).replace("{}", fill) + "\n" : ""
}

function box(handler, claim) {
    return Settings.rows("about", { about: { handler: handler, claim: claim } })
}

// The rows from the File manager fact to Keyboard sheet, as kind:label, the note included when there is one.
function boxGroup(handler, claim) {
    var labels = box(handler, claim).map(function (row) { return row.kind + ":" + row.label })
    var from = labels.indexOf("fact:File manager")
    return labels.slice(from, labels.indexOf("action:Keyboard sheet")).join(",")
}

// The line under the row, or {} when the next row is not a note.
function noteOf(handler, claim) {
    var rows = box(handler, claim)
    var next = rows[rows.map(function (row) { return row.id }).indexOf("makeDefault") + 1]
    return next.kind === "hint" ? next : {}
}

// The row and its note as on|inert|note|role|elide, the three note fields empty when there is none.
function look(handler, claim) {
    var row = find(box(handler, claim), "makeDefault"), note = noteOf(handler, claim)
    return [row.on, row.inert, note.label || "", note.role || "", note.elide || ""].join("|")
}

// A run that has exited, whose handler has been read again, and whose portal restart, if it asked for one, answered.
function after(args, code, stderr, restartOk) {
    var read = MakeDefault.settled(MakeDefault.finished(MakeDefault.started(MakeDefault.idle(), args), code, stderr, REREAD), REREAD)
    return read.restarting ? MakeDefault.restarted(read, restartOk !== false) : read
}

function runDefault(check) {
    // The two sentences MakeDefault.js reads are the binary's own, taken from the source rather than copied here.
    var defaults = Source.source("src/defaults.rs"), claims = Source.source("src/claims.rs")
    // Sample input: pub const DESKTOP_ID: &str = "com.thisisgm.flea.desktop";
    var rustId = (defaults.match(/pub const DESKTOP_ID: &str = "([^"]*)"/) || [])[1]
    var refusal = spoken(defaults, "fn claim_from(entry: Option<PathBuf>) -> i32 {", rustId)
    var skipped = spoken(claims, "fn claim_both() -> i32 {", "")
    check("the box is ticked by the very id src/defaults.rs claims", MakeDefault.DESKTOP_ID, rustId)
    var exitUninstalled = (defaults.match(/pub const EXIT_UNINSTALLED: i32 = (\d+)/) || [])[1]
    check("the refusal is defaults::claim()'s own flea: line", refusal.indexOf("flea: " + rustId + " ") === 0, true)
    check("and MakeDefault.js reads the status src/defaults.rs exits with, not the sentence", String(MakeDefault.UNINSTALLED_STATUS), exitUninstalled)
    check("the partly line is claim_both()'s own flea: line, word for word what MakeDefault.js reads",
          skipped, "flea: " + MakeDefault.SKIPPED + "\n")

    check("the row sits directly under the File manager fact, which stays a fact",
          boxGroup(NAUTILUS, undefined), "fact:File manager,check:Make Flea the default")
    var row = find(box(NAUTILUS, undefined), "makeDefault")
    check("it is the label and the box alone, with no caption and no mark", String(row.caption) + "|" + String(row.glyph), "undefined|undefined")
    check("a check the cursor stops on", Settings.focusable(row) + "|" + row.kind, "true|check")
    check("off: unticked, live, and no note", look(NAUTILUS, undefined), "false|false|||")
    check("off: a press claims", JSON.stringify(MakeDefault.press(NAUTILUS, MakeDefault.idle())), JSON.stringify(CLAIM))
    check("nothing answered reads as off, not as Flea", look("", undefined), "false|false|||")

    check("on: ticked by xdg-mime's answer, with the note in the foreground on one eliding line", look(FLEA, MakeDefault.idle()),
          "true|false|Folders, Show in folder and file dialogs open Flea.|foreground|right")
    check("on: a press hands folders back", JSON.stringify(MakeDefault.press(FLEA, MakeDefault.idle())), JSON.stringify(RELEASE))
    check("the note is a line the cursor steps over", noteOf(FLEA, undefined).kind + "|" + Settings.focusable(noteOf(FLEA, undefined)), "hint|false")

    var claiming = MakeDefault.started(MakeDefault.idle(), CLAIM)
    check("working on a claim: inert, the box still the truth, the note muted",
          look(NAUTILUS, claiming), "false|true|Making Flea the default|muted|right")
    check("working on a release says so", look(FLEA, MakeDefault.started(MakeDefault.idle(), RELEASE)), "true|true|Handing folders back|muted|right")
    check("a press while a run is in flight runs nothing", MakeDefault.press(NAUTILUS, claiming), null)
    check("and the row stays a stop so the cursor does not jump", Settings.focusable(find(box(NAUTILUS, claiming), "makeDefault")), true)
    check("an exited run is still working until the handler is read again",
          MakeDefault.state(NAUTILUS, MakeDefault.finished(claiming, 0, "", REREAD)), "working")
    check("the re-read answer decides the box, not the press", look(NAUTILUS, after(CLAIM, 0, "")), "false|false|||")
    check("a read landing while flea still runs, About opening in another window, ends nothing",
          MakeDefault.state(NAUTILUS, MakeDefault.settled(claiming, 1)) + "|" + MakeDefault.press(NAUTILUS, MakeDefault.settled(claiming, 1)), "working|null")
    var waiting = MakeDefault.finished(claiming, 0, "", REREAD)
    check("nor does one begun before flea exited, which may predate what it wrote",
          MakeDefault.state(FLEA, MakeDefault.restarted(MakeDefault.settled(waiting, REREAD - 1), true)), "working")
    check("the read begun after the exit is the one that does",
          MakeDefault.state(FLEA, MakeDefault.restarted(MakeDefault.settled(waiting, REREAD), true)), "on")
    check("an exit and its text join in either order, an empty text counting as said",
          [MakeDefault.whole(MakeDefault.landed({}, { code: 0 })), MakeDefault.whole(MakeDefault.landed({}, { text: "" })),
           JSON.stringify(MakeDefault.landed(MakeDefault.landed({}, { code: 1 }), { text: "" })),
           JSON.stringify(MakeDefault.landed(MakeDefault.landed({}, { text: "" }), { code: 1 })),
           MakeDefault.whole(MakeDefault.landed(MakeDefault.landed({}, { text: "" }), { code: 1 }))].join("|"),
          'false|false|{"code":1,"text":""}|{"text":"","code":1}|true')

    check("the portal restart is exactly systemctl's try-restart of the user unit", MakeDefault.RESTART.join(" "),
          "systemctl --user try-restart xdg-desktop-portal.service")
    check("a claim, a partly claim and a release that went through each ask for it",
          [MakeDefault.finished(claiming, 0, "", REREAD).restarting,
           MakeDefault.finished(claiming, 0, skipped, REREAD).restarting,
           MakeDefault.finished(MakeDefault.started(MakeDefault.idle(), RELEASE), 0, "", REREAD).restarting].join("|"), "true|true|true")
    check("a failed run and a refused one do not",
          MakeDefault.finished(claiming, 1, "flea: no\n", REREAD).restarting + "|" + MakeDefault.finished(claiming, MakeDefault.UNINSTALLED_STATUS, refusal, REREAD).restarting, "false|false")
    var exited = MakeDefault.finished(claiming, 0, "", REREAD)
    check("still working after the re-read while the restart runs", MakeDefault.state(FLEA, MakeDefault.settled(exited, REREAD)), "working")
    check("and after the restart while the re-read runs", MakeDefault.state(FLEA, MakeDefault.restarted(exited, true)), "working")
    check("either order ends the run",
          MakeDefault.settled(MakeDefault.restarted(exited, true), REREAD).running + "|" + MakeDefault.restarted(MakeDefault.settled(exited, REREAD), true).running,
          "false|false")

    // ui/ViewState.qml's writer rule: a program that cannot start raises no exited, only running going false.
    check("running going false with no status is a program that never ran, never one whose exit landed or one starting",
          [MakeDefault.neverRan({}, false), MakeDefault.neverRan({ text: "" }, false), MakeDefault.neverRan({ code: 0 }, false),
           MakeDefault.neverRan({}, true)].join("|"), "true|true|false|false")
    var unread = MakeDefault.landed({}, MakeDefault.NEVER_RAN)
    var manager = box(MakeDefault.handlerOf(unread), undefined).filter(function (row) { return row.label === "File manager" })[0] || {}
    check("an xdg-mime that never ran is a whole answer at once, stating no handler, so File manager reads Not reported",
          MakeDefault.whole(unread) + "|" + manager.value + "|" + MakeDefault.handlerOf({ code: 0, text: " " + FLEA + "\n" })
          + "|" + MakeDefault.handlerOf({ code: 3, text: FLEA + "\n" }), "true|Not reported|" + FLEA + "|")
    check("and as a run's own re-read it ends the run unticked, not working",
          look(MakeDefault.handlerOf(unread), MakeDefault.restarted(MakeDefault.settled(exited, REREAD), true)), "false|false|||")
    var dead = MakeDefault.unstarted(claiming, ["/usr/bin/flea", "--default"])
    check("a flea that never ran fails at once, naming the program, with no re-read or restart owed",
          look(NAUTILUS, dead) + "|" + dead.reading + "|" + dead.restarting,
          "false|false|flea could not start|error|right|false|false")
    check("and the next press tries again", JSON.stringify(MakeDefault.press(NAUTILUS, dead)), JSON.stringify(CLAIM))
    var restarting = MakeDefault.settled(exited, REREAD)
    check("a systemctl that never ran gives the restart-failed note and ends the run",
          restarting.restarting + "|" + look(FLEA, MakeDefault.restartStopped(restarting, false)),
          "true|true|false|File dialogs follow after xdg-desktop-portal restarts.|foreground|right")
    check("and one that exited is not taken for one that never ran when its running goes false",
          look(FLEA, MakeDefault.restartStopped(MakeDefault.restarted(restarting, true), false)), look(FLEA, MakeDefault.restarted(restarting, true)))
    check("while it still runs nothing ends", JSON.stringify(MakeDefault.restartStopped(restarting, true)), JSON.stringify(restarting))
    var late = after(CLAIM, 0, "", false)
    check("a restart that failed keeps the claim and says when file dialogs follow", look(FLEA, late),
          "true|false|File dialogs follow after xdg-desktop-portal restarts.|foreground|right")
    check("a release keeps its unticked box with the same line", look(NAUTILUS, after(RELEASE, 0, "", false)),
          "false|false|File dialogs follow after xdg-desktop-portal restarts.|foreground|right")
    check("the next run drops it", MakeDefault.started(late, RELEASE).outcome, "")

    check("partly: folders claimed, the chooser step skipped", look(FLEA, after(CLAIM, 0, skipped)),
          "true|false|File dialogs need the flea package's portal files.|foreground|right")
    check("partly only while Flea is still the answer", look(NAUTILUS, after(CLAIM, 0, skipped)), "false|false|||")
    check("partly keeps its own note when the restart fails, since its file dialogs never follow",
          look(FLEA, after(CLAIM, 0, skipped, false)), "true|false|File dialogs need the flea package's portal files.|foreground|right")

    var failed = after(CLAIM, 1, "xdg-mime: warning from xdg-mime itself\nflea: xdg-mime default exited 0 but inode/directory still resolves to org.gnome.Nautilus.desktop\n")
    check("failed: flea's own first line without its prefix, in the error role, the box the re-read truth",
          look(NAUTILUS, failed), "false|false|xdg-mime default exited 0 but inode/directory still resolves to org.gnome.Nautilus.desktop|error|right")
    check("failed: the next press tries again", JSON.stringify(MakeDefault.press(NAUTILUS, failed)), JSON.stringify(CLAIM))
    check("a failed release keeps the box ticked when Flea is still the answer", look(FLEA, after(RELEASE, 1, "flea: a half failed\n")),
          "true|false|a half failed|error|right")
    check("a line flea did not prefix is still better than nothing, the first that says anything",
          MakeDefault.errorLine("\n  \nsomething broke\nand then this\n", 1, true), "something broke")
    check("silence names the command and its status, a release", MakeDefault.errorLine("", 2, false), "flea --default off exited 2")
    check("and a claim, blank lines being silence too", MakeDefault.errorLine("\n \n", 1, true), "flea --default exited 1")
    check("a new run drops the last one's error", MakeDefault.started(failed, CLAIM).error + "|" + MakeDefault.started(failed, CLAIM).outcome, "|")

    var refused = MakeDefault.finished(claiming, MakeDefault.UNINSTALLED_STATUS, refusal, REREAD)
    var unpackaged = "false|true|Install a Flea package to make it the default.|foreground|right"
    check("unpackaged from the refusal, before the re-read lands", look(NAUTILUS, refused), unpackaged)
    check("and after it", look(NAUTILUS, MakeDefault.settled(refused, REREAD)), unpackaged)
    check("an unpackaged press runs nothing", MakeDefault.press(NAUTILUS, MakeDefault.settled(refused, REREAD)), null)
    // The status alone says unpackaged: the refusal's words with any other status are a plain failure, and the status with other words is unpackaged.
    var wordsOnly = MakeDefault.finished(claiming, 1, refusal, REREAD)
    check("the refusal's sentence under another status is a failed run, not an unpackaged build", wordsOnly.packaged + "|" + wordsOnly.outcome, "true|failed")
    var statusOnly = MakeDefault.finished(claiming, MakeDefault.UNINSTALLED_STATUS, "flea: some other words\n", REREAD)
    check("and the uninstalled status under other words is an unpackaged build", statusOnly.packaged + "|" + statusOnly.outcome, "false|")
    var probedOut = MakeDefault.probed(MakeDefault.idle(), false)
    check("unpackaged from the probe, before any press", look(NAUTILUS, probedOut), unpackaged)
    check("and an entry found later makes the row live again", look(NAUTILUS, MakeDefault.probed(probedOut, true)), "false|false|||")

    check("the probe walks src/userfile.rs data_file()'s ladder",
          MakeDefault.entryPaths({ HOME: "/home/gm", XDG_DATA_HOME: "", XDG_DATA_DIRS: "" }).join(","),
          "/home/gm/.local/share/applications/" + FLEA + ",/usr/local/share/applications/" + FLEA + ",/usr/share/applications/" + FLEA)
    check("a set data home and data dirs replace the defaults, and empty segments are skipped",
          MakeDefault.entryPaths({ HOME: "/home/gm", XDG_DATA_HOME: "/d/home", XDG_DATA_DIRS: "/d/a::/d/b" }).join(","),
          "/d/home/applications/" + FLEA + ",/d/a/applications/" + FLEA + ",/d/b/applications/" + FLEA)
    check("with no HOME there is no data home to look in", MakeDefault.entryPaths({ XDG_DATA_DIRS: "/d/a" }).join(","), "/d/a/applications/" + FLEA)
    check("each path is its own argument, never part of the script, which tests each one quoted and stops at the first",
          JSON.stringify(MakeDefault.probeCommand(["/a b/x.desktop", "$(y)"])),
          JSON.stringify(["sh", "-c", "for f; do [ -f \"$f\" ] && exit 0; done; exit 1", "sh", "/a b/x.desktop", "$(y)"]))
}

// The local package fields About reads from pacman -Qi under LC_ALL=C.
var LOCAL_QI = "Name            : flea\nVersion         : 0.3.4-1\nPackager        : Unknown Packager\n"
    + "Build Date      : Wed Sep 23 12:40:50 2026\nInstall Script  : No\nValidated By    : None\n"

// Installed from is the kind src/update.rs derives from the same two pacman answers, so it and the Update Flea row agree.
function runInstalled(check) {
    var update = Source.source("src/update.rs")
    // Sample input: const OPR_PACKAGE: &str = "flea";
    function named(constant) { return (update.match(new RegExp("const " + constant + ": &str = \"([^\"]*)\";")) || [])[1] }
    check("the OPR package name is src/update.rs's own", SettingsAbout.PACKAGES.opr, named("OPR_PACKAGE"))
    check("the release package name is src/update.rs's own", SettingsAbout.PACKAGES.release, named("RELEASE_PACKAGE"))
    // Sample input: Kind::Opr => "opr",
    var words = (update.match(/Kind::[A-Za-z]+ => "[a-z]+"/g) || []).map(function (arm) { return arm.split("\"")[1] })
    check("one sentence for each kind word flea --update check prints, and no other",
          Object.keys(SettingsAbout.SOURCES).sort().join("|"), words.sort().join("|"))

    var bin = SettingsAbout.packageFacts(LOCAL_QI)
    check("a local package -Qi reads as its package, its build date and no signature",
          [bin.package, bin.built, bin.signed].join("|"), "flea 0.3.4-1|Wed Sep 23 12:40:50 2026|false")
    check("the release binary package reads as a local package", SettingsAbout.installedFrom("flea-bin", bin.signed), "Local package")
    check("the kind the Update Flea row reads from the release check line is the same one",
          answered("unchecked release 0.3.4-1 -").kind + "|" + SettingsAbout.installKind("flea-bin", bin.signed), "release|release")
    check("a release owner stays release whether signed or unsigned",
          SettingsAbout.installKind("flea-bin", true) + "|" + SettingsAbout.installKind("flea-bin", false), "release|release")
    check("every other owner is the local kind, never release",
          [SettingsAbout.installKind("flea-git", false), SettingsAbout.installKind("flea", false), SettingsAbout.installKind("flea-custom", true)].join("|"), "local|local|local")
    check("OPR's signed flea keeps the board's words", SettingsAbout.installedFrom("flea", true), "Omarchy Package Repository")
    check("the same name unsigned is a local makepkg build", SettingsAbout.installedFrom("flea", false), "Local package")
    check("a former git package reads as local", SettingsAbout.installedFrom("flea-git", false), "Local package")
    check("a signed release binary package reads as a local package too", SettingsAbout.installedFrom("flea-bin", true), "Local package")
    check("a signed former git package is local too", SettingsAbout.installedFrom("flea-git", true), "Local package")
    check("any other owner is somebody's own package, signed or not", SettingsAbout.installedFrom("flea-custom", true), "Local package")
    check("no owner is a cargo build", SettingsAbout.installedFrom("", false), "Not from a package")
    check("Validated By is read as src/update.rs reads it",
          [SettingsAbout.packageFacts("Validated By    : SHA-256 Sum  Signature\n").signed,
           SettingsAbout.packageFacts("Validated By    : SHA-256 Sum\n").signed,
           SettingsAbout.packageFacts("Description     : Signature\nValidated By    : None\n").signed].join("|"), "true|false|false")
    check("a query that failed is an empty answer", JSON.stringify(SettingsAbout.packageFacts("")), '{"package":"","built":"","signed":false}')
    check("and About asks pacman -Si nothing, because it describes the installed owner", Source.source("ui/AboutFacts.qml").indexOf("\"-Si\""), -1)
}
