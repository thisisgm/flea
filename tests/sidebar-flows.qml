//@ pragma ShellId flea-sidebar-flows-test
import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Commons as Commons
import "flea" as Flea
import "flea/js/Filter.js" as Filter
import "flea/js/Format.js" as Format
import "flea/js/Search.js" as Search
import "sidebar-flows-extra.js" as Extra
import "sidebar-flows-palette.js" as PaletteFlow

// The real pane owns every tested property. TestEvent delivers keys to its real focused item.
ShellRoot {
    id: root
    readonly property var pane: body.currentPane
    readonly property string fixture: Quickshell.env("HOME") + "/fixture"
    property int checks: 0
    property int failures: 0
    property int stage: 0
    property double started: 0
    readonly property int settleGraceMs: 5000
    property bool executing: false
    property int beforeLists: 0
    property int reloadStage: 0
    property var reloadNotice: ({ from: -1, total: 0 })
    property var observedMessages: []
    property bool mutationDone: false
    property bool trashCountWas: false
    property bool driveSizeWas: false
    property string cursorBeforeRail: ""
    property double clickedAt: 0
    property int tagGenBefore: 0
    property int railHome: -1
    property int railRecent: -1
    property int railTrash: -1

    function check(name, actual, expected) {
        checks++
        var equal = JSON.stringify(actual) === JSON.stringify(expected)
        if (!equal) failures++
        console.log("SIDEBAR_FLOWS " + (equal ? "ok " : "FAIL ") + name
                    + ": got " + JSON.stringify(actual) + ", expected " + JSON.stringify(expected))
    }
    function ready(path) { return !pane.listInFlight && pane.path === path && pane.listingState === "ready" }
    function indexOf(name) {
        for (var i = 0; i < pane.rows.length; i++)
            if (pane.rows[i].n === name) return pane.held + i
        return -1
    }
    function pick(name) { pane.selectOnly(root.indexOf(name), 0) }
    function press(key, modifiers) {
        pane.listArea.forceActiveFocus()
        driver.keyClick(key, modifiers || Qt.NoModifier, -1)
    }
    function tagRow() { return pane.sidebar.entries.findIndex(function(e) { return e.kind === "tag" && e.tag === "Red" }) }
    // The Red rail row, activated through the sidebar's own route into TagState.openTag.
    function walkTag() {
        root.tagGenBefore = pane.searchGen
        pane.sidebar.activate(root.tagRow())
    }
    function tagWalked() { return pane.searchMode === "results" && pane.searchQuery === "tag:Red" && !pane.searchRunning }
    // The walk is Home's own: the pane stands in a folder of 5 rows, and only Home reaches the 40 files of Downloads (tests/sidebar-flows.sh).
    function tagChecks(label, queries) {
        check(label + ": the pane shows results", pane.searchMode, "results")
        check(label + ": the query is the tag", pane.searchQuery, "tag:Red")
        check(label + ": the walk is Home's, not the folder's", pane.searchHere, false)
        check(label + ": the queries sent", pane.searchGen - root.tagGenBefore, queries)
        check(label + ": the walk scanned Home", pane.searchScanned > 40, true)
    }
    function sheet() { return pane.keymapSheet.item }
    function openSheet() { root.press(Qt.Key_Question); return root.sheet() }
    function click(item, x, y, button) {
        driver.mousePress(item, x, y, button || Qt.LeftButton, Qt.NoModifier, 1)
        driver.mouseRelease(item, x, y, button || Qt.LeftButton, Qt.NoModifier, 1)
    }
    function move(item, x, y) { return driver.mouseMove(item, x, y, 1, Qt.NoButton, Qt.NoModifier) }
    function clickName(name) {
        var row = pane.visibleItemFor(root.indexOf(name))
        if (!row) throw new Error("No visible row " + name)
        // Sample input: 412 237 (rowNameCentre x y, window-relative), or empty for a row with no name.
        var centre = root.ipc().rowNameCentre(root.indexOf(name)).split(" ").map(Number)
        if (centre.length !== 2 || !centre.every(isFinite)) throw new Error("No name centre for " + name)
        var point = row.mapFromItem(null, centre[0], centre[1])
        root.click(row, point.x, point.y)
    }
    function ipc(whole) {
        for (var i = 0; i < body.data.length; i++)
            if (String(body.data[i].item).indexOf("Ipc_") === 0) return whole ? body.data[i].item : body.data[i].item.seam
        return null  // the seam is built after the first listing, and it is the item a Loader of the body holds
    }
    function find(item, type) {
        if (String(item).indexOf(type + "_") === 0) return item
        var children = item && item.children ? item.children : []
        for (var i = 0; i < children.length; i++) {
            var found = root.find(children[i], type)
            if (found) return found
        }
        return null
    }

    FloatingWindow {
        id: window
        implicitWidth: 1000
        implicitHeight: 650
        Flea.WindowBody { id: body; host: window }
        Item { anchors.fill: parent; TestEvent { id: driver } }
    }
    // Rail square-row gap probes, one SidebarRow per trailing-slot shape; the label's ink ends at its anchor edge.
    Item {
        id: railGap
        width: 240
        height: 4 * Flea.Theme.railRowHeight
        Flea.SidebarRow {
            id: probeSquare
            width: parent.width
            height: Flea.Theme.railRowHeight
            index: 0
            modelData: ({ label: "Archive", path: "", group: "network", kind: "share", uri: "smb://example.com/data", mounted: false, glyph: "server" })
        }
        Flea.SidebarRow {
            id: probeMark
            y: Flea.Theme.railRowHeight
            width: parent.width
            height: Flea.Theme.railRowHeight
            index: 1
            modelData: ({ label: "NAS", path: "", group: "network", kind: "share", uri: "smb://example.com/data", mounted: true, glyph: "server" })
        }
        Flea.SidebarRow {
            id: probeNumber
            y: 2 * Flea.Theme.railRowHeight
            width: parent.width
            height: Flea.Theme.railRowHeight
            index: 2
            modelData: ({ label: "Trash", path: "trash:///", group: "trash", kind: "trash", glyph: "trash", count: 3 })
        }
        Flea.SidebarRow {
            id: probeEmpty
            y: 3 * Flea.Theme.railRowHeight
            width: parent.width
            height: Flea.Theme.railRowHeight
            index: 3
            modelData: ({ label: "Home", path: "/home", group: "home", kind: "home", glyph: "house" })
        }
    }
    // What a rail row draws: one highlight fill and accent edge, with no keyboard frame.
    function railLook(row) {
        var look = { fill: false, edge: false, frame: false, color: "transparent" }
        for (var i = 0; i < row.children.length; i++) {
            var c = row.children[i]
            if (c.color === undefined || c.border === undefined || !c.visible) continue
            if (c.color.a === 0) look.frame = look.frame || (c.width < row.width && c.height === row.height)
            else if (c.width === Flea.Theme.spacing.hairline * 2) look.edge = true
            else if (c.width === row.width && c.opacity === 1) { look.fill = true; look.color = c.color }
        }
        return look
    }
    // Which rail rows draw each highlight part, by entry index.
    function railLit() { return root.railIndices("fill") }
    function railIndices(what) {
        var out = []
        for (var i = 0; i < pane.sidebar.entries.length; i++) {
            var row = pane.sidebar.railItemFor(i)
            if (row && root.railLook(row)[what]) out.push(i)
        }
        return out
    }
    // The gap from the label's end to what bounds it: the detail number when one stands, else the slot.
    function railLabelGap(row) {
        var target = row.detail.length > 0 ? row.detailItem : row.indicatorSlot
        return target.x - (row.labelItem.x + row.labelItem.width)
    }
    // One probe row per trailing shape the rail draws, pinned to the board's absolute edges (sidebar-flows-extra.js).
    Column {
        id: railBoard
        width: 240
        Repeater {
            id: railBoardRows
            model: Extra.RAIL_BOARD
            Flea.SidebarRow { }
        }
    }
    function railBoardReady() {
        var rows = railBoardRows
        for (var i = 0; i < rows.count; i++)
            if ((Extra.RAIL_BOARD[i].want === "number" || Extra.RAIL_BOARD[i].want === "count") !== (rows.itemAt(i).detail.length > 0)) return false
        return rows.count === Extra.RAIL_BOARD.length
    }
    function railBoardChecks() {
        for (var i = 0; i < railBoardRows.count; i++) {
            var row = railBoardRows.itemAt(i), kind = Extra.RAIL_BOARD[i]
            var edges = Extra.railBoardEdges(row, kind.want, Style.spacing.rowPaddingX, Style.spacing.rowGap, Flea.Theme.font.caption, Flea.Theme.railIconSize)
            check("rail-board: " + kind.name + " starts its label after the mark", row.labelItem.x, edges.left)
            check("rail-board: " + kind.name + " ends its label on the board's edge", row.labelItem.x + row.labelItem.width, edges.right)
            check("rail-board: " + kind.name + " elides at the end of its label", row.labelItem.elide, Text.ElideRight)
        }
    }
    // The drive sizes land after the Trash count and a poll, the way a reply lands in the real rail.
    Timer { id: lateSizes; interval: 250; onTriggered: Flea.ViewState.changeLeaf("places", { driveSize: true }) }
    Connections {
        target: root.pane
        // keyClick can deliver rows before returning, so record arming when it happens.
        function onReloadFromChanged() { if (pane.reloadFrom >= 0) root.reloadNotice = { from: pane.reloadFrom, total: root.reloadNotice.total } }
        function onMessage(text, isError) { root.observedMessages = root.observedMessages.concat([text]) }
    }
    Process {
        id: mutation
        command: ["touch", root.fixture + "/added.txt"]
        onExited: function(code) { root.check("fixture mutation completed", code, 0); root.mutationDone = true }
    }

    readonly property var steps: [
        function () {
            if (!ready(fixture) || pane.total !== 4 || pane.sidebar === null || root.ipc() === null) return false
            check("fresh Recent is off", pane.sidebar.recentEntries.length, 0)
            check("fresh Escape-up is off", pane.escapeUp, false)
            check("fresh opening is double click", pane.singleClick, false)
            check("fresh slow-click rename is on", pane.clickRename, true)
            pane.clearSelection(); pane.setCursor(0)
            root.press(Qt.Key_J)
            check("Qt key delivery moves the real cursor", pane.cursorIndex, 1)
            return true
        },
        function () {
            pane.chooseView("list")
            root.pick("a.txt")
            mutation.running = true
            return true
        },
        function () { return root.mutationDone && root.reloadChecks("list") },
        function () { pane.chooseView("grid"); return true },
        function () { return root.reloadChecks("grid") },
        function () { pane.chooseView("columns"); return true },
        function () { return root.reloadChecks("columns") },
        function () { pane.chooseView("list"); return true },
        function () {
            if (pane.viewMode !== "list" || !ready(fixture)) return false
            var s = root.openSheet()
            s.query = "go to folder"
            s.resultCursor = s.queryResults.findIndex(function(r) { return r.action === "pathBar" })
            check("control: sheet offers Go to folder", s.resultCursor >= 0, true)
            s.activateResult()
            var chrome = root.find(body, "ChromeBar")
            check("sheet-window-action: Go to folder opens its real field", chrome.editing, true)
            if (chrome.editing) driver.keyClick(Qt.Key_Escape, Qt.NoModifier, -1)
            return true
        },
        function () {
            root.pick("link.txt")
            var menu = pane.contextMenu()
            menu.openAt(Qt.point(400, 200))
            check("control: row menu offers Show original", menu.entries.some(function(e) { return e.action === "showOriginal" }), true)
            menu.close()
            var s = root.openSheet()
            check("question key opens real sheet", s && s.opened, true)
            if (!s || !s.opened) return true
            s.query = "show original"
            check("sheet-symlink: query finds the cursor menu's Show original", s.queryResults.some(function(e) { return e.menuAction === "showOriginal" }), true)
            s.close()
            pane.open(Quickshell.env("HOME") + "/readonly")
            return true
        },
        function () {
            if (!ready(Quickshell.env("HOME") + "/readonly")) return false
            check("control: backend reports read-only directory", pane.backend.dirWritable, false)
            root.pick("ro.txt")
            var menu = pane.contextMenu()
            menu.openAt(Qt.point(400, 200))
            check("control: row menu disables Rename", menu.entries.filter(function(e) { return e.action === "rename" })[0].disabled, true)
            menu.close()
            var s = root.openSheet()
            s.query = "rename"
            var row = s.queryResults.filter(function(e) { return e.menuAction === "rename" })[0]
            check("sheet-readonly: menu Rename stays disabled", row ? row.disabled : "absent", true)
            s.close()
            pane.open(fixture)
            return true
        },
        function () {
            if (!ready(fixture)) return false
            Flea.ViewState.changeLeaf("places", {rail: "hidden"})
            return true
        },
        function () {
            if (pane.sidebar !== null) return false
            var s = root.openSheet()
            s.query = "home"
            check("sheet-hidden-rail: exact Home place remains reachable", s.queryResults.some(function(e) { return e.section === 2 && e.name === "Home" }), true)
            s.close()
            Flea.ViewState.changeLeaf("places", {rail: "shown", showRecent: true})
            return true
        },
        function () {
            if (!pane.sidebar || pane.sidebar.recentEntries.length !== 1) return false
            pane.sidebar.activate(pane.sidebar.entries.findIndex(function(e) { return e.kind === "recent" }))
            return true
        },
        function () {
            if (pane.recentMode !== "results" || pane.listInFlight || pane.total !== 2) return false
            check("Recent preserves visit order", pane.rows.map(function(r) { return r.n.split("/").pop() }), ["a.txt", "b.txt"])
            var first = pane.visibleItemFor(0)
            if (!first) return false
            check("recent-used: Used is the XBEL visit date", first.dateText(), Format.date(Date.parse("2026-09-23T10:47:00Z") / 1000))
            check("Sidebar040 Recent Location is the folder under home, no home and no tilde slash", first.locationText, "fixture")
            pane.closeRecent()
            return true
        },
        function () {
            if (!ready(fixture)) return false
            Flea.ViewState.changeLeaf("places", { showTags: true })
            pane.tags().take([{ n: "tagged.txt", tg: ["Red"] }], true)
            return true
        },
        function () {
            if (root.tagRow() < 0) return false
            Filter.start(pane)
            Filter.typed(pane, "a")
            check("control: the filter stands before the tag row", pane.filterQuery, "a")
            root.walkTag()
            return true
        },
        function () {
            if (!root.tagWalked()) return false
            root.tagChecks("tag-filter", 1)
            check("tag-filter: the filter is closed", [pane.filterTyping, pane.filterQuery], [false, ""])
            root.press(Qt.Key_Escape)
            return true
        },
        function () {
            if (!ready(fixture) || pane.searchMode !== "") return false
            pane.sidebar.activate(pane.sidebar.entries.findIndex(function(e) { return e.kind === "recent" }))
            return true
        },
        function () {
            if (pane.recentMode !== "results" || pane.listInFlight || root.tagRow() < 0) return false
            root.walkTag()
            return true
        },
        function () {
            if (!root.tagWalked()) return false
            // Leaving Recent for the folder asks its rows once with an empty query, then the tag query follows.
            root.tagChecks("tag-recent", 2)
            check("tag-recent: Recent is left", pane.recentMode, "")
            root.press(Qt.Key_Escape)
            return true
        },
        function () {
            if (!ready(fixture) || pane.searchMode !== "") return false
            root.press(Qt.Key_F)
            pane.listArea.forceActiveFocus()
            driver.keyClickChar("a", Qt.NoModifier, -1)
            root.press(Qt.Key_Tab)
            root.press(Qt.Key_Return)
            check("control: a folder-scope search stands in results", [pane.searchMode, pane.searchQuery, pane.searchHere], ["results", "a", true])
            root.walkTag()
            return true
        },
        function () {
            if (!root.tagWalked()) return false
            root.tagChecks("tag-results", 1)
            // A direct call, because a key press spins the event loop and the folder could land before the checks.
            Search.abort(pane)
            check("control: the exit is out, the folder not yet landed", pane.searchLeaving, true)
            Filter.start(pane)
            Filter.typed(pane, "a")
            check("control: a filter stands while the exit is out", pane.filterQuery, "a")
            root.walkTag()
            check("tag-leaving: the filter is not closed for a search the exit refuses", [pane.filterTyping, pane.filterQuery], [true, "a"])
            check("tag-leaving: no query is sent", pane.searchGen - root.tagGenBefore, 0)
            return true
        },
        function () {
            if (!ready(fixture)) return false
            Filter.close(pane)
            root.pick("a.txt")
            pane.trash.open()
            return true
        },
        function () {
            var trash = pane.trash.item
            if (!trash || !trash.opened || trash.busy) return false
            trash.forceActiveFocus()
            driver.keyClick(Qt.Key_Question, Qt.NoModifier, -1)
            var s = root.sheet()
            check("control: question key opens sheet in Trash", s && s.opened, true)
            if (!s || !s.opened) return true
            s.query = "rename"
            var rows = s.queryResults
            var at = rows.findIndex(function(r) { return r.action === "rename" })
            check("trash-sheet: filesystem Rename is not offered over Trash", at >= 0, false)
            if (at >= 0) { s.resultCursor = at; s.activateResult() }
            else s.close()
            return true
        },
        function () {
            if (pane.menuActions.pendingAction.length > 0) return false
            check("trash-sheet: query never opens a covered filesystem editor", pane.renamingIndex, -1)
            pane.renamingIndex = -1
            pane.contextMenu().close()
            pane.trash.close()
            return true
        },
        function () {
            if (!ready(fixture)) return false
            pane.open(Quickshell.env("HOME"))
            return true
        },
        function () {
            if (!ready(Quickshell.env("HOME"))) return false
            Flea.ViewState.changeLeaf("places", {autoHide: true})
            return true
        },
        function () {
            var rail = root.find(pane, "PaneRail")
            if (!rail) throw new Error("No real PaneRail")
            // mouseMove takes delay before buttons and modifiers.
            check("autohide-click: six-argument mouseMove reaches the real pane",
                  driver.mouseMove(pane, 1, 80, 1, Qt.NoButton, Qt.NoModifier), true)
            return true
        },
        function () {
            if (!pane.sidebar) return false
            root.pick("readonly")
            root.cursorBeforeRail = pane.cursorRow.n
            var home = pane.sidebar.railItemFor(0)
            root.click(home.labelItem, 10, home.labelItem.height / 2)
            return true
        },
        function () {
            if (pane.listInFlight) return false
            check("autohide-click: rail consumes its click without selecting the covered row", pane.cursorRow.n, root.cursorBeforeRail)
            Flea.ViewState.changeLeaf("places", {autoHide: false})
            pane.open(fixture)
            return true
        },
        function () {
            if (!ready(fixture) || !pane.sidebar) return false
            var rail = pane.sidebar
            pane.focusView = "list"
            root.railHome = rail.entries.findIndex(function (e) { return e.kind === "home" })
            root.railRecent = rail.entries.findIndex(function (e) { return e.kind === "recent" })
            root.railTrash = rail.entries.findIndex(function (e) { return e.kind === "trash" })
            check("lit-place: the rail lights Home in a folder under it", rail.litIndex, root.railHome)
            check("lit-place: the rail rows draw the lit look on Home only, and the IPC Trash row is not current", [root.railLit(), root.ipc(true).trashRail().current], [[root.railHome], false])
            check("lit-place: an unfocused rail draws no cursor frame", root.railIndices("frame"), [])
            rail.activate(root.railRecent)
            check("lit-place: the request moves the cursor and not the lit place", [rail.cursorIndex, rail.litIndex], [root.railRecent, root.railHome])
            return true
        },
        function () {
            if (pane.recentMode !== "results" || pane.listInFlight) return false
            var rail = pane.sidebar
            check("lit-place: the landed Recent listing lights the Recent row", rail.litIndex, root.railRecent)
            check("lit-place: the rows follow the lit place", root.railLit(), [root.railRecent])
            return PaletteFlow.litPlace(root, pane, Flea.Theme, Style, Commons.Color, driver)
        },
        function () {
            if (!ready(fixture)) return false
            check("lit-place: leaving Recent lights the folder's row again", pane.sidebar.litIndex, root.railHome)
            pane.trash.open()
            return true
        },
        function () {
            if (!pane.trash.opened) return false
            check("lit-place: an open Trash lights the Trash row alone, and the IPC reads it as current with the list holding focus", [pane.sidebar.litIndex, root.railLit(), root.ipc(true).trashRail().current], [root.railTrash, [root.railTrash], true])
            pane.sidebar.cursorIndex = root.railHome
            pane.focusView = "rail"
            check("lit-place: an open Trash with the rail cursor off it is still current over the IPC", [root.railIndices("frame"), root.railLit(), root.ipc(true).trashRail().current], [[], [root.railHome], true])
            pane.focusView = "list"
            pane.trash.close()
            return true
        },
        function () {
            if (!ready(fixture)) return false
            check("lit-place: closing Trash lights the folder's row again", pane.sidebar.litIndex, root.railHome)
            pane.listArea.forceActiveFocus()
            return true
        },
        function () {
            if (!ready(fixture)) return false
            pane.clearSelection()
            root.clickName("a.txt")
            root.clickedAt = Date.now()
            return true
        },
        function () {
            if (Date.now() - root.clickedAt <= Qt.styleHints.mouseDoubleClickInterval + 100) return false
            root.clickName("a.txt")
            root.clickedAt = Date.now()
            return true
        },
        function () {
            if (Date.now() - root.clickedAt <= Qt.styleHints.mouseDoubleClickInterval + 100) return false
            check("slow-click: second name click starts rename", pane.renamingIndex, root.indexOf("a.txt"))
            pane.renamingIndex = -1
            pane.cancelSlowClick()
            var row = pane.visibleItemFor(root.indexOf("a.txt"))
            root.click(row, row.width - 20, row.height / 2)
            root.clickedAt = Date.now()
            return true
        },
        function () {
            if (Date.now() - root.clickedAt <= Qt.styleHints.mouseDoubleClickInterval + 100) return false
            var row = pane.visibleItemFor(root.indexOf("a.txt"))
            root.click(row, row.width - 20, row.height / 2)
            root.clickedAt = Date.now()
            return true
        },
        function () {
            if (Date.now() - root.clickedAt <= Qt.styleHints.mouseDoubleClickInterval + 100) return false
            check("slow-click-name-only: metadata clicks do not start rename", pane.renamingIndex, -1)
            pane.renamingIndex = -1
            pane.cancelSlowClick()
            return true
        },
        function () {
            root.pick("a.txt")
            root.press(Qt.Key_R)
            return true
        },
        function () {
            if (!pane.renameEditor()) return false
            check("rename-first-try: editor input owns focus", pane.renameEditor().editorField.inputItem.activeFocus, true)
            driver.keyClick(Qt.Key_A, Qt.ControlModifier, -1)
            var text = "renamed.txt"
            for (var i = 0; i < text.length; i++) driver.keyClickChar(text.charAt(i), Qt.NoModifier, -1)
            root.clickName("b.txt")
            return true
        },
        function () {
            if (pane.renamePending || pane.listInFlight || root.indexOf("renamed.txt") < 0) return false
            check("rename-click-away: committed name lands", root.indexOf("a.txt"), -1)
            check("rename-click-away: clicked row keeps the cursor", pane.cursorRow.n, "b.txt")
            check("rename-click-away: clicked row keeps its mark", pane.selectedIndices(), [root.indexOf("b.txt")])
            return true
        },
        function () {
            pane.clearSelection(); root.pick("added.txt")
            pane.toggleSelectAt(root.indexOf("link.txt"))
            root.press(Qt.Key_Up, Qt.ShiftModifier)
            check("shift-range: extend keeps earlier block", pane.selectedIndices(), [1, 2, 3])
            root.press(Qt.Key_Down, Qt.ShiftModifier)
            check("shift-range: shrinking keeps earlier block", pane.selectedIndices(), [1, 3])
            Flea.ViewState.changeKey("escapeUp", true)
            pane.message("Probe error", true)
            root.press(Qt.Key_Escape)
            check("escape-up: error closes before climbing", pane.path, fixture)
            root.press(Qt.Key_Escape)
            check("escape-up: marks clear before climbing", pane.selectionCount(), 0)
            root.press(Qt.Key_Escape)
            return true
        },
        function () {
            if (!ready(Quickshell.env("HOME"))) return false
            check("escape-up: idle Escape climbs", pane.path, Quickshell.env("HOME"))
            Flea.ViewState.changeKey("openMode", "single")
            pane.chooseView("grid")
            pane.open(fixture)
            return true
        },
        function () {
            if (!ready(fixture) || pane.viewMode !== "grid") return false
            root.clickName("sub")
            return true
        },
        function () {
            if (pane.listInFlight) return false
            check("single-click: one grid tap opens its folder", pane.path, fixture + "/sub")
            return true
        }
    ].concat(Extra.steps(root, pane, Flea.ViewState, Flea.Theme), [
        function () {
            root.trashCountWas = (Flea.ViewState.state.places || {}).trashCount === true
            root.driveSizeWas = (Flea.ViewState.state.places || {}).driveSize === true
            Flea.ViewState.changeLeaf("places", { trashCount: true, driveSize: false })
            return true
        },
        function () {
            var waiting = Extra.railBoardWaiting(railBoardRows, railBoard.width)
            if (waiting !== Extra.FIRST_SIZE_ROW && Date.now() - root.started < root.settleGraceMs) return false
            check("rail-board: before the sizes land the board waits on a drive size", waiting, Extra.FIRST_SIZE_ROW)
            check("rail-board: before the sizes land the first drive size row draws no detail", Extra.firstSizeDetail(railBoardRows), "")
            lateSizes.start()
            return true
        },
        function () {
            var waiting = probeNumber.detail !== "3" ? "the number probe" : Extra.railBoardWaiting(railBoardRows, railBoard.width)
            if (waiting !== "" && Date.now() - root.started < root.settleGraceMs) return false
            check("rail-board: no row still waits on its width or detail", waiting, "")
            if (waiting === "") Extra.railBoardChecks(root, railBoardRows, Style.spacing.rowPaddingX, Style.spacing.rowGap, Flea.Theme.font.caption, Flea.Theme.railIconSize)
            var gap = Style.spacing.rowGap
            check("control: the square probe reserves its slot", probeSquare.indicatorSlot.width > 0, true)
            check("control: the mark probe reserves its slot", probeMark.indicatorSlot.width > 0, true)
            check("control: the empty probe reserves no slot", probeEmpty.indicatorSlot.width, 0)
            check("rail-gap: a square row keeps one gap before the slot", root.railLabelGap(probeSquare), gap)
            check("rail-gap: a mark row keeps one gap before the slot", root.railLabelGap(probeMark), gap)
            check("rail-gap: a number row keeps one gap before its number", root.railLabelGap(probeNumber), gap)
            check("rail-gap: an empty row keeps no gap", root.railLabelGap(probeEmpty), 0)
            probeSquare.renaming = true
            return true
        },
        function () {
            if (!probeSquare.editorShown) return false
            var editor = probeSquare.editorField.parent
            check("rail-gap: the rename editor keeps one gap on a square row",
                probeSquare.indicatorSlot.x - (editor.x + editor.width), Style.spacing.rowGap)
            probeSquare.renaming = false
            Flea.ViewState.changeLeaf("places", { trashCount: root.trashCountWas, driveSize: root.driveSizeWas })
            return true
        }
    ])
    function reloadChecks(mode) {
        if (pane.viewMode !== mode || !ready(fixture)) return false
        if (root.reloadStage === 0) {
            root.pick("a.txt")
            root.beforeLists = pane.backend.listRequests
            root.reloadNotice = { from: -1, total: pane.total }
            root.press(Qt.Key_F5)
            check("reload-F5-" + mode + ": sends one re-list", pane.backend.listRequests - root.beforeLists, 1)
            root.reloadStage = 1
            return false
        }
        if (root.reloadStage === 1) {
            check("reload-notice-F5-" + mode + ": arms changed-row notice", root.reloadNotice.from, root.reloadNotice.total)
            check("reload-complete-F5-" + mode + ": spends its notice", pane.reloadFrom, -1)
            check("reload-anchor-" + mode + ": keeps the cursor name", pane.cursorRow.n, "a.txt")
            if (mode === "list") check("reload-changed-count: added row is reported", root.observedMessages.indexOf("Reloaded · 1 row changed") >= 0, true)
            root.beforeLists = pane.backend.listRequests
            root.reloadNotice = { from: -1, total: pane.total }
            root.press(Qt.Key_R, Qt.ControlModifier)
            check("reload-CtrlR-" + mode + ": sends one re-list", pane.backend.listRequests - root.beforeLists, 1)
            root.reloadStage = 2
            return false
        }
        check("reload-notice-CtrlR-" + mode + ": arms changed-row notice", root.reloadNotice.from, root.reloadNotice.total)
        check("reload-complete-CtrlR-" + mode + ": spends its notice", pane.reloadFrom, -1)
        root.reloadStage = 0
        return true
    }
    Timer {
        interval: 100
        repeat: true
        running: true
        onTriggered: {
            if (root.executing) return
            if (root.stage >= root.steps.length) {
                stop()
                console.log("SIDEBAR_FLOWS DONE checks=" + root.checks + " failed=" + root.failures)
                body.quitBackends()
                return
            }
            if (!root.started) root.started = Date.now()
            root.executing = true
            var done = false
            try { done = root.steps[root.stage]() }
            catch (error) { root.check("step " + root.stage + " threw", String(error), "no exception"); done = true }
            root.executing = false
            if (!done && Date.now() - root.started > 7000) {
                root.check("step " + root.stage + " completed", false, true)
                done = true
            }
            if (done) { root.stage++; root.started = 0 }
        }
    }
}
