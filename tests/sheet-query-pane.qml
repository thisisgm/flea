//@ pragma ShellId flea-sheet-query-pane-test
import QtQuick
import QtTest
import Quickshell
import "flea" as Flea
import "flea/js/SheetQuery.js" as SheetQuery

// tests/sheet-query.sh's pane half: under shipped defaults, Enter on a hidden menu row in the real sheet reaches that row's own surface with no menu opened.
ShellRoot {
    id: root
    readonly property var pane: body.currentPane
    readonly property var sheet: pane.keymapSheet.item
    readonly property string fixture: Quickshell.env("FLEA_PATH")
    readonly property int fixtureRows: 4
    readonly property string copiedName: "b.txt"
    readonly property int cursorRow: 1
    readonly property int pollMs: 20
    readonly property int stageLimitMs: 8000
    // Compress runs last: its archive joins the listing and moves the cursor row.
    readonly property var cases: [
        { name: "Permissions", query: "perm", label: "Permissions" },
        { name: "Move to", query: "move to", label: "Move to" },
        { name: "Delete permanently confirm", action: "deletePermanently" },
        { name: "Copy as leaf", query: "shell-quoted", label: "Shell-quoted" },
        { name: "Compress leaf", query: "compress to .", label: "Compress to .zip" }
    ]
    property int caseIndex: 0
    property string stage: "ready"
    property int checks: 0
    property int failures: 0
    property bool finished: false
    property bool executing: false
    property double started: Date.now()
    property int archiveStarts: 0
    property int archiveDones: 0
    property int startsBefore: 0
    property string lastMessage: ""
    readonly property var current: cases[caseIndex]
    readonly property string hiddenAction: "permissions"
    readonly property string rightClickName: "Right-click menu"
    readonly property string refusedText: "That action is no longer available; reopen the menu."
    readonly property string copierMarker: "wl-copy"
    readonly property int copierTextArg: 4
    readonly property real menuPointX: 120
    readonly property real menuPointY: 120

    function check(name, actual, expected) {
        checks += 1
        var equal = actual === expected
        if (!equal) failures += 1
        console.log("SHEETPANE " + (equal ? "ok " : "FAIL ") + (current ? current.name : rightClickName) + " " + name + ": got " + actual + ", expected " + expected)
    }
    function ipcObject() {
        for (var i = 0; i < body.data.length; i++)
            if (String(body.data[i].item).indexOf("Ipc_") === 0) return body.data[i].item
        return null  // the seam is built after the first listing, and it is the item a Loader of the body holds
    }
    function permissionsOpen() {
        var state = JSON.parse(ipcObject().seam.permissionsState())
        return state.opened === true && state.busy !== true
    }
    // The text the Opener last handed wl-copy, "" before any copy; the Process is the Opener's own child.
    function copiedText() {
        var kids = pane.opener.data
        for (var i = 0; i < kids.length; i++) {
            var command = kids[i].command
            if (command !== undefined && String(command).indexOf(copierMarker) >= 0) return String(command[copierTextArg])
        }
        return ""
    }
    // True once the case's own surface is up: the Permissions dialog, a started compress, or the menu's dialog card.
    function observed() {
        if (current.name === "Permissions") return permissionsOpen()
        if (current.name === "Compress leaf") return archiveStarts > startsBefore
        if (current.name === "Copy as leaf") return copiedText().indexOf(copiedName) >= 0
        if (current.name === "Move to") return pane.menuActions.opened && pane.menuActions.dialogFor === "moveTo"
        return pane.menuActions.opened && pane.menuActions.dialogFor === "deletePermanently"
    }
    function idle() {
        return !pane.listInFlight && !pane.menuActions.opened && !permissionsOpen() && !pane.keymapSheet.opened
            && archiveDones >= archiveStarts
    }
    function typeQuery(word) {
        for (var i = 0; i < word.length; i++) driver.keyClickChar(word.charAt(i), Qt.NoModifier, -1)
    }
    function pressKey(key) { driver.keyClick(key, Qt.NoModifier, -1) }
    function pickIndex() {
        var results = sheet.queryResults
        for (var i = 0; i < results.length; i++)
            if ((current.label !== undefined && results[i].label === current.label)
                    || (current.where !== undefined && results[i].where === current.where && results[i].menuAction !== undefined))
                return i
        return -1
    }
    function next(stageName) { stage = stageName; started = Date.now() }
    function finish() {
        finished = true
        console.log("SHEETPANE DONE checks=" + checks + " failed=" + failures)
        body.quitBackends()
    }
    function advance() {
        if (executing || finished) return
        executing = true
        try { runStage() }
        catch (error) { check("harness error", String(error), "none"); finish() }
        executing = false
    }
    function runStage() {
        var timedOut = Date.now() - started > stageLimitMs
        if (stage === "ready") {
            if (timedOut) { check("fixture listed", false, true); finish(); return }
            if (pane.listInFlight || pane.path !== fixture || pane.total < fixtureRows || !pane.visibleItemFor(cursorRow) || root.ipcObject() === null) return
            next("open")
        } else if (stage === "open") {
            if (!idle() && !timedOut) return
            if (caseIndex === 0)
                check("the profile hides " + hiddenAction, pane.contextMenu().listingContext().hiddenActions.indexOf(hiddenAction) >= 0, true)
            pane.setCursor(cursorRow)
            pane.listArea.forceActiveFocus()
            lastMessage = ""
            startsBefore = archiveStarts
            // Opened as Focus.windowAction opens it: the sheet is the only thing the cursor row's menu was ever asked for.
            pane.keymapSheet.open(pane)
            if (current.action !== undefined) {
                // A confirm row has a key in every preset, so the row the sheet's dispatch would hand runMenu is run directly.
                SheetQuery.runMenu(pane, current.action, function () { sheet.close() })
                next("ran")
                return
            }
            typeQuery(current.query)
            next("typed")
        } else if (stage === "typed") {
            if (sheet.query !== current.query && !timedOut) return
            check("the query reads back whole", sheet.query, current.query)
            var pickAt = pickIndex()
            check("the row is listed", pickAt >= 0, true)
            if (pickAt < 0) {
                sheet.close()
                next("settle")
                return
            }
            // Down moves the real cursor; reading its row first makes a ranking change fail by name.
            for (var step = 0; step < pickAt; step++) pressKey(Qt.Key_Down)
            check("the highlighted row is the one picked", sheet.queryResults[sheet.resultCursor].label, sheet.queryResults[pickAt].label)
            pressKey(Qt.Key_Return)
            next("ran")
        } else if (stage === "ran") {
            if (!observed() && !timedOut) return
            check("Enter reached the menu's own surface (last message '" + lastMessage + "')", observed(), true)
            check("and the sheet closed", pane.keymapSheet.opened, false)
            // Escape dismisses the card the row opened; a compress needs only to finish.
            if (current.name !== "Compress leaf") pressKey(Qt.Key_Escape)
            next("settle")
        } else if (stage === "settle") {
            if (!idle() && !timedOut) return
            check("the pane came back to idle", idle(), true)
            caseIndex += 1
            if (caseIndex === cases.length) next("rightclick")
            else next("open")
        } else if (stage === "rightclick") {
            // A sheet activation must leave nothing behind: the pane's own menu still hides the row and still refuses it.
            pane.setCursor(cursorRow)
            var menu = pane.contextMenu()
            menu.openAt(pane.listArea.mapToItem(null, menuPointX, menuPointY))
            var shown = []
            for (var i = 0; i < menu.entries.length; i++) shown.push(menu.entries[i].action)
            check("the menu opened", menu.opened, true)
            check("the menu still omits the hidden row", shown.indexOf(hiddenAction) >= 0, false)
            check("the menu still refuses the hidden row", menu.validateChoice(hiddenAction, ""), false)
            menu.close()
            // A pick from outside the sheet carries no sheet request, so the backend's reply must refuse the row too.
            lastMessage = ""
            pane.menuActions.snapshot()
            pane.menuActions.activate(hiddenAction, true)
            next("pointer")
        } else if (stage === "pointer") {
            if (lastMessage.length === 0 && !permissionsOpen() && !timedOut) return
            check("a pick outside the sheet is refused", lastMessage, refusedText)
            check("and opens no Permissions dialog", permissionsOpen(), false)
            finish()
        }
    }

    Connections {
        target: root.pane
        function onMessage(text, isError) { root.lastMessage = text }
    }
    Connections {
        target: root.pane.backend
        function onArchiveStarted(id) { root.archiveStarts += 1 }
        function onArchiveDone(id, ok, verified, err) { root.archiveDones += 1 }
    }

    FloatingWindow {
        id: window
        implicitWidth: 1000
        implicitHeight: 800
        // The IPC seam's permissionsState reads the dialog's rectangle and centre through the window it lives in.
        function centreOf(item) {
            if (!item) return ""
            var rect = window.itemRect(item)
            return Math.round(rect.x + rect.width / 2) + " " + Math.round(rect.y + rect.height / 2)
        }
        function rectOf(item) {
            if (!item) return ""
            var rect = window.itemRect(item)
            var left = Math.round(rect.x), top = Math.round(rect.y)
            return left + " " + top + " " + (Math.round(rect.x + rect.width) - left) + " " + (Math.round(rect.y + rect.height) - top)
        }
        Flea.WindowBody { id: body; host: window }
        Item { anchors.fill: parent; TestEvent { id: driver } }
    }
    Timer { interval: root.pollMs; repeat: true; running: !root.finished; onTriggered: root.advance() }
}
