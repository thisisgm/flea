import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import "flea/js/History.js" as History
import "flea/js/Sort.js" as Sort
import "flea/js/Tabs.js" as Tabs

// A tab whose Back still owes d450 comes back in another order, so its cursor waits for the re-sorted
// rows; tests/nav-history.sh sort holds those rows 1.5 s in the real backend's reply stream. A filter
// the operator opens meanwhile is newer intent than the owed row, and the rows must not override it.
// The tab descriptor is set up through Tabs.snapshot and History.entry; the hop is Tabs.selectAt, so
// the production Tabs.apply same-path re-sort and the different-path applyPending/defer route both run.
ShellRoot {
    id: root
    readonly property string base: Quickshell.env("NAV_HISTORY_BASE")
    readonly property string rowsGate: Quickshell.env("NAV_HISTORY_ROWS_GATE")
    readonly property string receipt: Quickshell.env("NAV_HISTORY_RECEIPT")
    readonly property var body: loader.item
    readonly property var pane: body ? body.currentPane : null
    // d450 is row 450 by name ascending, outside the 321 rows a listing holds, and row 199 descending, inside them.
    readonly property int owedDesc: 649 - 450
    property int checks: 0
    property int failures: 0
    property int stepIndex: 0
    property real stepStarted: 0
    property bool inStep: false
    property bool done: false
    property string waiting: ""
    property string sig: ""
    property real stableSince: 0
    property int rowsSeen: 0
    property int rowsAtMark: 0
    property int mark: 0
    property int shots: 0
    property int pendingShots: 0
    property var old: null
    property var before: null

    function check(label, actual, expected) {
        root.checks++
        if (JSON.stringify(actual) === JSON.stringify(expected)) { console.log("ok   " + label); return }
        root.failures++
        console.log("FAIL " + label + ": got " + JSON.stringify(actual) + ", expected " + JSON.stringify(expected))
    }
    function at(name) { return root.base + "/" + name }
    function descName(index) { return "d" + String(649 - index).padStart(3, "0") }
    function cursorName() { var r = root.pane.rowFor(root.pane.cursorIndex); return r ? r.n : null }
    function viewY() { return Math.round(root.pane.listArea.contentY) }
    function signature() {
        var p = root.pane
        return p ? [p.path, p.cursorIndex, p.listInFlight, p.listingState, p.held, p.rows.length, p.total, root.viewY(),
                    p.filterQuery, p.filterTyping, p.backend.heldListing, p.swap.holding].join("|") : ""
    }
    function settled(path) {
        var p = root.pane
        if (!p || p.path !== path || p.listInFlight || p.swap.holding || p.listingState !== "ready") return false
        var item = p.visibleItemFor(p.cursorIndex)
        return !!p.rowFor(p.cursorIndex) && !!item && !!item.row && Date.now() - root.stableSince >= 500
    }
    // The cursor's own delegate and every lit delegate on screen around it, in window pixels.
    function drawn() {
        var p = root.pane, item = p.visibleItemFor(p.cursorIndex), list = win.itemRect(p.listArea)
        var rect = item ? win.itemRect(item) : null, lit = 0
        for (var i = Math.max(0, p.cursorIndex - 80); i < Math.min(p.total, p.cursorIndex + 80); i++) {
            var other = p.visibleItemFor(i)
            if (!other || !other.visible || other.cursor !== true) continue
            var r = win.itemRect(other)
            if (r.y + r.height > list.y && r.y < list.y + list.height) lit++
        }
        return { name: item && item.row ? item.row.n : null, cursor: !!item && item.visible && item.cursor === true, lit: lit,
                 inside: !!rect && rect.height > 0 && rect.y >= list.y - 0.5 && rect.y + rect.height <= list.y + list.height + 0.5,
                 seen: !!rect && rect.y + rect.height > list.y && rect.y < list.y + list.height, rect: rect, list: list }
    }
    function box(r) { return r ? [Math.round(r.x), Math.round(r.y), Math.round(r.width), Math.round(r.height)].join(",") : "none" }
    function shot(label) {
        var name = "sort-" + String(++root.shots).padStart(2, "0") + "-" + label.replace(/[^A-Za-z0-9]+/g, "-").toLowerCase().substring(0, 60)
        root.pendingShots++
        root.body.grabToImage(function (result) {
            if (!result.saveToFile(Quickshell.env("NAV_HISTORY_SHOTS") + "/" + name + ".png")) root.check("screenshot " + name + " saves", false, true)
            root.pendingShots--
        })
        return name
    }
    function geom(label) {
        var p = root.pane, s = root.drawn()
        console.log("geom " + label + " index=" + p.cursorIndex + " name=" + s.name + " row=" + root.box(s.rect) + " list=" + root.box(s.list)
                    + " contentY=" + root.viewY() + " window=" + win.width + "x" + win.height + " shot=" + root.shot(label))
    }
    function requests() {
        requestLog.reload()
        requestLog.waitForJob()
        return requestLog.text().split("\n").filter(function (l) { return l.length > 0 }).map(function (l) { return JSON.parse(l) })
    }
    function since() { return root.requests().slice(root.mark) }
    // The descending sort went out after the hop, and a window behind it asks for the reordered rows.
    function sortAsked() {
        var list = root.since(), s = -1
        for (var i = 0; i < list.length; i++) if (list[i].c === "sort" && list[i].desc === true) s = i
        return s >= 0 && list.slice(s + 1).some(function (r) { return r.c === "window" && (r.count || 0) <= root.pane.windowSize })
    }
    function waitingOwed() { var w = root.pane.historyRestore; return !!w && w.waiting === true && !!w.entry && w.entry.name === "d450" }
    function sh(script, args) { shell.command = ["sh", "-c", script, "sh"].concat(args || []); shell.running = true }

    function act(fn) { return function () { fn(); return true } }
    function until(label, cond) { return function () { root.waiting = label; return cond() } }
    function settle(path) { return root.until("the pane settling in " + path, function () { return root.settled(path) }) }
    function shell_(script, args) { return [root.act(function () { root.sh(script, args) }), root.until("the fixture change", function () { return !shell.running })] }

    // One hop back to a tab that owes d450 in descending order, its re-sorted rows held. deferred starts
    // the pane in d450, so the tab lists base by name first and Tabs.applyPending sorts it.
    function owedHop(label, deferred, query, after) {
        var steps = deferred ? [root.act(function () { root.pane.open(root.at("d450")) }), root.settle(root.at("d450"))] : []
        return steps.concat(root.shell_(": > \"$1\"", [root.rowsGate]), [
            root.act(function () {
                var here = Tabs.snapshot(root.pane)
                var owes = Object.assign({}, deferred ? Tabs.snapshot(root.pane, root.base) : here,
                                         { sortBy: "name", sortDesc: true, cursorIndex: 0, cursorRestore: History.entry(root.base, "d450", 450) })
                root.check(label + ": setup: the pane lists by name ascending", [root.pane.backend.sortBy, root.pane.backend.sortDesc], ["name", false])
                root.pane.tabs = Tabs.pack([owes, here], 1)
                root.mark = root.requests().length
                root.rowsAtMark = root.rowsSeen
                Tabs.selectAt(root.pane, 0)
            }),
            root.until("the re-sort going out with its rows held and the owed d450 waiting", function () {
                var p = root.pane
                return p.path === root.base && !p.listInFlight && root.rowsSeen === root.rowsAtMark + (deferred ? 1 : 0)
                    && root.sortAsked() && root.waitingOwed()
            }),
            root.act(function () {
                root.old = { listing: root.pane.backend.heldListing, rows: root.rowsSeen }
                root.check(label + ": before the answer the restore waits for d450 over the old numbering", root.waitingOwed(), true)
                if (query === null) return
                root.pane.forceActiveFocus()
                var typed = "/" + query
                typed.split("").forEach(function (c) { keys.keyClickChar(c, Qt.NoModifier, -1) })
            }),
            root.until("the window holding still after the operator", function () { return root.rowsSeen > root.old.rows || Date.now() - root.stableSince >= 200 }),
            root.act(function () {
                var p = root.pane
                root.before = { index: p.cursorIndex, y: root.viewY(), early: root.rowsSeen > root.old.rows || p.backend.heldListing !== root.old.listing }
                if (query !== null)
                    root.check(label + ": the filter is open with " + JSON.stringify(query) + " before the re-sorted rows land",
                               [p.filterTyping, p.filterQuery, root.before.early], [true, query, false])
            }),
            root.until("the held re-sorted rows landing in a new numbering", function () {
                return root.rowsSeen > root.old.rows && root.pane.backend.heldListing !== root.old.listing
            })],
            root.shell_("rm -f -- \"$1\"", [root.rowsGate]),
            [root.settle(root.base), root.act(function () { after(label) }),
             // The filter stays drawn until its screenshot is on disk; only then is it closed for the next case.
             root.until("the screenshot being written", function () { return root.pendingShots === 0 }),
             root.act(function () { if (query !== null) root.closeFilter(label) })])
    }
    function restoredOwed(label) {
        var p = root.pane, s = root.drawn()
        root.check(label + ": the cursor is the owed d450 at its descending row " + root.owedDesc, [p.cursorIndex, root.cursorName()], [root.owedDesc, "d450"])
        root.check(label + ": d450 is drawn lit, wholly inside the list, the one lit row", [s.name, s.cursor, s.inside, s.lit], ["d450", true, true, 1])
        root.check(label + ": nothing is owed any more", p.historyRestore, null)
        root.geom(label)
    }
    function filterKept(query) {
        return function (label) {
            var p = root.pane, s = root.drawn(), b = root.before
            root.check(label + ": the operator's cursor and scroll were measured before the answer", b.early, false)
            root.check(label + ": the cursor and the scroll are where the operator left them", [p.cursorIndex, root.viewY()], [b.index, b.y])
            root.check(label + ": the owed d450 is not taken", [root.cursorName() !== "d450", p.cursorIndex !== root.owedDesc], [true, true])
            root.check(label + ": the late rows drop the owed restore", p.historyRestore, null)
            root.check(label + ": the filter keeps its line and its query", [p.filterTyping, p.filterQuery], [true, query])
            root.check(label + ": the cursor's delegate is the reversed row there, lit, on screen, the one lit row",
                       [s.name, s.cursor, s.seen, s.lit], [root.descName(b.index), true, true, 1])
            root.geom(label)
        }
    }
    function closeFilter(label) {
        var p = root.pane
        keys.keyClick(Qt.Key_Escape, Qt.NoModifier, -1)
        root.check(label + ": Escape closes the filter for the cases after it", [p.filterQuery, p.filterTyping], ["", false])
    }
    // Back to one tab listing by name ascending, the cursor on row 0, for the next case.
    function reset() {
        return [root.act(function () { if (Tabs.count(root.pane) > 1) Tabs.closeAt(root.pane, 1); Sort.resort(root.pane, "name", false); root.pane.setCursor(0) }),
                root.until("the base listed by name ascending again", function () {
                    var p = root.pane
                    return root.settled(root.base) && Tabs.count(p) === 1 && !p.backend.sortDesc && root.cursorName() === "d000" && p.historyRestore === null
                })]
    }

    readonly property var steps: [].concat(
        [root.until("the window listing the base", function () { return root.settled(root.base) && root.pane.total === 650 }),
         root.act(function () {
             root.check("premise: d450 by name ascending is outside the first window the listing holds", 450 >= root.pane.windowSize, true)
             root.check("premise: d450 by name descending is inside it", root.owedDesc < root.pane.windowSize, true)
         })],
        root.owedHop("same-folder tab re-sort, no filter", false, null, root.restoredOwed), root.reset(),
        root.owedHop("same-folder tab re-sort, empty filter strip", false, "", root.filterKept("")), root.reset(),
        root.owedHop("same-folder tab re-sort, filter d", false, "d", root.filterKept("d")), root.reset(),
        root.owedHop("other-folder tab sorted on arrival, no filter", true, null, root.restoredOwed), root.reset(),
        root.owedHop("other-folder tab sorted on arrival, filter d", true, "d", root.filterKept("d")), root.reset(),
        [root.until("the screenshots being written", function () { return root.pendingShots === 0 })]
    )

    function finish(complete) {
        if (root.done) return
        root.done = true
        var ok = complete && root.failures === 0
        console.log("nav-history " + root.receipt + ": " + (ok ? "PASS " : "FAIL ") + root.checks + " checks, " + root.failures + " failed"
                    + (complete ? "" : ", stopped at step " + root.stepIndex))
        if (root.body) root.body.quitBackends()
        else Quickshell.execDetached(["kill", String(Quickshell.processId)])
    }

    Connections {
        target: root.pane ? root.pane.backend : null
        function onRows(start, items, ms, kinds, listing) { root.rowsSeen++ }
    }
    FileView { id: requestLog; path: Quickshell.env("NAV_HISTORY_REQUESTS"); blockLoading: true; watchChanges: false; printErrors: false }
    Process { id: shell }

    FloatingWindow {
        id: win
        implicitWidth: 900
        implicitHeight: 600
        color: "#101315"
        Loader {
            id: loader
            anchors.fill: parent
            focus: true
            onStatusChanged: if (status === Loader.Error) { console.log("FAIL the window body did not load"); root.failures++; root.finish(false) }
        }
        Item { anchors.fill: parent; TestEvent { id: keys } }
        Component.onCompleted: loader.setSource("file://" + Quickshell.shellDir + "/flea/WindowBody.qml", { host: win })
    }

    Timer {
        interval: 10
        repeat: true
        running: !root.done && loader.status === Loader.Ready
        onTriggered: {
            var now = Date.now(), next = root.signature()
            if (next !== root.sig) { root.sig = next; root.stableSince = now }
            if (root.stepStarted === 0) root.stepStarted = now
            // TestEvent delivers through a nested event loop, which fires this timer again mid-step; it waits.
            if (root.inStep) return
            root.inStep = true
            var stepDone = false
            try {
                stepDone = root.steps[root.stepIndex]()
            } catch (error) {
                root.failures++
                console.log("FAIL step " + root.stepIndex + " threw: " + error)
                root.inStep = false
                root.finish(false)
                return
            }
            root.inStep = false
            if (stepDone) {
                root.stepIndex++
                root.stepStarted = 0
                root.waiting = ""
                if (root.stepIndex >= root.steps.length) root.finish(true)
            } else if (now - root.stepStarted > 20000) {
                root.failures++
                console.log("FAIL step " + root.stepIndex + " never completed: waited 20 s for " + root.waiting
                            + " (path " + root.pane.path + ", cursor " + root.pane.cursorIndex + " " + root.cursorName() + ")")
                root.finish(false)
            }
        }
    }
}
