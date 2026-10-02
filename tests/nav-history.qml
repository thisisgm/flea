import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import "flea/js/Tabs.js" as Tabs

// Back and Forward put the cursor back on the item the pane left, through the real ui/WindowBody.qml and
// the built backend; tests/nav-history.sh owns the fixture. The navigation under test is always the
// mouse's side buttons or the toolbar's Back pressed by TestEvent; setup uses the pane's own functions.
ShellRoot {
    id: root
    readonly property string base: Quickshell.env("NAV_HISTORY_BASE")
    readonly property string home: Quickshell.env("HOME")
    readonly property string gate: Quickshell.env("NAV_HISTORY_GATE")
    readonly property string receipt: Quickshell.env("NAV_HISTORY_RECEIPT")
    readonly property var body: loader.item
    readonly property var pane: body ? body.currentPane : null
    // Three folders sort in ahead of every dNNN once the run makes them, so a saved index alone points three rows early.
    readonly property int ahead: 3
    property var backButton: null
    property var chrome: null
    property int checks: 0
    property int failures: 0
    property int stepIndex: 0
    property real stepStarted: 0
    property bool inStep: false
    property bool done: false
    property string waiting: ""
    property string sig: ""
    property real stableSince: 0
    property int located: 0
    property int locatedAtMark: 0
    property int mark: 0
    property int shots: 0
    property int pendingShots: 0
    property int moved: -1
    property int filterY: 0
    property var before: null

    function check(label, actual, expected) {
        root.checks++
        if (JSON.stringify(actual) === JSON.stringify(expected)) { console.log("ok   " + label); return }
        root.failures++
        console.log("FAIL " + label + ": got " + JSON.stringify(actual) + ", expected " + JSON.stringify(expected))
    }
    // The view the pane's own rows are drawn in: the list, the grid, or the columns view's active column.
    function view() { var a = root.pane.listArea; return typeof a.activeColumn === "function" ? a.activeColumn() : a }
    function viewY() { var a = root.pane.listArea; return typeof a.activeContentY === "function" ? a.activeContentY() : a.contentY }
    function at(name) { return root.base + "/" + name }
    function cursorName() { var r = root.pane.rowFor(root.pane.cursorIndex); return r ? r.n : null }
    function signature() {
        var p = root.pane
        return p ? [p.path, p.cursorIndex, p.listInFlight, p.listingState, p.held, p.rows.length, p.total,
                    Math.round(root.viewY()), p.viewMode, p.swap.holding].join("|") : ""
    }
    // Landed, released, the row under the cursor loaded and drawn, and nothing moving for half a second.
    function settled(path) {
        var p = root.pane
        if (!p || p.path !== path || p.listInFlight || p.swap.holding) return false
        if (p.listingState === "ready") {
            var item = p.visibleItemFor(p.cursorIndex)
            if (!p.rowFor(p.cursorIndex) || !item || !item.row) return false
        } else if (p.listingState !== "empty") return false
        return Date.now() - root.stableSince >= 500
    }
    // What the window draws for the cursor: its delegate, where it sits against the list, and how many rows on screen are lit as the cursor.
    function drawn() {
        var p = root.pane, item = p.visibleItemFor(p.cursorIndex), list = win.itemRect(root.view())
        var rect = item ? win.itemRect(item) : null, lit = 0
        for (var i = Math.max(0, p.cursorIndex - 80); i < Math.min(p.total, p.cursorIndex + 80); i++) {
            var other = p.visibleItemFor(i)
            if (!other || !other.visible || other.cursor !== true) continue
            var r = win.itemRect(other)
            if (r.y + r.height > list.y && r.y < list.y + list.height) lit++
        }
        return { name: item && item.row ? item.row.n : null, cursor: !!item && item.visible && item.cursor === true, lit: lit,
                 inside: !!rect && rect.height > 0 && rect.y >= list.y - 0.5 && rect.y + rect.height <= list.y + list.height + 0.5,
                 rect: rect, list: list }
    }
    function box(r) { return r ? [Math.round(r.x), Math.round(r.y), Math.round(r.width), Math.round(r.height)].join(",") : "none" }
    function shot(label) {
        var name = String(++root.shots).padStart(2, "0") + "-" + label.replace(/[^A-Za-z0-9]+/g, "-").toLowerCase().substring(0, 60)
        root.pendingShots++
        root.body.grabToImage(function (result) {
            if (!result.saveToFile(Quickshell.env("NAV_HISTORY_SHOTS") + "/" + name + ".png")) root.check("screenshot " + name + " saves", false, true)
            root.pendingShots--
        })
        return name
    }
    function restored(label, path, name, index) {
        var p = root.pane, s = root.drawn()
        root.check(label + ": the pane is in " + path.substring(root.home.length), p.path, path)
        root.check(label + ": the cursor is on " + name, root.cursorName(), name)
        root.check(label + ": at its current index " + index, p.cursorIndex, index)
        root.check(label + ": that row is drawn lit, wholly inside the list, the one lit row", [s.name, s.cursor, s.inside, s.lit], [name, true, true, 1])
        console.log("geom " + label + " view=" + p.viewMode + " index=" + p.cursorIndex + " name=" + s.name + " row=" + root.box(s.rect) + " list=" + root.box(s.list)
                    + " contentY=" + Math.round(root.viewY()) + " window=" + win.width + "x" + win.height + " shot=" + root.shot(label))
    }
    function requests() {
        requestLog.reload()
        requestLog.waitForJob()
        return requestLog.text().split("\n").filter(function (l) { return l.length > 0 }).map(function (l) { return JSON.parse(l) })
    }
    function markRequests() { root.mark = root.requests().length; root.locatedAtMark = root.located }
    function since() { return root.requests().slice(root.mark) }
    function locates(list) { return list.filter(function (r) { return r.c === "locate" }) }
    // No request after the press may name more rows than one window, which is what a whole-directory read would.
    function viewportSized(label) {
        var size = root.pane.windowSize
        var wide = root.since().filter(function (r) {
            return Math.max(r.count || 0, r.first || 0, (r.rows || []).length, (r.paths || []).length) > size
        })
        root.check(label + ": every request after the press stays within one window of " + size + " rows", wide, [])
    }
    function side(button) { root.pane.forceActiveFocus(); keys.mouseClick(root.view(), 40, 40, button, Qt.NoModifier, -1); root.park() }
    function toolbarBack(label) {
        var b = root.backButton
        root.check(label + ": the toolbar Back is enabled", b.enabled, true)
        keys.mouseClick(b, b.width / 2, b.height / 2, Qt.LeftButton, Qt.NoModifier, -1)
        root.park()
    }
    function sh(script, args) { shell.command = ["sh", "-c", script, "sh"].concat(args || []); shell.running = true }

    // Step builders: act runs once, until waits, settle waits for the pane to land and hold still in path.
    function act(fn) { return function () { fn(); return true } }
    function until(label, cond) { return function () { root.waiting = label; return cond() } }
    function settle(path) { return root.until("the pane settling in " + path, function () { return root.settled(path) }) }
    function cursorAt(path, index) {
        return [root.act(function () { root.pane.setCursor(index) }),
                root.until("the cursor resting on row " + index + " in " + path, function () { return root.settled(path) && root.pane.cursorIndex === index })]
    }
    function enter(path, index, name) {
        return root.cursorAt(path, index).concat([
            root.act(function () { root.check("setup: the cursor is on " + name + " before opening it", root.cursorName(), name); root.pane.openCursor() }),
            root.settle(path + "/" + name)])
    }
    function shell_(script, args) { return [root.act(function () { root.sh(script, args) }), root.until("the fixture change", function () { return !shell.running })] }
    function press(what, path) {
        return [root.act(function () {
            root.markRequests()
            if (what === "toolbar") root.toolbarBack("toolbar Back into " + path)
            else root.side(what === "back" ? Qt.BackButton : Qt.ForwardButton)
        }), root.settle(path)]
    }
    // The side buttons land over the listing; the pointer then parks on the status bar so no row is drawn hovered.
    function park() { keys.mouseMove(root.body, 5, root.body.height - 5, -1, Qt.NoButton, Qt.NoModifier) }
    // Real wheel notches over the listing through ui/FastScrollHandler.qml; positive scrolls down.
    function wheel(notches) {
        var a = root.pane.listArea
        keys.mouseWheel(a, a.width / 2, a.height / 2, Qt.NoButton, Qt.NoModifier, 0, -120 * notches, -1)
        root.park()
    }
    // What the operator left, measured once the window holds still after their action and before the answer lands.
    function snap() {
        var s = root.drawn()
        return { index: root.pane.cursorIndex, name: s.name, row: root.box(s.rect), y: Math.round(root.viewY()), early: root.located > root.locatedAtMark }
    }
    // The operator's own cursor survived the late answer: same row, same drawn place, same scroll, lit alone, not d450.
    // A wheel may leave the cursor on a row the viewport edge cuts, so whole asks for wholly inside only where a key put it.
    function kept(label, whole) {
        var p = root.pane, s = root.drawn(), b = root.before
        var seen = !!s.rect && s.rect.y + s.rect.height > s.list.y && s.rect.y < s.list.y + s.list.height
        root.check(label + ": measured before the answer landed", b.early, false)
        root.check(label + ": the cursor, its name, its drawn row and the scroll are what the operator left", [p.path, p.cursorIndex, s.name, root.box(s.rect), Math.round(root.viewY())],
                   [root.base, root.moved, b.name, b.row, b.y])
        root.check(label + ": that row is the one lit, on screen" + (whole ? " wholly" : "") + ", and it is not d450",
                   [s.name !== "d450", s.cursor, whole ? s.inside : seen, s.lit], [true, true, true, 1])
        console.log("geom " + label + " index=" + p.cursorIndex + " name=" + s.name + " row=" + root.box(s.rect) + " before=" + b.row + " list=" + root.box(s.list)
                    + " contentY=" + Math.round(root.viewY()) + " shot=" + root.shot(label))
    }
    // From base with the cursor put back on off-window d450: Forward, then a Back whose located answer the
    // wrapper holds for 1.5 s. during runs once that locate is out and unanswered; after runs once it lands.
    function heldBack(during, after) {
        return root.cursorAt(root.base, 450 + root.ahead).concat(
            [root.act(function () { root.check("setup: the base cursor is on d450 before a held Back", root.cursorName(), "d450") })],
            root.press("forward", root.at("d450")),
            root.shell_(": > \"$1\"", [root.gate]),
            [root.act(function () { root.markRequests(); root.side(Qt.BackButton) }),
             root.until("Back's locate for d450 going out while its answer is held", function () {
                 var p = root.pane
                 return p.path === root.base && !p.listInFlight && p.rowFor(p.cursorIndex) !== null && root.located === root.locatedAtMark
                     && root.locates(root.since()).some(function (r) { return r.path === root.at("d450") })
             })],
            during,
            [root.until("the window holding still after the operator", function () { return root.located > root.locatedAtMark || Date.now() - root.stableSince >= 200 }),
             root.act(function () { root.before = root.snap() })],
            [root.until("the held answer arriving", function () { return root.located > root.locatedAtMark })],
            root.shell_("rm -f -- \"$1\"", [root.gate]),
            after)
    }
    // The grid and the columns view, chosen with the toolbar's own button: shallow and deep Back, by mouse and by toolbar.
    function viewBack(mode) {
        function done(what, name, index) {
            return [root.act(function () { root.restored(mode + " view, " + what, root.base, name, index); root.viewportSized(mode + " view, " + what) })]
        }
        return [root.act(function () { var b = root.chrome.buttonFor(mode); keys.mouseClick(b, b.width / 2, b.height / 2, Qt.LeftButton, Qt.NoModifier, -1); root.park() }),
                root.until("the " + mode + " view drawing the base", function () { return root.pane.viewMode === mode && root.view() !== null && root.settled(root.base) })]
            .concat(root.enter(root.base, 4 + root.ahead, "d004"), root.press("back", root.base), done("mouse Back to the first screen", "d004", 4 + root.ahead),
                    root.enter(root.base, 450 + root.ahead, "d450"), root.press("toolbar", root.base), done("toolbar Back to row 453", "d450", 450 + root.ahead),
                    root.press("forward", root.at("d450")), root.press("back", root.base), done("mouse Back to row 453 after a Forward", "d450", 450 + root.ahead),
                    root.enter(root.base, 4 + root.ahead, "d004"), root.press("toolbar", root.base), done("toolbar Back to the first screen", "d004", 4 + root.ahead))
    }

    readonly property var steps: [].concat(
        [root.until("the window listing the base", function () { return root.settled(root.base) && root.pane.total === 650 }),
         root.act(function () {
             var kids = root.body.children
             for (var i = 0; i < kids.length && !root.backButton; i++)
                 if (typeof kids[i].buttonFor === "function") { root.chrome = kids[i]; root.backButton = kids[i].buttonFor("arrow-left") }
             root.check("the toolbar's Back button is found", root.backButton !== null, true)
             root.check("premise: row 4 is on the first screen", 4 < root.pane.visibleRows, true)
             root.check("premise: row 450 is outside the first window the listing holds", 450 >= root.pane.windowSize, true)
         })],
        // The first screen, through the toolbar: the held rows already carry the name, so nothing is looked up.
        root.enter(root.base, 4, "d004"),
        root.press("toolbar", root.base),
        [root.act(function () {
            root.restored("toolbar Back to the first screen", root.base, "d004", 4)
            root.check("toolbar Back to the first screen: no locate for a row the listing already holds", root.locates(root.since()), [])
            root.viewportSized("toolbar Back to the first screen")
        })],
        // Deep in the listing, with a child cursor of its own, and three folders sorted in ahead while away.
        root.enter(root.base, 450, "d450"),
        root.cursorAt(root.at("d450"), 7),
        root.shell_("mkdir -- \"$1/a-new-0\" \"$1/a-new-1\" \"$1/a-new-2\"", [root.base]),
        root.press("back", root.base),
        [root.act(function () {
            var label = "mouse Back to row 450 after three folders sorted in ahead"
            root.restored(label, root.base, "d450", 450 + root.ahead)
            var asked = root.locates(root.since())
            root.check(label + ": one name-only locate for the off-window row", asked.map(function (r) { return r.path || (r.paths || []).join(",") }), [root.at("d450")])
            root.viewportSized(label)
        })],
        root.shell_(": > \"$1/a0.txt\"", [root.at("d450")]),
        root.press("forward", root.at("d450")),
        [root.act(function () { root.restored("mouse Forward to f07 after a file sorted in ahead", root.at("d450"), "f07.txt", 8) })],
        // Repeated: each direction keeps putting the same items back.
        root.press("back", root.base),
        [root.act(function () { root.restored("second mouse Back", root.base, "d450", 453) })],
        root.press("forward", root.at("d450")),
        [root.act(function () { root.restored("second mouse Forward", root.at("d450"), "f07.txt", 8) })],
        root.press("toolbar", root.base),
        [root.act(function () { root.restored("toolbar Back after a Forward", root.base, "d450", 453) })],
        // The saved item gone: the saved index, then the saved index clamped to a shorter listing.
        root.shell_("rm -- \"$1/f07.txt\"", [root.at("d450")]),
        root.press("forward", root.at("d450")),
        [root.act(function () { root.restored("Forward to a removed f07 falls back to its index", root.at("d450"), "f08.txt", 8) })],
        root.press("back", root.base),
        root.shell_("cd -- \"$1\" && rm -- f0[2-9].txt f[12]?.txt", [root.at("d450")]),
        root.press("forward", root.at("d450")),
        [root.act(function () { root.restored("Forward to a removed f08 clamps index 8 to the last of three", root.at("d450"), "f01.txt", 2) })],
        // An empty destination settles and the history still works from it.
        root.press("back", root.base),
        root.shell_("rm -- \"$1\"/*", [root.at("d450")]),
        root.press("forward", root.at("d450")),
        [root.act(function () {
            var p = root.pane
            root.check("Forward into a now empty folder settles empty at row 0", [p.path, p.listingState, p.cursorIndex, p.listInFlight], [root.at("d450"), "empty", 0, false])
            console.log("geom empty destination shot=" + root.shot("empty destination"))
        })],
        root.press("back", root.base),
        [root.act(function () { root.restored("mouse Back out of the empty folder", root.base, "d450", 453) })],
        // Late answers: the wrapper holds the located reply while something happens, and the answer must not undo it.
        root.heldBack([root.act(function () {
            root.moved = root.pane.cursorIndex + 1
            keys.keyClick(Qt.Key_Down, Qt.NoModifier, -1)
            root.check("held Back, Down: the operator's Down moves the cursor while the answer is held", [root.pane.cursorIndex, root.located], [root.moved, root.locatedAtMark])
        })], [root.settle(root.base), root.act(function () { root.kept("the late answer after Down", true) })]),
        // The wheel moves the viewport and List's clamp drags the cursor with it, which no key path sees.
        root.heldBack([root.act(function () {
            var p = root.pane
            root.wheel(4)
            root.moved = p.cursorIndex
            root.check("held Back, wheel: a real wheel scrolls the list and the cursor follows it off row 0 while the answer is held",
                       [p.listArea.contentY > 0, p.cursorIndex > 0, root.cursorName() !== "d450", root.located], [true, true, true, root.locatedAtMark])
        })], [root.settle(root.base), root.act(function () { root.kept("the late answer after a wheel scroll", false) })]),
        // Away and back: the viewport ends where the locate left it, the cursor does not.
        root.heldBack([root.act(function () {
            var p = root.pane
            root.wheel(4)
            var away = [p.listArea.contentY > 0, p.cursorIndex > 0]
            root.wheel(-8)
            root.moved = p.cursorIndex
            root.check("held Back, wheel away and back: the list went down and back to its top, the cursor left row 0, the answer still held",
                       [away, p.listArea.atYBeginning, p.cursorIndex > 0, root.located], [[true, true], true, true, root.locatedAtMark])
        })], [root.settle(root.base), root.act(function () { root.kept("the late answer after a wheel away and back", false) })]),
        // A filter typed while the answer is held keeps row 0 shown; the answer must not reset, scroll or close it.
        root.heldBack([root.act(function () {
            var p = root.pane
            "/a-new".split("").forEach(function (c) { keys.keyClickChar(c, Qt.NoModifier, -1) })
            root.moved = p.cursorIndex
            root.filterY = Math.round(root.viewY())
            root.check("held Back, filter: / and a-new narrow the held rows to the three new folders, cursor on row 0, the answer still held",
                       [p.filterQuery, p.filterTyping, p.shownTotal, p.cursorIndex, root.cursorName(), root.located], ["a-new", true, 3, 0, "a-new-0", root.locatedAtMark])
        })], [root.settle(root.base), root.act(function () {
            var p = root.pane
            root.check("the late answer under a filter leaves its query, its line and its scroll", [p.filterQuery, p.filterTyping, Math.round(root.viewY())], ["a-new", true, root.filterY])
            root.kept("the late answer under a filter", true)
            keys.keyClick(Qt.Key_Escape, Qt.NoModifier, -1)
            root.check("Escape closes the filter for the cases after it", p.filterQuery, "")
        })]),
        // A tab hop while the answer is held: the same-folder tab it clones resumes the owed d450, and so does the tab left.
        root.heldBack([root.act(function () { Tabs.openNew(root.pane); root.check("held Back, new tab: a second tab opens on the base while the answer is held", [Tabs.count(root.pane), root.pane.path, root.located], [2, root.base, root.locatedAtMark]) })],
                      [root.settle(root.base), root.act(function () { root.restored("the same-folder new tab resumes the owed d450", root.base, "d450", 453); Tabs.selectAt(root.pane, 0) }), root.settle(root.base),
                       root.act(function () { root.restored("returning to the tab whose Back was still owed puts d450 back", root.base, "d450", 453); Tabs.closeAt(root.pane, 1)
                                              root.check("the extra tab closes, leaving the first", [Tabs.count(root.pane), root.pane.path], [1, root.base]) })]),
        // A superseding climb: Back again with no history left climbs while the first answer is held; the
        // place it left is the d450 that Back still owed, so Back from the climb returns there.
        root.heldBack([root.act(function () { root.side(Qt.BackButton) })],
                      [root.settle(root.home), root.act(function () { root.restored("a climb past a held answer keeps the folder it left", root.home, "nav", 2) })]),
        root.press("back", root.base),
        [root.act(function () { root.restored("mouse Back from that climb returns to the d450 the held Back still owed", root.base, "d450", 453) })],
        // Tabs: each keeps its own Back, and one tab's later navigation does not rewrite the other's.
        root.enter(root.base, 453, "d450"),
        [root.act(function () { Tabs.openNew(root.pane) }), root.settle(root.at("d450"))],
        root.press("back", root.base),
        [root.act(function () { root.restored("a new tab's mouse Back", root.base, "d450", 453) })],
        root.enter(root.base, 4 + root.ahead, "d004"),
        [root.act(function () { Tabs.selectAt(root.pane, 0) }), root.settle(root.at("d450"))],
        root.press("back", root.base),
        [root.act(function () { root.restored("the first tab's mouse Back keeps its own item", root.base, "d450", 453) })],
        [root.act(function () { Tabs.selectAt(root.pane, 1) }), root.settle(root.at("d004"))],
        root.press("toolbar", root.base),
        [root.act(function () { root.restored("the second tab's toolbar Back keeps its own item", root.base, "d004", 4 + root.ahead) })],
        root.viewBack("grid"),
        root.viewBack("columns"),
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
        // The single form only: the transfer retry's batched located carries matches instead.
        function onLocated(message) { if (message.matches === undefined) root.located++ }
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
