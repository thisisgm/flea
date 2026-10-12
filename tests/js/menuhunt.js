.import "../../ui/js/Menu.js" as Menu
.import "../../ui/js/Ops.js" as Ops
.import "../../ui/js/Errors.js" as Errors
.import "../../ui/js/Messages.js" as Messages
.import "../../ui/js/Swap.js" as Swap
.import "sourcefixture.js" as Source

function executable(check, state, entry, actions) {
    var menuSource = Source.source("ui/js/Menu.js")
    var eligibility = Source.slice(menuSource, "function canMakeExecutable(", "function copyAsEntries(")
    var shebang = Source.slice(Source.source("ui/Pane.qml"), "function shebangTarget()", "function checkShebang()")
    var formatSource = Source.source("ui/js/Format.js")
    check("regular-file type has one Format constant", formatSource.indexOf("var S_IFREG = 0o100000") >= 0, true)
    var formatNames = ["S_IFMT", "S_IFREG", "ANY_EXECUTE_BIT"]
    formatNames.forEach(function (name) {
        check("Make executable uses Format." + name, eligibility.indexOf("Format." + name) >= 0, true)
        check("shebang target uses Format." + name, shebang.indexOf("Format." + name) >= 0, true)
    })
    // A logical row can land before its delegate; Paste as still needs the row's snapshot.
    var entrance = Source.slice(Source.source("ui/Pane.qml"), "function openPasteAs()", "function invertSelection")
    var openPasteAs = eval("(function(root, menu) {" + entrance + "\nopenPasteAs();})")
    var opened = "", p = {cursorRow: {n: "canary.txt"},
        openCursorMenu: function() { return false },
        listSlot: {width: 40, height: 40, mapToItem: function() { return Qt.point(20, 20) }}}
    var menu = {clipboardAvailable: true, openAt: function() { opened = "row" },
        openBackground: function() { opened = "background" }, openSubmenuFor: function() {}}
    openPasteAs(p, menu)
    check("hunt: Paste as snapshots a logical row before its delegate lands", opened, "row")
    p.cursorRow = null
    openPasteAs(p, menu)
    check("hunt: empty Paste as uses the background inventory", opened, "background")
    // Both approved boards require a cursor-row script without any execute bit.
    check("Make executable shows on a script missing its bit",
        entry(Menu.listingEntries(state({ hiddenActions: [], rowMode: 0o100644, hasShebang: true, cursorIsTarget: true })), "makeExecutable").action, "makeExecutable")
    check("and wears the play mark no neighbour wears",
        entry(Menu.listingEntries(state({ hiddenActions: [], rowMode: 0o100644, hasShebang: true, cursorIsTarget: true })), "makeExecutable").glyph, "play")
    check("without a shebang it is absent",
        entry(Menu.listingEntries(state({ hiddenActions: [], rowMode: 0o100644, cursorIsTarget: true })), "makeExecutable").action, undefined)
    check("with the execute bit already set it is absent too",
        entry(Menu.listingEntries(state({ hiddenActions: [], rowMode: 0o100744, hasShebang: true, cursorIsTarget: true })), "makeExecutable").action, undefined)
    // Both approved boards require no execute bit, rather than just no owner execute bit.
    check("hunt: Make executable is absent with group execute already set",
        entry(Menu.listingEntries(state({ hiddenActions: [], rowMode: 0o100654, hasShebang: true, cursorIsTarget: true })), "makeExecutable").action, undefined)
    check("hunt: Make executable is absent with everyone execute already set",
        entry(Menu.listingEntries(state({ hiddenActions: [], rowMode: 0o100645, hasShebang: true, cursorIsTarget: true })), "makeExecutable").action, undefined)
    check("on a directory it is absent",
        entry(Menu.listingEntries(state({ hiddenActions: [], rowMode: 0o040755, hasShebang: true, cursorIsTarget: true })), "makeExecutable").action, undefined)
    check("on a multi-selection it is absent",
        entry(Menu.listingEntries(state({ hiddenActions: [], rowMode: 0o100644, selectionCount: 2, hasShebang: true, cursorIsTarget: true })), "makeExecutable").action, undefined)
    check("on a single selection that is not the cursor row it is absent",
        entry(Menu.listingEntries(state({ hiddenActions: [], rowMode: 0o100644, hasShebang: true, cursorIsTarget: false })), "makeExecutable").action, undefined)
    check("and the Menus switch takes it away like any other row",
        entry(Menu.listingEntries(state({ hiddenActions: ["makeExecutable"], rowMode: 0o100644, hasShebang: true, cursorIsTarget: true })), "makeExecutable").action, undefined)
    check("it sits where hidden Permissions would sit before it",
        actions(Menu.listingEntries(state({ hiddenActions: [], rowMode: 0o100644, hasShebang: true, cursorIsTarget: true, selectionModes: [0o100644] }))).indexOf("permissions,makeExecutable") >= 0, true)
}

