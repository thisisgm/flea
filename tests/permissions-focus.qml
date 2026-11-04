//@ pragma ShellId flea-permissions-focus-test
import QtQuick
import QtTest
import Quickshell
import "flea" as Flea
import "permissions-layout.js" as Layout

// Real menu dispatch, Permissions and row input, with no focus repair before the tested input.
ShellRoot {
    id: root
    readonly property var pane: body.currentPane
    readonly property string fixture: Quickshell.env("FLEA_PATH")
    readonly property int fixtureRows: 4
    readonly property string draftMode: "0777"
    readonly property int pollMs: 20
    readonly property int stageLimitMs: 5000
    readonly property int fadeLimitMs: 1000
    readonly property int pointerEventMs: 1
    readonly property int ownerExecuteBit: 0o100
    readonly property var markedRows: [1, 3]
    readonly property var refreshCases: [
        {view: "list", side: 0}, {view: "grid", side: 0}, {view: "columns", side: 0},
        {view: "dual", side: 0}, {view: "dual", side: 1}
    ]
    readonly property var cases: [
        {dismiss: "Escape", input: "Down"}, {dismiss: "Cancel", input: "Down"},
        {dismiss: "Escape", input: "Right"}, {dismiss: "Cancel", input: "Right"},
        {dismiss: "Apply", input: "Down"}, {dismiss: "Apply", input: "Right"}
    ]
    property int caseIndex: 0
    property int refreshIndex: 0
    property string keptMarks: ""
    property int keptCursor: 0
    property int beforeLists: 0
    property int undoReplies: 0
    property int beforeUndoReplies: 0
    property bool undoInspectSent: false
    property var undoPaths: []
    property var expectedModes: []
    property var undoModes: []
    readonly property int undoInspectBase: 1000000
    property string stage: "guardReady"
    property int checks: 0
    property int failures: 0
    property int oldCursor: 0
    property double started: Date.now()
    // When the dismissed stage first saw the dialog closed and the listing settled; the fade budget runs from here.
    property double closedAt: 0
    property bool executing: false
    property bool finished: false
    property var observedRow: null
    property var observedTap: null
    property int rowActivations: 0
    property point heldPoint: Qt.point(0, 0)
    property var heldEntries: null
    readonly property string reopenedApp: "Reopened provider"
    readonly property string flyoutApp: "Held flyout provider"
    readonly property var current: cases[caseIndex]

    function label() { return caseIndex < cases.length ? current.dismiss + "/" + current.input : "batch/" + refreshCases[refreshIndex].view + "/" + refreshCases[refreshIndex].side }
    function rememberSelection() { keptMarks = pane.selectedIndices().join(","); keptCursor = pane.cursorIndex }
    function checkSelection(action) {
        check(action + " keeps marked rows", pane.selectedIndices().join(","), keptMarks)
        check(action + " keeps cursor row", pane.cursorIndex, keptCursor)
    }
    function check(name, actual, expected) {
        checks += 1
        var equal = actual === expected
        if (!equal) failures += 1
        console.log("PERMFOCUS " + (equal ? "ok " : "FAIL ") + label() + " " + name
                    + ": got " + actual + ", expected " + expected)
    }
    function ipcObject(orNull) {
        for (var i = 0; i < body.data.length; i++)
            if (String(body.data[i].item).indexOf("Ipc_") === 0) return body.data[i].item
        if (orNull) return null  // a Loader of the body holds the seam once the first listing answers
        throw new Error("WindowBody has no IPC seam")
    }
    function dialog() { return ipcObject().permissionsDialog }
    function control(name) {
        var controls = dialog().controls()
        for (var i = 0; i < controls.length; i++)
            if (controls[i].name === name) return controls[i].item
        throw new Error("Permissions has no " + name)
    }
    function press(key) { driver.keyClick(key, Qt.NoModifier, -1) }
    function applications(label) {
        pane.backend.menuResult({op: "applications", id: pane.menuActions.requestId,
            applications: [{id: "permfocus.desktop", label: label, icon: "", default: true}]})
    }
    function hasApplication(menu, label) {
        return menu.entries.some(function(entry) {
            return entry.action === "openWith" && entry.submenu.some(function(app) { return app.label === label })
        })
    }
    function settledRow(menu, action) {
        if (menu.pointerSettling || menu.providersRefreshing || !menu.openWithLoaded || menu.localSend.checking) return null
        var at = menu.entries.findIndex(function(entry) { return entry.action === action })
        var expectedY = 0
        for (var i = 0; i <= at; i++) {
            var row = menu.itemFor(i)
            if (!row || row.y !== expectedY || row.width <= 0 || row.height <= 0) return null
            expectedY += row.height
        }
        return at >= 0 ? menu.itemFor(at) : null
    }
    function trace(event) {
        var menu = pane.contextMenu()
        var at = menu.entries.findIndex(function(entry) { return entry.action === "permissions" })
        var row = at >= 0 ? menu.itemFor(at) : null
        console.log("PERMFOCUS trace " + label() + " " + event + " " + JSON.stringify({
            activeFocusItem: ipcObject(true) ? JSON.parse(ipcObject().seam.keyDeliveryState()).activeFocusItem : "no seam yet",
            opened: menu.opened, cursor: menu.cursor, listInFlight: pane.listInFlight,
            pointerSettling: menu.pointerSettling, providersRefreshing: menu.providersRefreshing,
            openWithLoaded: menu.openWithLoaded, localSendChecking: menu.localSend.checking,
            row: String(row), rowCentre: row ? window.centreOf(row) : "", rowY: row ? row.y : -1,
            tapPressed: observedTap ? observedTap.pressed : false, rowActivations: rowActivations
        }))
    }
    function click(item, button) {
        var point = body.mapFromItem(item, item.width / 2, item.height / 2)
        if (stage === "menu") trace("before press")
        driver.mousePress(body, point.x, point.y, button, Qt.NoModifier, pointerEventMs)
        if (stage === "menu") trace("after press")
        if (stage === "menu") {
            check("Permissions receives the press", observedTap && observedTap.pressed, true)
            // Deliver the real applications reply path while the pointer still holds the row.
            pane.backend.menuResult({op: "applications", id: pane.menuActions.requestId,
                applications: [{id: "permfocus.desktop", label: "Focus fixture", icon: "", default: true}]})
            check("applications reply retains the pressed row", pane.contextMenu().itemFor(pane.contextMenu().cursor) === observedRow, true)
        }
        driver.mouseRelease(body, point.x, point.y, button, Qt.NoModifier, pointerEventMs)
        if (stage === "menu") trace("after release")
        if (stage === "menu") check("release activates Permissions once", rowActivations, 1)
    }
    function next(stage) { root.stage = stage; started = Date.now(); closedAt = 0 }
    function finish() {
        finished = true
        console.log("PERMFOCUS DONE checks=" + checks + " failed=" + failures)
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
        if (Date.now() - started > stageLimitMs) {
            trace("stage timeout " + stage)
            check("stage " + stage + " completed", false, true)
            finish()
            return
        }
        if (stage === "guardReady") {
            if (pane.listInFlight || pane.path !== fixture || pane.total !== fixtureRows || !pane.visibleItemFor(1)) return
            pane.setCursor(1)
            pane.listArea.forceActiveFocus()
            driver.keyClickChar("m", Qt.NoModifier, -1)
            next("guardMenu")
        } else if (stage === "guardMenu") {
            var closingMenu = pane.contextMenu()
            if (!settledRow(closingMenu, "permissions")) return
            applications("Before close")
            closingMenu.openSubmenu(closingMenu.entries.findIndex(function(entry) { return entry.action === "openWith" }))
            next("guardClose")
        } else if (stage === "guardClose") {
            var closingMenu = pane.contextMenu()
            var closingRow = closingMenu.submenuItemFor(0)
            if (!closingRow || closingRow.width <= 0 || closingRow.height <= 0 || closingRow.y !== 0) return
            heldPoint = body.mapFromItem(closingRow, closingRow.width / 2, closingRow.height / 2)
            driver.mousePress(body, heldPoint.x, heldPoint.y, Qt.LeftButton, Qt.NoModifier, pointerEventMs)
            check("closing menu row is pressed", closingRow.pressed, true)
            applications("Reply before close")
            press(Qt.Key_Escape)
            press(Qt.Key_Escape)
            check("Escape closes menu before release", closingMenu.opened, false)
            driver.keyClickChar("m", Qt.NoModifier, -1)
            check("new menu opens before old release", closingMenu.opened, true)
            next("guardReopened")
        } else if (stage === "guardReopened") {
            var reopenedMenu = pane.contextMenu()
            if (!settledRow(reopenedMenu, "permissions")) return
            applications(reopenedApp)
            check("new menu accepts provider reply after held row destruction", hasApplication(reopenedMenu, reopenedApp), true)
            driver.mouseRelease(body, heldPoint.x, heldPoint.y, Qt.LeftButton, Qt.NoModifier, pointerEventMs)
            reopenedMenu.close()
            driver.keyClickChar("m", Qt.NoModifier, -1)
            next("flyoutMenu")
        } else if (stage === "flyoutMenu") {
            var flyoutMenu = pane.contextMenu()
            if (!settledRow(flyoutMenu, "permissions")) return
            applications(reopenedApp)
            var parentAt = flyoutMenu.entries.findIndex(function(entry) { return entry.action === "openWith" })
            flyoutMenu.cursor = parentAt
            flyoutMenu.openSubmenu(parentAt)
            next("flyoutPress")
        } else if (stage === "flyoutPress") {
            var heldMenu = pane.contextMenu()
            var flyoutRow = heldMenu.submenuItemFor(0)
            if (!flyoutRow || flyoutRow.width <= 0 || flyoutRow.height <= 0 || flyoutRow.y !== 0) return
            heldPoint = body.mapFromItem(flyoutRow, flyoutRow.width / 2, flyoutRow.height / 2)
            driver.mousePress(body, heldPoint.x, heldPoint.y, Qt.LeftButton, Qt.NoModifier, pointerEventMs)
            check("flyout row receives press", flyoutRow.pressed, true)
            heldEntries = heldMenu.entries
            applications(flyoutApp)
            check("provider reply keeps inventory while flyout held", heldMenu.entries === heldEntries, true)
            check("provider reply keeps pressed flyout delegate", heldMenu.submenuItemFor(0) === flyoutRow, true)
            next("flyoutHeld")
        } else if (stage === "flyoutHeld") {
            var releasingMenu = pane.contextMenu()
            check("deferred refresh still waits for flyout release", releasingMenu.entries === heldEntries, true)
            // Release outside the row leaves the menu open so the owed refresh can be observed.
            driver.mouseRelease(body, 0, 0, Qt.LeftButton, Qt.NoModifier, pointerEventMs)
            next("flyoutReleased")
        } else if (stage === "flyoutReleased") {
            var refreshedMenu = pane.contextMenu()
            if (!hasApplication(refreshedMenu, flyoutApp)) return
            check("flyout release runs owed provider refresh", hasApplication(refreshedMenu, flyoutApp), true)
            refreshedMenu.close()
            next("ready")
        } else if (stage === "ready") {
            if (pane.listInFlight || pane.path !== fixture || pane.total !== fixtureRows || !pane.visibleItemFor(1) || root.ipcObject(true) === null) return
            pane.clearSelection()
            observedRow = null
            observedTap = null
            rowActivations = 0
            pane.setCursor(current.dismiss === "Cancel" ? 0 : 1)
            pane.listArea.forceActiveFocus()
            if (current.dismiss === "Escape") {
                click(pane.visibleItemFor(pane.cursorIndex), Qt.LeftButton)
                driver.keyClickChar("m", Qt.NoModifier, -1)
            } else click(pane.visibleItemFor(pane.cursorIndex), Qt.RightButton)
            check("native gesture opens menu", pane.contextMenu().opened, true)
            next("menu")
        } else if (stage === "menu") {
            var menu = pane.contextMenu()
            if (menu.pointerSettling || menu.providersRefreshing || !menu.openWithLoaded || menu.localSend.checking) return
            var at = menu.entries.findIndex(function(entry) { return entry.action === "permissions" })
            if (at >= 0 && menu.cursor !== at) { menu.cursor = at; return }
            var row = at >= 0 ? menu.itemFor(at) : null
            // A new Repeater row has y=0 until Column positions it; it is not a painted click target yet.
            var expectedY = 0
            for (var index = 0; index <= at; index++) {
                var positioned = menu.itemFor(index)
                if (!positioned || positioned.y !== expectedY || positioned.width <= 0 || positioned.height <= 0) return
                expectedY += positioned.height
            }
            if (row && !menu.frameItem.contains(menu.frameItem.mapFromItem(row, row.width / 2, row.height / 2))) return
            check("menu offers Permissions", at >= 0 && !menu.entries[at].disabled, true)
            check("menu refuses listing input readiness", JSON.parse(ipcObject().seam.permissionsState()).inputReady, false)
            observedRow = row
            for (var child = 0; row && child < row.data.length; child++)
                if (String(row.data[child]).indexOf("QQuickTapHandler") === 0) observedTap = row.data[child]
            trace("activate Permissions")
            if (current.dismiss === "Escape") press(Qt.Key_Return)
            else click(row, Qt.LeftButton)
            next("dialog")
        } else if (stage === "dialog") {
            var card = dialog()
            if (!card || !card.opened || card.busy) return
            Layout.checkLayout(root, card, Flea.Theme)
            check("menu closes before dialog", pane.contextMenu().opened, false)
            check("fixture editability", card.editable, current.dismiss !== "Cancel")
            check("open dialog refuses input readiness", JSON.parse(ipcObject().seam.permissionsState()).inputReady, false)
            if (current.dismiss === "Escape") {
                click(control("Octal"), Qt.LeftButton)
                driver.keyClick(Qt.Key_A, Qt.ControlModifier, -1)
                for (var digit = 0; digit < draftMode.length; digit++)
                    driver.keyClickChar(draftMode.charAt(digit), Qt.NoModifier, -1)
                press(Qt.Key_Escape)
            } else {
                if (current.dismiss === "Apply") rememberSelection()
                click(control(current.dismiss), Qt.LeftButton)
            }
            next("dismissed")
        } else if (stage === "dismissed") {
            if (dialog().opened || pane.listInFlight || pane.wire.stale || pane.wire.anchor) return
            if (!closedAt) closedAt = Date.now()
            // The 140 ms fade-out keeps the dialog drawn after opened flips; past fadeLimitMs a dialog still drawn fails the check below.
            if (dialog().visible && Date.now() - closedAt < fadeLimitMs) return
            check("dialog is hidden", dialog().visible, false)
            console.log("PERMFOCUS focus " + label() + " " + ipcObject().seam.keyDeliveryState())
            check("listing owns keyboard", pane.listArea.activeFocus, true)
            check("dismissal reports actual input readiness", JSON.parse(ipcObject().seam.permissionsState()).inputReady, true)
            if (current.dismiss === "Apply") checkSelection("single Apply")
            oldCursor = pane.cursorIndex
            if (current.input === "Down") press(Qt.Key_Down)
            else click(pane.visibleItemFor(oldCursor), Qt.RightButton)
            next("input")
        } else if (stage === "input") {
            if (current.input === "Down") check("first Down moves cursor", pane.cursorIndex, oldCursor + 1)
            else check("first right press/release opens menu", pane.contextMenu().opened, true)
            pane.contextMenu().close()
            caseIndex += 1
            if (caseIndex === cases.length) next("refreshView")
            else next("ready")
        } else if (stage === "refreshView") {
            Flea.ViewState.changeKey("view", refreshCases[refreshIndex].view)
            next("refreshPane")
        } else if (stage === "refreshPane") {
            body.focusPane(refreshCases[refreshIndex].side)
            if (pane.listInFlight || pane.listingState === "loading") return
            if (pane.path !== fixture) { pane.open(fixture); return }
            if (pane.total !== fixtureRows || pane.wire.anchor) return
            pane.selectOnly(markedRows[0])
            for (var mark = 1; mark < markedRows.length; mark++) pane.toggleSelectAt(markedRows[mark])
            pane.setCursor(markedRows[markedRows.length - 1])
            rememberSelection()
            check("batch marks are nonadjacent", keptMarks, markedRows.join(","))
            pane.openPermissions()
            next("refreshDialog")
        } else if (stage === "refreshDialog") {
            if (!dialog() || !dialog().opened || dialog().busy) return
            check("dialog takes both marked files", dialog().multiPaths.length, markedRows.length)
            Layout.checkLayout(root, dialog(), Flea.Theme)
            if (refreshIndex === 0) Layout.checkStatuses(root, dialog(), Flea.Theme)
            undoPaths = dialog().multiPaths.slice()
            expectedModes = dialog().multiModes.slice()
            dialog().multiToggle(ownerExecuteBit)
            beforeLists = pane.backend.listRequests
            click(control("Apply"), Qt.LeftButton)
            next("refreshApplied")
        } else if (stage === "refreshApplied") {
            if (dialog().opened || pane.listInFlight || pane.wire.anchor || pane.backend.listRequests <= beforeLists) return
            check("batch Apply returns the keyboard to the visible view", pane.listArea.activeFocus, true)
            checkSelection("batch Apply")
            // A watched change owes a re-read that lands after this click: it must keep the click's one mark and cursor, not restore the batch's.
            pane.backend.changed(pane.path)
            check("a watched change owes the re-read", pane.wire.stale, true)
            beforeLists = pane.backend.listRequests
            click(pane.visibleItemFor(0), Qt.LeftButton)
            rememberSelection()
            check("click before the re-read marks its row", keptMarks, "0")
            next("owedPaid")
        } else if (stage === "owedPaid") {
            if (pane.listInFlight || pane.wire.anchor || pane.wire.stale || pane.backend.listRequests <= beforeLists) return
            checkSelection("click before the re-read")
            beforeLists = pane.backend.listRequests
            beforeUndoReplies = undoReplies
            undoInspectSent = false
            undoModes = []
            pane.backend.send({c: "undo"})
            next("refreshUndone")
        } else if (stage === "refreshUndone") {
            if (undoReplies <= beforeUndoReplies || pane.listInFlight || pane.wire.anchor || pane.wire.stale || pane.backend.listRequests <= beforeLists) return
            if (!undoInspectSent) {
                undoInspectSent = true
                for (var inspected = 0; inspected < undoPaths.length; inspected++)
                    pane.backend.send({c: "permissions", op: "inspect", id: undoInspectBase + refreshIndex * markedRows.length + inspected, path: undoPaths[inspected]})
                return
            }
            if (undoModes.length !== expectedModes.length || expectedModes.some(function (mode, i) { return undoModes[i] === undefined })) return
            for (var restored = 0; restored < expectedModes.length; restored++) {
                check("Undo restores on-disk mode " + restored, undoModes[restored], expectedModes[restored])
                if (undoModes[restored] !== expectedModes[restored]) {
                    finish()
                    return
                }
            }
            checkSelection("Permissions Undo")
            refreshIndex += 1
            if (refreshIndex === refreshCases.length) finish()
            else next("refreshView")
        }
    }

    Connections {
        target: root.pane.backend
        function onUndone(op, ok) {
            if (root.stage === "refreshUndone" && op === "permissions" && ok) root.undoReplies += 1
        }
        // Sample input: {op: "inspect", id: 1000000, ok: true, mode: "0644"}.
        function onPermissionsResult(message) {
            var offset = message.id - root.undoInspectBase - root.refreshIndex * root.markedRows.length
            if (root.stage !== "refreshUndone" || message.op !== "inspect" || offset < 0 || offset >= root.undoPaths.length) return
            root.undoModes[offset] = message.ok ? message.mode : "Inspection failed"
            root.pane.backend.send({c: "permissions", op: "close", id: message.id})
        }
    }

    Connections {
        target: root.observedRow
        function onActivated() { root.rowActivations += 1; root.trace("row activated") }
    }
    Connections {
        target: root.pane.contextMenu()
        function onChosen(action) { root.trace("menu chosen " + action) }
        function onEntriesChanged() { if (!root.finished) root.trace("entries rebuilt") }
    }
    Connections {
        target: root.observedTap
        function onPressedChanged() { root.trace("tap pressed changed") }
        function onTapped() { root.trace("tap received") }
    }

    FloatingWindow {
        id: window
        implicitWidth: 1000
        implicitHeight: 800
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
