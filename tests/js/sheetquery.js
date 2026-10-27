.import "../../ui/js/SheetQuery.js" as SheetQuery
.import "../../ui/js/SheetKeys.js" as SheetKeys
.import "../../ui/js/Input.js" as Input
.import "../../ui/js/Keymap.js" as Keymap
.import "../../ui/js/Swap.js" as Swap
.import "sourcefixture.js" as Source

function run(check) {
    // Candidates arrive in section order, each place and file reading "Open <name>".
    function candidate(label, keys, section, where) {
        return { label: label, keys: keys, section: section, where: where || "" }
    }
    var rows = [
        candidate("move", "j k", 0),
        candidate("trash", "dd", 0),
        candidate("menu", "m", 0),
        candidate("Open Downloads", "", 1, "Places"),
        candidate("Open Documents", "", 1, "Places"),
        candidate("Open receipt.pdf", "", 2, "Downloads")
    ]
    // At rest the sheet draws whole: an empty query filters nothing.
    check("an empty query keeps every row", SheetQuery.rank(rows, "").length, 6)
    check("and keeps their order", SheetQuery.rank(rows, "  ")[0].label, "move")
    // A label match is case-insensitive and keeps section order among its own.
    var open = SheetQuery.rank(rows, "open")
    check("a label substring matches", open.length, 3)
    check("section order holds among matches", open.map(function (row) { return row.label }).join("|"),
          "Open Downloads|Open Documents|Open receipt.pdf")
    check("upper case matches too", SheetQuery.rank(rows, "OPEN").length, 3)
    // A place whose name the query matches exactly ranks first, whatever section it is in.
    var exact = SheetQuery.rank(rows, "menu")
    check("an exact label match ranks first", exact[0].label, "menu")
    // An exact label outranks a substring match even when the substring stands first in section order.
    var exactRows = [candidate("menu bar", "", 0), candidate("menu", "m", 0)]
    check("an exact label outranks a substring match ahead of it",
          SheetQuery.rank(exactRows, "menu").map(function (row) { return row.label }).join("|"), "menu|menu bar")
    var place = SheetQuery.rank(rows, "Open Documents")
    check("an exact place name ranks first", place[0].label, "Open Documents")
    // Keys match when no label does: the cap is what half the sheet is read by.
    var keys = SheetQuery.rank(rows, "dd")
    check("a keys match answers", keys.length, 1)
    check("with the row it names", keys[0].label, "trash")
    // A query matching nothing lists nothing rather than the whole sheet.
    check("no match is an empty sheet", SheetQuery.rank(rows, "zzz").length, 0)
    check("a keys-only miss is empty too", SheetQuery.rank(rows, "Open ^z").length, 0)
    // The native capture types the board's specimens through cap_sheet_type, and perm must still find its shipped row.
    var capture = Source.slice(Source.source("tests/ui-captures.sh"), "case_cap_sheet() {", "\nmatrix_check() {")
    // Sample input: cap_sheet_type perm
    var typed = capture.match(/^\s*cap_sheet_type [a-z]+$/gm) || []
    var captureQueries = typed.map(function (line) { return line.trim().split(/\s+/)[1] })
    check("the capture types the place, place and recent, leaves, one-place key, Permissions and delete card specimens in order", captureQueries.join(","), "trash,fl,comp,mute,perm,perm")
    var captureRows = SheetQuery.rank(SheetQuery.actionCandidates(Keymap.sheetFor("default", "gui", false)), captureQueries[4])
    check("the capture query finds its shipped board row",
          captureRows.map(function (row) { return row.keys + " " + row.label }).join("\n"), "shift-delete Delete permanently")
    // CommandPalette callout 3: mute works in the Preview only, so its row reads "m mute" with that where.
    var muteRows = SheetQuery.rank(SheetQuery.actionCandidates(Keymap.sheetFor("default", "gui", false)), captureQueries[3])
    check("the capture mute query finds its shipped row with its where",
          muteRows.map(function (row) { return row.keys + " " + row.label + " in " + row.where }).join("\n"), "m Mute in Preview")
    // CommandPalette "After typing fl": the favourite flea leads the recent file mix.flac, each with its own where.
    var flRows = SheetQuery.rank(SheetQuery.placeCandidates([{ label: "flea", group: "favourite", kind: "favourite", path: "/home/probe/flea" }]).concat(
        SheetQuery.recentCandidates(["/home/probe/Documents/claude/mix.flac"], "/home/probe")), captureQueries[1])
    check("fl lists the favourite then the recent file", flRows.map(function (row) { return row.label + " in " + row.where }).join("|"),
          "Open flea in Favorites|Open mix.flac in ~/Documents/claude")
    // An exact NAME match ranks first, above an exact action label.
    var trashRows = [
        { label: "trash", keys: "dd", section: 0, where: "", action: "trash" },
        { label: "Open Trash", name: "Trash", keys: "", section: 2, where: "Places",
          railIndex: 0, entry: { label: "Trash" } }
    ]
    check("an exact place name beats an exact action label",
          SheetQuery.rank(trashRows, "trash")[0].label, "Open Trash")
    // Sections hold their order inside each bucket: actions, menu rows, places, recents.
    var sections = [
        candidate("open sesame", "", 0),
        candidate("Open Sesame", "", 1),
        candidate("Open Sesame", "", 2),
        candidate("Open Sesame", "", 3)
    ]
    check("section order holds across all four sections",
          SheetQuery.rank(sections, "sesame").map(function (row) { return row.section }).join("|"), "0|1|2|3")
    // The result list stays bounded and the cursor starts on the first row, the menu lift.
    var many = []
    for (var i = 0; i < 80; i++) {
        many.push(candidate("open " + i, "", i % 4))
    }
    check("the list is bounded", SheetQuery.rank(many, "open").length <= SheetQuery.RESULT_LIMIT, true)
    // A flyout parent whose leaf is listed leaves the list, so the cursor starts on the leaf (CommandPalette "After typing comp").
    var flyout = [
        { label: "Compress", keys: "", section: 1, where: "", menuAction: "compress" },
        { label: "Compress to .zip", keys: "", section: 1, where: "", menuAction: "compress:zip", parentAction: "compress" },
        { label: "Open with", keys: "", section: 1, where: "", menuAction: "openWith" }
    ]
    check("the leaf holds the first row under comp", SheetQuery.rank(flyout, "comp").map(function (row) { return row.label }).join("|"), "Compress to .zip")
    check("a parent no leaf matched keeps its row", SheetQuery.rank(flyout, "open w").map(function (row) { return row.label }).join("|"), "Open with")
    // A parent stays when the bound cuts its leaves, so a long list never loses both.
    var crowded = [{ label: "Compress", keys: "", section: 1, where: "", menuAction: "compress" }]
    for (var k = 0; k < SheetQuery.RESULT_LIMIT; k++)
        crowded.push(candidate("compress note " + k, "", 2))
    crowded.push({ label: "Compress to .zip", keys: "", section: 1, where: "", menuAction: "compress:zip", parentAction: "compress" })
    var crowdedLabels = SheetQuery.rank(crowded, "comp").map(function (row) { return row.label })
    check("a parent whose leaves the bound cut keeps its row", crowdedLabels.indexOf("Compress") >= 0 && crowdedLabels.indexOf("Compress to .zip") < 0, true)
    // The matched run is what the sheet washes.
    var at = SheetQuery.matchOf("Compress to .zip", "comp")
    check("a match names its run start", at && at.start, 0)
    check("and its run length", at && at.length, 4)
    check("a miss washes nothing", SheetQuery.matchOf("trash", "zzz"), null)
    // A key that works in one place only says where, from the key table's own context.
    check("listing names no place", SheetQuery.whereForContext("listing"), "")
    check("a multi-context key names none either", SheetQuery.whereForContext("rail,menu"), "")
    check("the media preview is named as the UI names it", SheetQuery.whereForContext("media"), "Preview")
    check("a context the UI never names is left out", SheetQuery.whereForContext("pdf"), "")
    check("a raw id is never the suffix", SheetQuery.whereForContext("all"), "")

    // A sheet menu row closes then the holder runs it, but it refuses while a listing is out.
    var calls = []
    var holder = { listInFlight: false,
        message: function (text, sticky) { calls.push("message:" + text + ":" + sticky) },
        sheetMenuAction: function (action) { calls.push("run:" + action) } }
    SheetQuery.runMenu(holder, "trash", function () { calls.push("close") })
    check("the sheet closes before its holder runs the row", calls.join(","), "close,run:trash")
    var blocked = []
    var busy = { listInFlight: true,
        message: function (text, sticky) { blocked.push("message:" + text + ":" + sticky) },
        sheetMenuAction: function (action) { blocked.push("run:" + action) } }
    SheetQuery.runMenu(busy, "trash", function () { blocked.push("close") })
    check("a menu row refuses while a listing is out", blocked.join(","), "message:A directory is already loading.:false")

    // A printable key types into the query; DEL never does, so Delete types nothing invisible.
    check("a letter is printable", Input.isPrintable("c"), true)
    check("a space is printable", Input.isPrintable(" "), true)
    check("DEL is not printable", Input.isPrintable("\u007f"), false)
    check("empty is not printable", Input.isPrintable(""), false)
    check("two chars are not printable", Input.isPrintable("ab"), false)
    // A bare modifier press is no query edit and never closes the sheet, so a shifted letter types.
    check("Shift alone is a bare modifier", SheetKeys.isBareModifier(Qt.Key_Shift), true)
    check("Control alone is a bare modifier", SheetKeys.isBareModifier(Qt.Key_Control), true)
    check("Alt alone is a bare modifier", SheetKeys.isBareModifier(Qt.Key_Alt), true)
    check("AltGr alone is a bare modifier", SheetKeys.isBareModifier(Qt.Key_AltGr), true)
    check("Meta alone is a bare modifier", SheetKeys.isBareModifier(Qt.Key_Meta), true)
    check("CapsLock alone is a bare modifier", SheetKeys.isBareModifier(Qt.Key_CapsLock), true)
    check("Delete is no bare modifier", SheetKeys.isBareModifier(Qt.Key_Delete), false)
    check("Escape is no bare modifier", SheetKeys.isBareModifier(Qt.Key_Escape), false)
    // The key decision the sheet's Keys.onPressed runs: CommandPalette callout 2, Esc closes the sheet from any state.
    check("Esc closes from a standing query", SheetKeys.sheetKey("co", Qt.Key_Escape, ""), "close")
    check("Esc with none closes", SheetKeys.sheetKey("", Qt.Key_Escape, ""), "close")
    // The header reads "esc closes" in every state, and no branch of the sheet still empties a query on Esc.
    var sheetText = Source.source("ui/KeymapSheet.qml")
    check("the header never reads esc clears", sheetText.indexOf("esc clears"), -1)
    check("the header reads esc closes in every state", sheetText.indexOf('text: "esc closes"') >= 0, true)
    check("the sheet has no clear branch", sheetText.indexOf('decision === "clear"'), -1)
    // Backspace shortening the query a character at a time is driven through the sheet's own handler, in tests/sheet-query.qml.
    check("Up moves the cursor", SheetKeys.sheetKey("c", Qt.Key_Up, ""), "up")
    check("Down moves the cursor", SheetKeys.sheetKey("c", Qt.Key_Down, ""), "down")
    check("Return runs the row", SheetKeys.sheetKey("c", Qt.Key_Return, ""), "activate")
    check("Enter runs the row too", SheetKeys.sheetKey("c", Qt.Key_Enter, ""), "activate")
    check("Backspace shortens a query", SheetKeys.sheetKey("c", Qt.Key_Backspace, ""), "backspace")
    check("Backspace with none closes", SheetKeys.sheetKey("", Qt.Key_Backspace, ""), "close")
    check("a letter types", SheetKeys.sheetKey("", Qt.Key_C, "c"), "type")
    check("Shift alone is ignored", SheetKeys.sheetKey("co", Qt.Key_Shift, ""), "ignore")
    check("Shift first is ignored too", SheetKeys.sheetKey("", Qt.Key_Shift, ""), "ignore")
    check("Delete never types DEL", SheetKeys.sheetKey("co", Qt.Key_Delete, "\u007f"), "ignore")
    check("an unbound key closes", SheetKeys.sheetKey("co", Qt.Key_F1, ""), "close")
    // The cursor wraps at both ends, the way the sheet's arrows move.
    check("Up at the top wraps to the last", SheetKeys.stepCursor(0, -1, 3), 2)
    check("Down at the end wraps to the first", SheetKeys.stepCursor(2, 1, 3), 0)
    check("a middle step does not wrap", SheetKeys.stepCursor(1, -1, 3), 0)
    check("no rows parks the cursor", SheetKeys.stepCursor(0, 1, 0), 0)
    // The place lookup answers the rail's own index, never the first label match.
    var dupes = [
        { label: "src", group: "favourite", kind: "favourite", path: "/a/src", original: { label: "src", path: "/a/src" } },
        { label: "src", group: "favourite", kind: "favourite", path: "/b/src", original: { label: "src", path: "/b/src" } }
    ]
    var second = SheetQuery.placeCandidates(dupes).filter(function (row) { return row.railIndex === 1 })[0]
    check("the second src is offered", second.label, "Open src")
    check("the second src opens rail row 1", SheetQuery.placeIndex(dupes, SheetQuery.dispatch(second)), 1)
    var first = SheetQuery.placeCandidates(dupes).filter(function (row) { return row.railIndex === 0 })[0]
    check("the first src still opens rail row 0", SheetQuery.placeIndex(dupes, SheetQuery.dispatch(first)), 0)
    check("a gone row answers -1", SheetQuery.placeIndex([], SheetQuery.dispatch(second)), -1)
    // The pane's own half of a menu row: it stands the menu on the row, snapshots, then activates, in that order.
    var paneRun = Source.slice(Source.source("ui/Pane.qml"), "function sheetMenuAction(action)", "function permissionSelection()")
    var paneOrder = ["menu.openedIdentity = root.menuSelectionIdentity()", "menuActions.snapshot()", "menuActions.activate(action, true)"]
        .map(function (step) { return paneRun.indexOf(step) })
    check("the pane stands the menu on the row, snapshots, then activates", paneOrder[0] >= 0 && paneOrder[0] < paneOrder[1] && paneOrder[1] < paneOrder[2], true)
    // The sheet action arm lives in SheetQuery.runAction, so a swapped gate stays red.
    function stubHolder(inFlight) {
        var calls = []
        var holder = { listInFlight: inFlight, message: function (text, shown) { calls.push("message:" + text + ":" + shown) }, act: function (action) { calls.push("act:" + action) } }
        function close() { calls.push("close") }
        return { holder: holder, calls: calls, close: close }
    }
    check("SheetQuery.runAction exists", typeof SheetQuery.runAction, "function")
    var swallowed = stubHolder(true)
    if (typeof SheetQuery.runAction === "function") {
        SheetQuery.runAction(swallowed.holder, "trash", swallowed.close)
    }
    check("a swallowed action says loading with no close and no act", swallowed.calls.join(",") || "missing", "message:" + Swap.LOADING + ":false")
    var idle = stubHolder(false)
    if (typeof SheetQuery.runAction === "function") {
        SheetQuery.runAction(idle.holder, "trash", idle.close)
    }
    check("an idle action closes then acts", idle.calls.join(",") || "missing", "close,act:trash")
    var letThrough = stubHolder(true)
    if (typeof SheetQuery.runAction === "function") {
        SheetQuery.runAction(letThrough.holder, "open", letThrough.close)
    }
    check("an action the gate lets through acts while in flight", letThrough.calls.join(",") || "missing", "close,act:open")
    // Query actions reach the same window signals as physical keys.
    var window = stubHolder(false)
    window.holder.pathBarRequested = function () { window.calls.push("window:path") }
    window.holder.textSizeRequested = function (direction) { window.calls.push("window:size:" + direction) }
    SheetQuery.runAction(window.holder, "pathBar", window.close)
    SheetQuery.runAction(window.holder, "textSizeUp", window.close)
    check("query Path and Larger reach the window dispatcher", window.calls.join(","), "close,window:path,close,window:size:1")
    // Merge keeps every window action on the shared sheet, rail and list dispatcher.
    var windowActions = ["windowNew", "reload", "sidebar", "openTerminal", "settings", "copydirpath",
        "tabNew", "tabClose", "tab1", "tabNext", "tabPrevious", "tabMoveLeft", "tabMoveRight"]
    windowActions.forEach(function (action) {
        var routed = stubHolder(false)
        SheetQuery.runAction(routed.holder, action, routed.close)
        check("query window action closes then dispatches " + action, routed.calls.join(","), "close,act:" + action)
    })
    var trashCalls = []
    SheetQuery.runMenu({ sheetMenuAction: function (action) { trashCalls.push(action) } }, "deletePermanently", function () { trashCalls.push("close") })
    check("Trash menu rows keep their host's confirmation path", trashCalls.join(","), "close,deletePermanently")
    var sheetAction = Source.source("ui/KeymapSheet.qml")
    check("activateResult runs through SheetQuery.runAction", Source.slice(sheetAction, "function activateResult()", "// Directive 18").indexOf("SheetQuery.runAction") >= 0, true)

    // CommandPalette "After typing": a muted ? in the cap column, the query in a field on the label column, its focus a hairline accent frame (decided 2026-10-03).
    var lineSource = Source.slice(sheetText, "// The query line, drawn only while one stands", "// The query results replace")
    check("the query line draws the board's ? prompt", lineSource.indexOf('text: "?"') >= 0, true)
    check("the prompt is centred in the cap column", lineSource.indexOf("width: root.capWidth") >= 0
          && lineSource.indexOf("horizontalAlignment: Text.AlignHCenter") >= 0, true)
    check("the prompt is muted at bodySmall", lineSource.indexOf("font.pixelSize: Theme.font.bodySmall") >= 0
          && lineSource.indexOf("color: Theme.color.muted") >= 0, true)
    check("the field starts on the label column", lineSource.indexOf("anchors.leftMargin: root.capWidth + root.capGap") >= 0, true)
    check("the field is a hairline frame in the accent", lineSource.indexOf("border.width: Theme.spacing.hairline") >= 0
          && lineSource.indexOf("border.color: Theme.color.accent") >= 0, true)
    check("the field draws no foreground ring", lineSource.indexOf("Buttons.RING"), -1)
    check("the field holds a caret after the text", lineSource.indexOf("id: queryCaret") >= 0, true)
    // Sample input: "font.pixelSize: Theme.font.body\n" is body size, "font.pixelSize: Theme.font.bodySmall\n" is not.
    function readsAtBody(block) { return /font\.pixelSize: Theme\.font\.body(?![A-Za-z0-9_])/.test(block) }
    var queryText = Source.slice(lineSource, "id: queryLine", "id: queryCaret")
    check("the query reads at body size like a field", readsAtBody(queryText), true)
    check("and a bodySmall query line would not pass that check", readsAtBody(queryText.replace("Theme.font.body\n", "Theme.font.bodySmall\n")), false)
    // One cap column in every state: the sheet the column is measured over is the whole table, never the query's results.
    var tableSource = Source.slice(sheetText, "readonly property var sheet:", "// The query results across")
    check("the sheet the cap column measures does not narrow with the query", tableSource.indexOf("root.query"), -1)
    check("the ranked results are the only thing the query narrows", tableSource.indexOf("SheetQuery.rank"), -1)
    // A place or recent result says where inline, muted, right after its name.
    var resultText = Source.source("ui/KeymapSheetResult.qml")
    var hitSource = Source.slice(resultText, "id: hitLabel", "id: hitWhere")
    check("the name does not stop at a right-anchored suffix", hitSource.indexOf("anchors.right: hitWhere.left"), -1)
    var whereSource = Source.slice(resultText, "id: hitWhere", "elide: Text.ElideRight\n    }\n}")
    check("the suffix follows the name", whereSource.indexOf("anchors.left: hitLabel.right") >= 0, true)
    check("the suffix reads as a space then in, inline with the name", whereSource.indexOf('" in " + hit.whereText') >= 0, true)
}
