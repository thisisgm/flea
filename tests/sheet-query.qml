//@ pragma ShellId flea-sheet-query-test

import QtQuick
import QtTest
import Quickshell
import "flea" as Flea

// tests/sheet-query.sh's harness: the real ui/KeymapSheet.qml over a stub pane measures CommandPalette's query states.
ShellRoot {
    id: root

    property var failures: []
    property int checks: 0
    property int phase: 0
    // The menu answers for a regular file under the cursor; a dead context is a pane with nothing to act on.
    property var fileContext: ({ hasRow: true, hiddenActions: [], selectionCount: 1, rowMode: 0o100644, selectionModes: [0o100644],
        cursorIsTarget: true, rowIsFile: true, clipboardAvailable: true, canTrash: true, dirWritable: true, archiveFormats: ["zip"] })
    property var deadContext: ({ hasRow: true, hiddenActions: [], selectionCount: 0, rowMode: 0, selectionModes: [] })
    property var activated: []
    // Both ticks wait a frame so the sheet's bindings and layout settle before a read.
    readonly property int settleMs: 250

    function expect(label, ok, detail) {
        root.checks += 1
        if (!ok) root.failures.push(label + " got " + detail)
    }
    function walk(item, out) {
        for (var i = 0; i < item.children.length; i++) {
            out.push(item.children[i])
            root.walk(item.children[i], out)
        }
        return out
    }
    // Shown text only: the resting grid stays built beneath a query and carries its own ? cap.
    function texts(value) {
        return root.walk(sheet.cardItem, []).filter(function (item) { return item.visible && item.text === value })
    }
    function rowsOf() { return sheet.rows().split("\n") }

    Item {
        id: holder
        property bool dualMode: false
        property string home: "/home/probe"
        property var sidebar: null
        property bool listInFlight: false
        property var cursorRow: ({ n: "a.txt", d: false })
        property var context: root.fileContext
        function sheetMenuAction(action) { root.activated.push(action) }
        function checkShebang() {}
        function message(text, sticky) {}
        function contextMenu() { return { listingContext: function () { return holder.context } } }
    }

    FloatingWindow {
        implicitWidth: 900
        implicitHeight: 700
        color: "#303030"
        // The stage stands in for the window's height, which an offscreen window never changes.
        Item {
            id: stage
            width: parent.width
            height: parent.height
            Flea.KeymapSheet { id: sheet; anchors.fill: parent }
        }
        Item { anchors.fill: parent; TestEvent { id: driver } }
    }

    Timer {
        interval: root.settleMs
        running: true
        repeat: true
        onTriggered: root.advance()
    }

    function advance() {
        if (root.phase === 0) {
            sheet.open(holder)
            root.restWidth = sheet.capWidth
            root.restLabelX = sheet.cardItem.x
            root.phase = 1
        } else if (root.phase === 1) {
            root.restTitleY = root.titleY()
            root.restCardTop = sheet.cardItem.y
            root.checkTop("rest")
            sheet.query = "perm"
            root.phase = 2
        } else if (root.phase === 2) {
            root.checkTop("perm")
            root.checkLift()
            root.checkPerm()
            sheet.query = "trash"
            root.phase = 3
        } else if (root.phase === 3) {
            root.checkTop("trash")
            root.checkPlace()
            root.checkBackspace()
            sheet.open(holder)
            sheet.recentPaths = [root.longRecent]
            sheet.query = "open"
            root.phase = 4
        } else if (root.phase === 4) {
            root.checkTop("open")
            root.checkLongName()
            sheet.open(holder)
            sheet.query = "comp"
            root.phase = 5
        } else if (root.phase === 5) {
            root.checkTop("comp")
            root.checkCompress()
            // A query whose rows outgrow the window keeps the title and scrolls inside the clamp.
            sheet.query = "e"
            root.phase = 6
        } else if (root.phase === 6) {
            root.checkTop("a taller state than the window")
            root.expect("the tall state is clamped, so it scrolls inside", sheet.cardItem.height < root.tallWanted(), sheet.cardItem.height)
            sheet.query = ""
            root.phase = 7
        } else if (root.phase === 7) {
            root.checkTop("the empty query")
            // A window taller than the rest card, where the card sits centred and its top is no clamp's.
            stage.height = root.tallWindowHeight
            sheet.open(holder)
            root.phase = 8
        } else if (root.phase === 8) {
            root.restTitleY = root.titleY()
            root.restCardTop = sheet.cardItem.y
            root.expect("the window took its new height", sheet.height === root.tallWindowHeight, sheet.height)
            root.expect("a tall window centres the rest card", sheet.cardItem.y > sheet.clampMargin, sheet.cardItem.y)
            sheet.query = "perm"
            root.phase = 9
        } else if (root.phase === 9) {
            root.checkTop("tall window perm")
            sheet.query = "comp"
            root.phase = 10
        } else if (root.phase === 10) {
            root.checkTop("tall window comp")
            sheet.query = "e"
            root.phase = 11
        } else if (root.phase === 11) {
            root.checkTop("tall window crossing its bottom")
            sheet.query = ""
            root.phase = 12
        } else if (root.phase === 12) {
            root.checkTop("tall window empty query")
            sheet.query = "mute"
            root.phase = 13
        } else if (root.phase === 13) {
            root.checkMute()
            root.phase = 14
            root.report()
        }
    }
    // A recent file whose name outruns the card, in a folder whose muted suffix must still be read.
    readonly property string longRecent: "/home/probe/Documents/" + "a-very-long-recent-file-name-".repeat(8) + "end.txt"
    readonly property int keyDelayMs: -1
    property int restWidth: 0
    readonly property int tallWindowHeight: 1000
    property real restTitleY: -1
    property real restCardTop: -1
    // CommandPalette callout 1: the title and the card's top stay where the rest card put them, and the card grows from its bottom.
    function titleY() {
        var title = root.texts("Keys")
        return title.length === 1 ? title[0].mapToItem(sheet, 0, 0).y : -2
    }
    function checkTop(state) {
        root.expect(state + ": the title stays where the rest card put it", root.titleY() === root.restTitleY, root.titleY() + " vs " + root.restTitleY)
        root.expect(state + ": the card's top stays", sheet.cardItem.y === root.restCardTop, sheet.cardItem.y + " vs " + root.restCardTop)
        root.expect(state + ": the card stays inside the window", sheet.cardItem.y + sheet.cardItem.height <= sheet.height - sheet.clampMargin, (sheet.cardItem.y + sheet.cardItem.height) + " of " + sheet.height)
    }
    // CommandPalette "the cursor lift": the wash runs edge to edge across the card's inner width, while the text keeps its columns.
    function checkLift() {
        var lifts = root.walk(sheet.cardItem, []).filter(function (item) {
            return item.visible && item.height === sheet.rowPitch && item.color !== undefined
                && item.color.toString() === Qt.alpha(Flea.Theme.color.foreground, Flea.Theme.washHover).toString() })
        root.expect("one cursor lift is drawn", lifts.length === 1, lifts.length)
        if (lifts.length !== 1)
            return
        var edge = Flea.Theme.spacing.hairline
        var painted = lifts[0].mapToItem(sheet.cardItem, 0, 0)
        root.expect("the lift starts at the card's inner left edge", painted.x === edge, painted.x)
        root.expect("and spans its inner width", lifts[0].width === sheet.cardItem.width - 2 * edge, lifts[0].width + " of " + sheet.cardItem.width)
        var caps = root.walk(sheet.cardItem, []).filter(function (item) { return item.visible && item.text === "shift-delete" })
        var capLeft = caps.length === 1 ? caps[0].mapToItem(sheet.cardItem, 0, 0).x : -1
        root.expect("the cap keeps its column inside the lift", capLeft >= Flea.Theme.spacing.rowPaddingX, capLeft)
    }
    // CommandPalette callout 3: the preview's own key says where in the muted ink, in the name the UI shows.
    function checkMute() {
        var wheres = root.texts(" in Preview")
        root.expect("mute carries its muted in Preview suffix", wheres.length === 1 && wheres[0].color.toString() === Flea.Theme.color.muted.toString(), wheres.length)
        root.expect("and no raw context id is drawn", root.texts(" in media").length === 0, root.rowsOf().join("|"))
    }
    function tallWanted() { return sheet.queryResults.length * sheet.rowPitch }
    property real restLabelX: 0

    function checkPerm() {
        // Item 4: one cap column in every state.
        root.expect("the cap column keeps its rest width under a query", sheet.capWidth === root.restWidth, sheet.capWidth + " vs " + root.restWidth)
        // Item 1: one action, one row.
        var labels = sheet.queryResults.map(function (row) { return row.label })
        root.expect("delete permanently lists once", labels.filter(function (l) { return l.toLowerCase() === "delete permanently" }).length === 1, labels.join("|"))
        // The IPC reader the native capture asserts on lists the same rows, a disabled one marked.
        root.expect("the sheet reads back its results", root.rowsOf().indexOf("shift-delete Delete permanently") >= 0 && root.rowsOf().indexOf(" Permissions") >= 0, root.rowsOf().join("|"))
        // Item 3: the board's query line, the prompt in the cap column and the field on the label column.
        var prompts = root.texts("?")
        root.expect("a muted ? prompt is drawn", prompts.length === 1 && prompts[0].color.toString() === Flea.Theme.color.muted.toString(), prompts.length)
        if (prompts.length === 1) {
            root.expect("the prompt fills the cap column", prompts[0].width === sheet.capWidth && prompts[0].horizontalAlignment === Text.AlignHCenter, prompts[0].width)
            var fields = root.walk(sheet.cardItem, []).filter(function (item) {
                return item.border !== undefined && item.border.color.toString() === Flea.Theme.color.accent.toString() && item.height > 0 })
            root.expect("one hairline accent field frames the query", fields.length === 1 && fields[0].border.width === Flea.Theme.spacing.hairline, fields.length)
            if (fields.length === 1)
                root.expect("the field starts on the label column", fields[0].x === sheet.capWidth + sheet.capGap, fields[0].x)
        }
        // The washed run of a matching label is the same word at caption size; the field draws it at body size.
        var typed = root.texts("perm").filter(function (item) { return item.font.pixelSize === Flea.Theme.font.body })
        root.expect("the query text is drawn at body size", typed.length === 1, typed.length)
        // Item 5: the header reads esc closes under a query.
        root.expect("the header reads esc closes", root.texts("esc closes").length === 1 && root.texts("esc clears").length === 0, "closes=" + root.texts("esc closes").length)
        // Item 2: Permissions is live over a file, and Enter runs it through the menu.
        var permissionsAt = -1
        var results = sheet.queryResults
        for (var i = 0; i < results.length; i++)
            if (results[i].label === "Permissions") permissionsAt = i
        root.expect("a Permissions row is listed", permissionsAt >= 0, permissionsAt)
        root.expect("and it is available over a file", permissionsAt >= 0 && results[permissionsAt].disabled === false, "disabled")
        sheet.resultCursor = permissionsAt
        sheet.activateResult()
        root.expect("Enter runs Permissions through the menu", root.activated.join(",") === "permissions", root.activated.join(","))
        // With nothing to act on the row is disabled and Enter is refused.
        sheet.open(holder)
        holder.context = root.deadContext
        sheet.query = "perm"
        results = sheet.queryResults
        permissionsAt = -1
        for (var j = 0; j < results.length; j++)
            if (results[j].label === "Permissions") permissionsAt = j
        root.expect("Permissions reads disabled with nothing to act on", permissionsAt >= 0 && results[permissionsAt].disabled === true, permissionsAt)
        root.activated = []
        sheet.resultCursor = permissionsAt
        sheet.activateResult()
        root.expect("and Enter is refused", root.activated.length === 0 && sheet.opened, root.activated.join(","))
        holder.context = root.fileContext
    }

    function checkPlace() {
        // Item 6: a place result says where inline, muted, right after its name.
        var wheres = root.walk(sheet.cardItem, []).filter(function (item) { return item.text === " in Places" })
        root.expect("a place row carries its muted suffix", wheres.length >= 1, wheres.length)
        if (wheres.length >= 1) {
            var where = wheres[0]
            var row = where.parent
            var label = null
            for (var i = 0; i < row.children.length; i++)
                if (row.children[i].text === "Open Trash") label = row.children[i]
            root.expect("the suffix sits right after the name", label !== null && Math.abs(where.x - (label.x + label.width)) <= 1, label ? where.x + " vs " + (label.x + label.width) : "no label")
            root.expect("and not at the card's right edge", where.x + where.contentWidth < row.width - sheet.capWidth, where.x + " + " + where.contentWidth + " of " + row.width)
            root.expect("in the muted ink", where.color.toString() === Flea.Theme.color.muted.toString(), where.color)
        }
    }

    // Backspace through the sheet's own Keys handler: three typed characters come off one at a time.
    function checkBackspace() {
        sheet.open(holder)
        var word = "tag"
        for (var i = 0; i < word.length; i++)
            driver.keyClickChar(word.charAt(i), Qt.NoModifier, root.keyDelayMs)
        root.expect("typing reads back whole", sheet.query === word, sheet.query)
        var steps = []
        for (var j = 0; j < word.length; j++) {
            driver.keyClick(Qt.Key_Backspace, Qt.NoModifier, root.keyDelayMs)
            steps.push(sheet.query)
        }
        root.expect("Backspace empties the query a character at a time", steps.join("|") === "ta|t|", steps.join("|"))
        root.expect("and the last one leaves the resting sheet open", sheet.opened === true, sheet.opened)
    }

    // A3: the name elides and the muted suffix keeps its whole width, inside the card.
    function checkLongName() {
        var wheres = root.walk(sheet.cardItem, []).filter(function (item) { return item.text === " in ~/Documents" })
        root.expect("the long recent file carries its suffix", wheres.length === 1, wheres.length)
        if (wheres.length !== 1)
            return
        var where = wheres[0]
        var row = where.parent
        var label = null
        for (var i = 0; i < row.children.length; i++)
            if (row.children[i].text === "Open " + root.longRecent.split("/").pop())
                label = row.children[i]
        root.expect("its name is drawn", label !== null, "no label")
        if (label === null)
            return
        var painted = where.mapToItem(sheet.cardItem, 0, 0)
        root.expect("the name is elided", label.width < label.implicitWidth, label.width + " of " + label.implicitWidth)
        root.expect("the suffix keeps its whole width", where.width === where.implicitWidth, where.width + " of " + where.implicitWidth)
        root.expect("the suffix lies inside the card", painted.x >= 0 && painted.x + where.width <= sheet.cardItem.width, painted.x + " + " + where.width + " of " + sheet.cardItem.width)
        root.expect("and starts where the name ends", Math.abs(where.x - (label.x + label.width)) <= 1, where.x + " vs " + (label.x + label.width))
    }

    // CommandPalette "After typing comp": the Compress leaf draws as its own row, capless and with no muted "in Compress".
    function checkCompress() {
        var rows = root.rowsOf()
        root.expect("the comp query lists the Compress leaf as its own row", rows.indexOf(" Compress to .zip") >= 0, rows.join("|"))
        root.expect("the leaf holds the cursor and no Compress parent row stands first", rows[0] === " Compress to .zip" && rows.indexOf(" Compress") < 0 && sheet.resultCursor === 0, rows.join("|"))
        root.expect("no leaf row carries a cap", rows.filter(function (row) { return row.indexOf("Compress to .") >= 0 && row.charAt(0) !== " " }).length === 0, rows.join("|"))
        var suffixes = root.walk(sheet.cardItem, []).filter(function (item) {
            return item.visible && typeof item.text === "string" && item.text.indexOf(" in Compress") >= 0 })
        root.expect("the leaf draws no in Compress suffix", suffixes.length === 0, suffixes.length)
        var drawn = root.walk(sheet.cardItem, []).filter(function (item) { return item.visible && item.text === "Compress to .zip" })
        root.expect("the leaf's wording is drawn once", drawn.length === 1, drawn.length)
    }

    function report() {
        for (var f = 0; f < root.failures.length; f++)
            console.log("SHEETQUERY FAIL " + root.failures[f])
        if (root.failures.length === 0)
            console.log("SHEETQUERY PASS checks=" + root.checks)
        console.log("SHEETQUERY DONE failures=" + root.failures.length)
        Quickshell.execDetached(["kill", String(Quickshell.processId)])
    }
}