function clipboard(check, pathsPane) {
    var raceSent = [], race = pathsPane(raceSent), raceAt = 0
    race.selectedIndices = function () { return [raceAt] }
    Ops.clip(race, true)
    Ops.clip(race, false, ["/d/B"])
    raceAt = 2
    Ops.clip(race, false)
    Ops.pathsResolved(race, ["/d/A"])
    check("r1: Cut A reply preserves resolved Copy B while Copy C waits", JSON.stringify(race.clipboard),
          JSON.stringify({paths: ["/d/B"], moving: false, token: ""}))
    Ops.paste(race)
    check("r1: Paste copies B and never moves A", JSON.stringify(race.asked[0]),
          JSON.stringify({c: "transfer", op: "copy", paths: ["/d/B"], dest: "/d"}))
    check("r1: queued Copy C resolves its own rows",
          JSON.stringify(raceSent.filter(function (r) { return r.c === "paths" })[1].rows), "[2]")
    Ops.pathsResolved(race, ["/d/C"])
    Ops.paste(race)
    check("r1: newest Copy C wins after resolving", JSON.stringify(race.asked[1]),
          JSON.stringify({c: "transfer", op: "copy", paths: ["/d/C"], dest: "/d"}))
    var cutSent = []
    var cutPane = pathsPane(cutSent)
    cutPane.pathsPending = { kind: "drag" }
    Ops.clip(cutPane, true)
    check("a cut refuses out loud while a drag claim is waiting",
          cutSent.length + "|" + String(cutPane.clipPending) + "|" + JSON.stringify(cutPane.said),
          "0|null|[[\"Still resolving the last selection; try again.\",false]]")
    // An overlapping Cut cannot rewrite the outstanding Copy verb.
    var overlapSent = []
    var overlapPane = pathsPane(overlapSent)
    overlapPane.clipboard = {paths: ["/d/before"], moving: false}
    var overlapAt = 0
    overlapPane.selectedIndices = function () { return [overlapAt] }
    Ops.clip(overlapPane, false)
    overlapAt = 1
    Ops.clip(overlapPane, true)
    check("hunt: pending Copy queues a second Cut", overlapSent.length, 1)
    Ops.pathsResolved(overlapPane, ["/d/f0"])
    check("hunt: older Copy reply preserves the previous clipboard",
          overlapPane.clipboard.moving, false)
    Ops.paste(overlapPane)
    check("hunt: Paste uses the previous clipboard while the latest Cut waits",
          JSON.stringify(overlapPane.asked[0]),
          JSON.stringify({ c: "transfer", op: "copy", paths: ["/d/before"], dest: "/d" }))
    check("hunt: deferred Cut asks for its own selection", JSON.stringify(overlapSent[1]),
          JSON.stringify({ c: "paths", rows: [1] }))
    Ops.pathsResolved(overlapPane, ["/d/f1"])
    check("hunt: later Cut wins the whole clipboard", JSON.stringify(overlapPane.clipboard),
          JSON.stringify({ paths: ["/d/f1"], moving: true, token: "" }))
    Ops.paste(overlapPane)
    check("hunt: Paste moves only the later Cut selection", JSON.stringify(overlapPane.asked[1]),
          JSON.stringify({ c: "transfer", op: "move", paths: ["/d/f1"], dest: "/d" }))
    var latestSent = [], latest = pathsPane(latestSent), at = 0
    latest.selectedIndices = function () { return [at] }
    Ops.clip(latest, false)
    at = 1
    Ops.clip(latest, true)
    at = 2
    Ops.clip(latest, false)
    Ops.pathsResolved(latest, ["/d/f0"])
    check("hunt: only the latest deferred selection resolves", JSON.stringify(latestSent[1].rows), "[2]")
    Ops.pathsResolved(latest, ["/d/f2"])
    check("hunt: deferred Copy retains its own verb", JSON.stringify(latest.clipboard),
          JSON.stringify({ paths: ["/d/f2"], moving: false, token: "" }))
    var direct = pathsPane([])
    Ops.clip(direct, false)
    Ops.clip(direct, true, ["/d/menu-choice"])
    Ops.pathsResolved(direct, ["/d/old-copy"])
    check("hunt: old paths cannot replace a newer resolved menu choice", JSON.stringify(direct.clipboard),
          JSON.stringify({ paths: ["/d/menu-choice"], moving: true, token: "" }))
    var staleSent = [], stale = pathsPane(staleSent)
    stale.backend.heldListing = 1
    Ops.clip(stale, false)
    Ops.clip(stale, true)
    stale.backend.heldListing = 2
    Ops.pathsResolved(stale, ["/d/f0"])
    check("hunt: deferred row numbers cannot follow a re-list", staleSent.length, 1)
    check("hunt: stale deferred selection never publishes the older Copy", stale.clipboard, null)
    failedClipboard(check, pathsPane)
}

