//@ pragma ShellId flea-picker-header-test

import QtQuick
import Quickshell
import "flea" as Flea
import "flea/js/Picker.js" as Picker

// The real picker window: grid draws no header, tiles start one gap under the strip; a view round trip keeps the sort and cursor.
ShellRoot {
    id: root

    property var failures: []
    property int stage: 0
    property double stageSince: 0
    property var winShell: null
    property var win: null
    property var keys: null
    property bool reported: false
    // A cursor row other than 0, which a reset to the top would also produce.
    readonly property int cursorProbe: 3
    readonly property string probeSort: "size"
    readonly property int waitMs: 5000
    readonly property int probeTimeoutMs: 30000
    readonly property real geometryTolerance: 0.5
    readonly property real inlineTolerance: 1
    readonly property int boardStripHeight: 27
    readonly property real centreEpsilon: 0.01
    // Far wider than any window: 40 directories of 14 characters.
    readonly property string longParents: "/" + new Array(41).join("directory-name/")
    readonly property string longLeaf: new Array(60).join("a-very-long-file-name-")
    readonly property string specialPath: "/tmp/space name/#[draft]%/雪"

    function fail(text) { root.failures.push(text) }
    function late() { return Date.now() - root.stageSince > root.waitMs }
    function go(stage) { root.stage = stage; root.stageSince = Date.now() }

    function findType(item, name) {
        if (String(item).indexOf(name + "_QMLTYPE") === 0) return item
        for (var i = 0; i < item.children.length; i++) {
            var hit = root.findType(item.children[i], name)
            if (hit) return hit
        }
        return null
    }
    function under(item, ancestor) {
        for (var at = item; at; at = at.parent) if (at === ancestor) return true
        return false
    }
    function findName(item, name) {
        if (item.objectName === name) return item
        for (var i = 0; i < item.children.length; i++) {
            var hit = root.findName(item.children[i], name)
            if (hit) return hit
        }
        return null
    }
    function header() { return root.findType(root.win.contentItem, "PickerHeader") }
    function pathStrip(chrome) {
        var strip = root.findName(chrome, "pickerStrip")
        if (strip) strip.forceLayout()
        return strip
    }

    // The strip's recipe from Picker040 at base size 14: board height and the path one gap after Back and Up.
    function checkGeometry(tag, chrome, strip) {
        var where = root.findName(chrome, "pickerWhere")
        var back = root.findName(chrome, "pickerBack")
        if (!where || !back) return false
        if (where.height !== root.boardStripHeight) root.fail(tag + ": the strip is " + where.height + " px tall, want the board's " + root.boardStripHeight)
        if (where.height !== Flea.Theme.chromeHeight) root.fail(tag + ": the strip is " + where.height + " px tall, want the chrome height " + Flea.Theme.chromeHeight)
        var start = strip.mapToItem(where, 0, 0).x
        var want = Flea.Theme.spacing.rowPaddingX + 2 * back.width + 2 * Flea.Theme.spacing.gap
        if (Math.abs(start - want) > root.inlineTolerance) root.fail(tag + ": the path starts at x " + start + ", want " + want + " after Back and Up")
        var top = strip.mapToItem(where, 0, 0).y
        var wantTop = Math.round(where.height / 2) - Math.round(strip.height / 2) - Flea.Theme.spacing.hairline / 2
        if (Math.abs(top - wantTop) > root.centreEpsilon) root.fail(tag + ": the path top is at y " + top + ", want " + wantTop + " in the band above the rule")
        return true
    }
    function crumbs(strip) { return strip && strip.crumbItems ? strip.crumbItems : null }
    function drawn(strip) {
        var items = crumbs(strip), text = ""
        if (!items) return text
        for (var i = 0; i < items.count; i++) text += items.itemAt(i).text
        return text
    }
    // The path strip keeps the root and leaf as distinct crumbs over /winprobe.
    function checkStrip(tag) {
        var chrome = root.findType(root.win.contentItem, "PickerChrome")
        if (!chrome) return false
        var strip = root.pathStrip(chrome)
        if (!strip || strip.width <= 0 || !strip.visible) return false
        var items = crumbs(strip)
        if (!items || items.count !== 2) return false
        if (drawn(strip) !== "/winprobe") root.fail(tag + ": drawn segments " + drawn(strip) + ", want /winprobe")
        if (items.itemAt(0).modelData.path !== "/") root.fail(tag + ": root target is " + items.itemAt(0).modelData.path)
        if (items.itemAt(1).modelData.path !== "/winprobe") root.fail(tag + ": leaf target is " + items.itemAt(1).modelData.path)
        if (!Qt.colorEqual(items.itemAt(0).color, Flea.Theme.color.muted)) root.fail(tag + ": root crumb is not muted")
        if (!Qt.colorEqual(items.itemAt(1).color, Flea.Theme.color.foreground)) root.fail(tag + ": leaf crumb is not foreground")
        if (!root.checkGeometry(tag, chrome, strip)) return false
        var up = root.findName(chrome, "pickerUp")
        var back = root.findName(chrome, "pickerBack")
        var list = root.findName(chrome, "pickerListView")
        var grid = root.findName(chrome, "pickerGridView")
        if (!up || !back || !list || !grid) return false
        if (!up.enabled) root.fail(tag + ": Up is disabled over /winprobe, want live")
        else if (!Qt.colorEqual(up.markItem.color, Flea.Theme.color.foreground))
            root.fail(tag + ": live Up ink " + up.markItem.color + ", want foreground")
        if (back.enabled) root.fail(tag + ": Back is live with an empty history, want disabled")
        else {
            if (!Qt.colorEqual(back.markItem.color, Flea.Theme.color.mutedFrame))
                root.fail(tag + ": dead Back ink " + back.markItem.color + ", want muted")
            if (Math.abs(back.markItem.opacity - Flea.Theme.disabledOpacity) > 0.001)
                root.fail(tag + ": dead Back opacity " + back.markItem.opacity + ", want " + Flea.Theme.disabledOpacity)
        }
        if (list.lit) root.fail(tag + ": the list mark is lit in grid mode, want unlit")
        else if (!Qt.colorEqual(list.markItem.color, Flea.Theme.color.mutedFrame))
            root.fail(tag + ": unlit list ink " + list.markItem.color + ", want muted")
        if (!grid.lit) root.fail(tag + ": the grid mark is unlit in grid mode, want lit")
        else if (!Qt.colorEqual(grid.markItem.color, Flea.Theme.color.foreground))
            root.fail(tag + ": lit grid ink " + grid.markItem.color + ", want foreground")
        return true
    }
    // A deeper path exposes each intermediate directory as its own target.
    function checkDeep(tag) {
        var chrome = root.findType(root.win.contentItem, "PickerChrome")
        if (!chrome) return false
        var strip = root.pathStrip(chrome)
        if (!strip || strip.width <= 0 || !strip.visible) return false
        if (strip.display !== "/a/b/winprobe") return false
        var items = crumbs(strip)
        if (!items || items.count !== 4) return false
        if (items.itemAt(1).modelData.path !== "/a" || items.itemAt(2).modelData.path !== "/a/b")
            root.fail(tag + ": intermediate targets are not /a and /a/b")
        return root.checkGeometry(tag, chrome, strip)
    }
    // A long path keeps the final destination even when whole middle segments collapse.
    function checkLongParents(tag) {
        var chrome = root.findType(root.win.contentItem, "PickerChrome")
        if (!chrome) return false
        var strip = root.pathStrip(chrome)
        if (!strip || strip.width <= 0 || !strip.visible) return false
        if (strip.display !== root.longParents + "winprobe") return false
        var items = crumbs(strip)
        if (!items || items.count < 2) return false
        if (items.itemAt(items.count - 1).modelData.path !== root.longParents + "winprobe")
            root.fail(tag + ": final segment lost its exact path target")
        return true
    }
    // Even a long leaf keeps its exact destination when its drawn label is shortened.
    function checkLongLeaf(tag) {
        var chrome = root.findType(root.win.contentItem, "PickerChrome")
        if (!chrome) return false
        var strip = root.pathStrip(chrome)
        if (!strip || strip.width <= 0 || !strip.visible) return false
        if (strip.display !== "/a/" + root.longLeaf) return false
        var items = crumbs(strip)
        if (!items || items.count < 2) return false
        if (items.itemAt(items.count - 1).modelData.path !== "/a/" + root.longLeaf)
            root.fail(tag + ": long leaf lost its exact destination")
        return true
    }
    // With a history, Back is live and rests in the foreground like the main chrome.
    function checkBackLit(tag) {
        var chrome = root.findType(root.win.contentItem, "PickerChrome")
        var back = chrome ? root.findName(chrome, "pickerBack") : null
        if (!back || root.win.history.length === 0) return false
        if (!back.enabled) root.fail(tag + ": Back is dead with a history of " + root.win.history.length)
        else if (!Qt.colorEqual(back.markItem.color, Flea.Theme.color.foreground))
            root.fail(tag + ": live Back ink " + back.markItem.color + ", want foreground")
        if (back.markItem.opacity !== 1) root.fail(tag + ": live Back opacity " + back.markItem.opacity + ", want 1")
        return true
    }
    // Recent is one lit word with no parents; false while the strip still shows a path.
    function checkRecent(tag) {
        var chrome = root.findType(root.win.contentItem, "PickerChrome")
        if (!chrome) return false
        var strip = root.pathStrip(chrome)
        if (!strip || strip.width <= 0 || !strip.visible) return false
        if (strip.display !== Picker.RECENT_LABEL) return false
        var items = crumbs(strip)
        if (!items || items.count !== 1 || items.itemAt(0).text !== Picker.RECENT_LABEL)
            root.fail(tag + ": Recent is not drawn as one label")
        return true
    }

    // Grid mode: no header, tile 0 one gap under the path strip, Tab reaches the grid; false while tile 0 is unbuilt.
    function checkGrid(tag) {
        var head = root.header()
        var chrome = root.findType(root.win.contentItem, "PickerChrome")
        var rail = root.findType(root.win.contentItem, "PickerPlaces").focusItem
        var gview = root.win.viewItem()
        // The cursor may have scrolled the grid, so measure tile 0 at the start.
        gview.contentY = gview.originY - gview.topMargin
        var tile = gview.itemAtIndex(0)
        if (!tile) return false
        if (head.visible) root.fail(tag + ": grid mode draws the column header")
        if (head.height !== 0) root.fail(tag + ": grid mode header takes height " + head.height + ", want 0")
        var tileTop = tile.mapToItem(root.win.contentItem, 0, 0).y
        var want = chrome.mapToItem(root.win.contentItem, 0, chrome.height).y + Flea.Theme.spacing.gap
        if (Math.abs(tileTop - want) > root.geometryTolerance)
            root.fail(tag + ": first tile top " + tileTop + ", want " + want + " (path strip bottom plus gap)")
        // Tab walks the picker's own chain from its first control, never a header control, and reaches the grid within one lap.
        var window = root.win.contentItem.Window.window
        var chain = chrome.focusItems().concat([rail, gview])
            .filter(function(item) { return item.visible && item.enabled && item.activeFocusOnTab })
        var reached = false
        chain[0].forceActiveFocus(Qt.TabFocusReason)
        if (root.under(window.activeFocusItem, gview)) root.fail(tag + ": the Tab walk began inside the grid")
        for (var step = 0; step < chain.length && !reached; step++) {
            var from = window.activeFocusItem
            // A rail row holds focus inside the rail, whose own Tab handler is the one that steps.
            if (root.under(from, rail)) from = rail
            root.win.stepFocus(from, false)
            var at = window.activeFocusItem
            if (at && root.under(at, head)) { root.fail(tag + ": Tab landed on the hidden header"); break }
            reached = at && root.under(at, gview)
        }
        if (!reached) root.fail(tag + ": Tab never reached the grid")
        return true
    }
    // List mode: the header at its implicit height, the rows starting directly under it; false while the cursor row is unbuilt.
    function checkList(tag) {
        var head = root.header()
        if (!root.win.viewItem().itemAtIndex(root.win.cursorIndex)) return false
        if (!head.visible) root.fail(tag + ": list mode lost the column header")
        if (head.implicitHeight <= 0 || Math.abs(head.height - head.implicitHeight) > root.geometryTolerance)
            root.fail(tag + ": list header height " + head.height + ", want implicit " + head.implicitHeight)
        // The list's own top is its first row's slot, whichever row the cursor scrolled to.
        var rowsTop = root.win.viewItem().mapToItem(root.win.contentItem, 0, 0).y
        var headBottom = head.mapToItem(root.win.contentItem, 0, head.height).y
        if (Math.abs(rowsTop - headBottom) > root.geometryTolerance)
            root.fail(tag + ": rows start at " + rowsTop + ", want the header bottom " + headBottom)
        return true
    }
    // The sort the header click set and the cursor set in grid, which a view switch must leave as they were.
    function checkKept(tag) {
        var head = root.header()
        if (head.sortBy !== root.probeSort || head.sortDesc !== false)
            root.fail(tag + ": the sort mark became " + head.sortBy + "/" + head.sortDesc + ", want " + root.probeSort + "/false")
        if (head.title("Size", "size") !== "Size \u25b4") root.fail(tag + ": the header draws " + head.title("Size", "size") + " for the sort")
        if (root.win.cursorIndex !== root.cursorProbe)
            root.fail(tag + ": the cursor moved to " + root.win.cursorIndex + ", want " + root.cursorProbe)
    }

    Timer {
        id: ticker
        interval: 10
        repeat: true
        running: true
        onTriggered: root.step()
    }
    Timer { interval: root.probeTimeoutMs; running: true; onTriggered: { root.fail("probe timed out in stage " + root.stage); root.report() } }

    function step() {
        if (root.failures.length > 0) { root.report(); return }
        switch (root.stage) {
        case 0: {
            var comp = Qt.createComponent("flea/PickerWindow.qml")
            if (comp.status !== Component.Ready) { root.fail("PickerWindow does not compile: " + comp.errorString()); return }
            root.winShell = comp.createObject(root)
            if (!root.winShell) { root.fail("PickerWindow did not instantiate: " + comp.errorString()); return }
            root.win = root.winShell.pickerWin
            root.keys = Qt.createQmlObject("import QtTest; TestEvent {}", root.win.contentItem)
            root.go(1)
            return
        }
        case 1: {
            // The window launches in grid, from the state file the suite seeds.
            if (root.win.viewMode !== "grid" || root.win.total === 0 || root.win.rows.length === 0) {
                if (root.late()) root.fail("the window never listed in grid mode")
                return
            }
            if (!root.checkGrid("grid")) { if (root.late()) root.fail("grid: tile 0 never built"); return }
            if (!root.checkStrip("strip")) { if (root.late()) root.fail("strip: the path strip never built"); return }
            root.win.setView("list")
            root.go(2)
            return
        }
        case 2: {
            if (root.win.viewMode !== "list") { root.fail("setView list never switched view"); return }
            if (!root.checkList("list")) { if (root.late()) root.fail("list: the cursor row never built"); return }
            // The header's own click path: it asks the window for the order, which re-lists.
            root.header().sortRequested(root.probeSort)
            root.go(5)
            return
        }
        case 5: {
            if (root.win.pendingListings !== 0 || root.win.total === 0 || root.win.rows.length === 0) {
                if (root.late()) root.fail("the sorted listing never arrived")
                return
            }
            root.win.setView("grid")
            root.go(3)
            return
        }
        case 3: {
            if (root.win.viewMode !== "grid") { root.fail("setView grid never switched view"); return }
            root.win.cursorIndex = root.cursorProbe
            if (!root.checkGrid("grid")) { if (root.late()) root.fail("grid: tile 0 never built"); return }
            root.checkKept("grid")
            root.win.setView("list")
            root.go(4)
            return
        }
        case 4: {
            if (root.win.viewMode !== "list") { root.fail("round trip never returned to list"); return }
            if (!root.checkList("round trip list")) { if (root.late()) root.fail("round trip list: the cursor row never built"); return }
            root.checkKept("round trip list")
            root.win.setView("grid")
            root.go(6)
            return
        }
        case 6: {
            if (root.win.viewMode !== "grid") { root.fail("second setView grid never switched view"); return }
            if (!root.checkGrid("round trip grid")) { if (root.late()) root.fail("round trip grid: tile 0 never built"); return }
            root.checkKept("round trip grid")
            root.win.setView("list")
            root.go(7)
            return
        }
        case 7: {
            if (root.win.viewMode !== "list") { root.fail("second round trip never returned to list"); return }
            if (!root.checkList("second round trip list")) { if (root.late()) root.fail("second round trip list: the cursor row never built"); return }
            root.checkKept("second round trip list")
            root.win.openWithoutHistory("/a/b/winprobe")
            root.go(8)
            return
        }
        case 8: {
            if (!root.checkDeep("deep")) { if (root.late()) root.fail("deep: the strip never showed /a/b/winprobe"); return }
            var middle = root.crumbs(root.pathStrip(root.findType(root.win.contentItem, "PickerChrome"))).itemAt(1)
            root.keys.mouseClick(middle, middle.width / 2, middle.height / 2, Qt.LeftButton, Qt.NoModifier, -1)
            root.go(13)
            return
        }
        case 13: {
            if (root.win.path !== "/a") { if (root.late()) root.fail("clicking the /a segment did not navigate to /a"); return }
            if (!root.checkBackLit("crumb history")) { if (root.late()) root.fail("crumb navigation did not preserve picker history"); return }
            root.win.openWithoutHistory(root.specialPath)
            root.go(14)
            return
        }
        case 14: {
            var special = root.pathStrip(root.findType(root.win.contentItem, "PickerChrome"))
            var items = root.crumbs(special)
            if (!items || items.count !== 5 || items.itemAt(3).modelData.path !== "/tmp/space name/#[draft]%"
                || items.itemAt(4).modelData.path !== root.specialPath) {
                if (root.late()) root.fail("special-character path segments lost their exact navigation targets")
                return
            }
            root.win.openWithoutHistory(root.longParents + "winprobe")
            root.go(10)
            return
        }
        case 10: {
            if (!root.checkLongParents("long parents")) { if (root.late()) root.fail("long parents: the strip never showed the long path"); return }
            root.win.openWithoutHistory("/a/" + root.longLeaf)
            root.go(11)
            return
        }
        case 11: {
            if (!root.checkLongLeaf("long leaf")) { if (root.late()) root.fail("long leaf: the strip never showed the long name"); return }
            root.win.open("/a/b")
            root.go(12)
            return
        }
        case 12: {
            if (!root.checkBackLit("history")) { if (root.late()) root.fail("history: Back never had a history"); return }
            root.win.openWithoutHistory(Picker.RECENT)
            root.go(9)
            return
        }
        case 9: {
            if (!root.checkRecent("recent")) { if (root.late()) root.fail("recent: the strip never showed Recent"); return }
            root.report()
            return
        }
        }
    }

    function report() {
        // One report: the repeat timer is stopped and a late timeout cannot print a second verdict.
        if (root.reported) return
        root.reported = true
        ticker.stop()
        if (root.failures.length === 0) {
            console.log("PICKERHEADER PASS grid header hidden, tiles under the strip, list header restored")
        } else {
            for (var i = 0; i < root.failures.length; i++)
                console.log("PICKERHEADER FAIL " + root.failures[i])
        }
        Quickshell.execDetached(["kill", String(Quickshell.processId)])
    }
}