function failedClipboard(check, pathsPane) {
    // Sample input: function onFailed(where, input, message, mode) { ... } precedes the state connection.
    var handler = Source.slice(Source.source("ui/PaneWire.qml"),
        "function onFailed(where, input, message, mode) {", "\n    }\n\n    // flea --ui-state")
    var failedFor = eval("(function (pane, root) {\n" + handler + "\nreturn onFailed\n})")
    var sent = []
    var pane = pathsPane(sent)
    var initialListing = 1
    var currentListing = 2
    var selectedAt = 0
    var cutRow = 1
    pane.clipboard = {paths: ["/d/previous"], moving: false}
    pane.backend.heldListing = initialListing
    pane.backend.askPaths = function (rows) {
        sent.push({c: "paths", rows: rows, listing: pane.backend.heldListing})
    }
    pane.selectedIndices = function () { return [selectedAt] }
    var backend = {
        failed: failedFor(pane, {}),
        paths: function (paths) { Ops.pathsResolved(pane, paths) },
        rows: function (start, rows, ms, kinds, listing) { pane.backend.heldListing = listing }
    }
    Ops.clip(pane, false)
    Messages.route(backend, {t: "rows", start: 0, rows: [], ms: 0, listing: currentListing})
    selectedAt = cutRow
    Ops.clip(pane, true)
    check("r2: Cut B waits behind Copy A from the previous numbering", sent.length, 1)
    Messages.route(backend, {t: "error", where: "stale", path: "paths", msg: "rows out of date"})
    check("r2: stale Copy A error sends queued Cut B with current numbering", JSON.stringify(sent[1]),
        JSON.stringify({c: "paths", rows: [cutRow], listing: currentListing}))
    check("r2: stale Copy A error preserves the previous clipboard while Cut B waits",
        JSON.stringify(pane.clipboard), JSON.stringify({paths: ["/d/previous"], moving: false}))
    Messages.route(backend, {t: "paths", paths: ["/d/B"]})
    Ops.paste(pane)
    check("r2: Paste moves B after stale Copy A and queued Cut B", JSON.stringify(pane.asked[0]),
        JSON.stringify({c: "transfer", op: "move", paths: ["/d/B"], dest: "/d"}))
    check("r2: successful queued Cut leaves no pending request", pane.clipPending, null)
    check("r2: successful queued Cut drains its queue", pane.clipQueue.length, 0)
    Ops.clip(pane, false)
    var sentBeforeFailure = sent.length
    Messages.route(backend, {t: "error", where: "stale", path: "paths", msg: "rows out of date"})
    check("r2: failed latest Copy is never sent again", sent.length, sentBeforeFailure)
    check("r2: failed latest Copy keeps the last published Cut",
        JSON.stringify(pane.clipboard), JSON.stringify({paths: ["/d/B"], moving: true, token: ""}))
    check("r2: failed latest Copy releases its pending request", pane.clipPending, null)
    check("r2: failed latest Copy removes only its finished request", pane.clipQueue.length, 0)
    // A Permissions ask refused as stale (the watch relisted before it arrived) frees the claim and says why, so the next ask is not busy.
    pane.pathsPending = {kind: "permissions"}
    var saidBefore = pane.said.length
    Messages.route(backend, {t: "error", where: "stale", path: "paths", msg: "rows out of date"})
    check("stale Permissions ask releases its paths claim", pane.pathsPending, null)
    check("stale Permissions ask tells the user nothing was done", JSON.stringify(pane.said.slice(saidBefore)), JSON.stringify([["The listing changed before that arrived, so nothing was done.", true]]))
    var sentBeforeAsk = sent.length
    Ops.copyAs(pane, "path", null)
    check("the next ask after a stale Permissions refusal is not busy", pane.said.length, saidBefore + 1)
    check("the next ask after a stale Permissions refusal claims the paths", JSON.stringify(pane.pathsPending), JSON.stringify({kind: "copyAs", format: "path", base: "/d"}))
    check("the next ask after a stale Permissions refusal is sent", sent.length, sentBeforeAsk + 1)
}
