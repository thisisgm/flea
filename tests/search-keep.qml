import QtQuick
import "../ui/js/Search.js" as Search
import "../ui/js/Swap.js" as Swap
import "../ui/js/Focus.js" as Focus
import "../ui/js/Filter.js" as Filter
import "../ui/js/FilterWire.js" as FilterWire
import "../ui/js/Messages.js" as Messages
import "../ui/js/Nav.js" as Nav
import "../ui/js/RecentMode.js" as RecentMode
import "../ui/js/Anchor.js" as Anchor
import "../ui/js/BackReturn.js" as BackReturn
import "../ui/js/DirSizes.js" as DirSizes
import "../ui/js/Thumbs.js" as Thumbs
import "../ui/js/Reload.js" as Reload
import "../ui/js/Tabs.js" as Tabs
import "js/filterfixture.js" as Fixture
import "js/sourcefixture.js" as Ship

// Two live answers land through the shipped ui/PaneSwap.qml: a row listed again keeps its folder size and thumbnail at every step, and a new row has neither and is asked for.
Item {
    id: root
    property int checks: 0
    property int failures: 0
    property string scope: "/fixture/scope"
    property var pane: null
    property var swap: null
    // A probe run after every landed message, for the case that needs a state seen at each step.
    property var afterStep: null
    property string shipSrc: ""
    property var sidebar: ({ renameEditor: function () { return null } })
    property var themeStub: ({ fileRowHeight: 37 })
    property var viewStub: ({ changeLeaf: function () {} })
    property var capStub: ({ stop: function () {}, restart: function () {} })

    function check(label, actual, expected) {
        root.checks++
        if (JSON.stringify(actual) === JSON.stringify(expected)) {
            console.log("ok   " + label)
            return
        }
        root.failures++
        console.log("FAIL " + label + ": got " + JSON.stringify(actual) + ", expected " + JSON.stringify(expected))
    }
    function type(text) {
        for (var i = 0; i < text.length; i++)
            Focus.handleKey({ key: Qt.Key_A + text.charCodeAt(i) - 97, text: text.charAt(i), modifiers: Qt.NoModifier }, root.pane, root.sidebar)
    }
    function sample() {
        if (root.afterStep !== null)
            root.afterStep()
    }
    property var msgRoot: ({
        listed: function (total, read, sort, path, changed, gen) { root.swap.takeListed(total, read, sort, path, changed, gen) },
        rows: function (start, items, ms, kinds, listing, gen) { root.swap.takeRows(start, items, kinds, listing, gen) },
        searched: function (total, scanned, ms, cancelled, gen, walking) {
            Search.searched(root.pane, total, scanned, ms, cancelled, gen, walking)
        }
    })
    // One shipped PaneSwap function by its own text: the replay holds no landing of its own.
    function shipFn(swap, name, args) {
        var body = Ship.block(root.shipSrc, "function " + name + "(", "ui/PaneSwap.qml")
        // FilterWire is the real module: with no query standing on the stub its relisting is a no-op (issue 315).
        var pre = ["root", "cap", "Swap", "Search", "RecentMode", "Anchor", "BackReturn", "DirSizes", "Thumbs", "Reload", "Tabs", "Nav", "Theme", "ViewState", "FilterWire"]
        var f = new Function(pre.concat(args).join(","), body)
        var deps = [swap, root.capStub, Swap, Search, RecentMode, Anchor, BackReturn, DirSizes, Thumbs, Reload, Tabs, Nav, root.themeStub, root.viewStub, FilterWire]
        return function () { return f.apply(null, deps.concat(Array.prototype.slice.call(arguments))) }
    }
    // The real swap over the stub pane and backend: every landing below runs shipped code.
    function makeSwap(p) {
        var wire = { anchor: null, locateRetry: function () {}, openRenameOnArrival: function () {} }
        var swap = { pane: p, wire: wire, phase: Swap.idle(), landedGen: 0, holding: false, holds: 0, last: { ms: 0, end: "" } }
        swap.takeListed = root.shipFn(swap, "takeListed", ["total", "readMs", "sortMs", "path", "changed", "gen"])
        swap.takeRows = root.shipFn(swap, "takeRows", ["start", "items", "kinds", "listing", "gen"])
        // takeRows stamps tags through the swap, so the stub carries the shipped stampTags too.
        swap.stampTags = root.shipFn(swap, "stampTags", ["items", "fresh"])
        swap.land = root.shipFn(swap, "land", ["start", "items", "kinds", "fresh"])
        swap.applyListed = root.shipFn(swap, "applyListed", ["total", "readMs", "sortMs", "path", "changed"])
        swap.rowsLanded = root.shipFn(swap, "rowsLanded", [])
        swap.release = root.shipFn(swap, "release", ["next", "end"])
        swap.drop = root.shipFn(swap, "drop", [])
        // The shipped hold, minus its cap timer: every listing is answered at once.
        swap.hold = function (ask) {
            swap.phase = Swap.begin(swap.phase, swap.pane.listingState, swap.pane.searchMode, ask, Date.now())
            swap.holding = swap.phase.holding
            return swap.holding
        }
        var shippedRelease = swap.release
        swap.release = function (next, end) {
            shippedRelease(next, end)
            swap.holding = false
        }
        return swap
    }
    function setup(rows, folder, home) {
        root.shipSrc = Ship.source("ui/PaneSwap.qml")
        var p = Fixture.pane()
        p.path = folder
        p.home = home
        p.searchMode = ""
        p.searchQuery = ""
        p.searchFrom = ""
        p.searchHere = false
        p.searchRunning = false
        p.searchCancelled = false
        p.searchScanned = 0
        p.searchMs = 0
        p.searchGen = 0
        p.searchKeyAt = 0
        p.searchFirstRowsAt = 0
        p.searchSettledAt = 0
        // Untagged rows build no tag state, as ui/PaneSwap.qml stampTags reads it.
        p.tagState = null
        p.showHidden = false
        p.windowSize = 200
        p.visibleRows = 4
        p.focusView = "list"
        p.viewMode = "list"
        p.recentMode = ""
        p.searchBase = ""
        p.pendingSelect = ""
        p.listInFlight = false
        p.listingPath = ""
        p.listedSeen = false
        p.held = 0
        p.kindNames = []
        p.dualMode = false
        p.listingState = "ready"
        p.stateMessage = ""
        p.reloadFrom = -1
        p.reloadChanged = -1
        p.appliedListingPreferences = []
        p.listingPreferences = []
        p.rows = rows.slice()
        p.total = rows.length
        p.cursorIndex = 0
        p.refresh()
        p.setCursor = function (index) { Filter.setCursor(p, index) }
        p.renameEditor = function () { return null }
        p.restoreRecentSort = function () {}
        p.clearSelection = function () { p.selection.clear() }
        p.selectionCount = function () { return p.selection.count() }
        p.applyPendingSelect = function () { Nav.applyPendingSelect(p) }
        p.history = []
        p.forwardHistory = []
        p.listArea = { restartSettle: function () {}, restartCoalesce: function () {}, primeSettle: function () {} }
        p.act = function (action) { Focus.act(action, p) }
        p.join = function (base, name) { return base + "/" + name }
        p.backend = {
            heldListing: 0,
            sortBy: "name",
            sortDesc: false,
            searchOpen: function () {},
            searchQuery: function () {},
            searchClose: function () {},
            resetSort: function () {},
            list: function () {},
            askFsInfo: function () {}
        }
        root.pane = p
        root.swap = root.makeSwap(p)
        p.swap = root.swap
    }
    // The Size cell text of the row named name: its bytes once answered, else the placeholder dot.
    function sizeCell(name) {
        for (var i = 0; i < root.pane.rows.length; i++) {
            if (root.pane.rows[i].n === name) {
                var size = DirSizes.sizeFor(root.pane.dirSizeState, root.pane.held + i)
                return size ? String(size.bytes) : "dot"
            }
        }
        return "absent"
    }
    // The thumbnail file the tile of the row named name draws: its file once answered, else the empty icon fallback.
    function thumbCell(name) {
        for (var i = 0; i < root.pane.rows.length; i++) {
            if (root.pane.rows[i].n === name)
                return Thumbs.fileFor(root.pane.thumbState, root.pane.held + i) || "icon"
        }
        return "absent"
    }
    // One canned answer triple through the shipped landing, the reply a backend would send.
    function feedAnswer(gen, rows) {
        Messages.route(msgRoot, { t: "listed", n: rows.length, read: 1, sort: 1, path: root.scope, changed: 0, v: 1, w: true, gen: gen })
        sample()
        Messages.route(msgRoot, { t: "rows", start: 0, rows: rows, ms: 1, kinds: [], listing: 900 + gen, gen: gen })
        sample()
        Messages.route(msgRoot, { t: "searched", n: rows.length, scanned: 10, ms: 5, cancelled: false, gen: gen, walking: false })
        sample()
        root.pane.refresh()
    }
    function openSession() {
        Search.closeSession(root.pane)
        Search.start(root.pane)
        type("n")
    }
    // A live answer keeps the sizes of the folders it lists again, and a folder new to the answer still shows the dot.
    function checkDirSizesKept() {
        openSession()
        feedAnswer(root.pane.searchGen, [{ n: "docs", d: true }, { n: "sub", d: true }, { n: "notes.md", d: false }])
        root.pane.dirSizeState = DirSizes.remember(DirSizes.applied(root.pane.dirSizeState, [1]), 1, 4096, false, 240)
        root.check("a folder answered in one live answer shows its bytes", sizeCell("sub"), "4096")
        var seen = []
        root.afterStep = function () { seen.push(sizeCell("sub") + "|" + sizeCell("fresh")) }
        type("o")
        feedAnswer(root.pane.searchGen, [{ n: "fresh", d: true }, { n: "docs", d: true }, { n: "sub", d: true }])
        root.afterStep = null
        root.check("the folder kept by the next live answer never shows the dot, at its new index",
            seen.filter(function (cell) { return cell.indexOf("dot|") === 0 }), [])
        root.check("the folder kept by the next live answer shows the same bytes after it lands",
            sizeCell("sub"), "4096")
        root.check("a folder new to the answer still shows the dot", sizeCell("fresh"), "dot")
        root.check("a folder kept is answered, so only the new and unanswered folders are asked for",
            DirSizes.plan(root.pane.dirSizeState, root.pane.rows, root.pane.held, 0, 2).join(","), "0,1")
    }
    // A live answer keeps the thumbnails of the files it lists again, and a file new to the answer draws its icon until asked.
    function checkThumbsKept() {
        openSession()
        feedAnswer(root.pane.searchGen, [{ n: "cat.png", t: true }, { n: "dog.png", t: true }, { n: "notes.md" }])
        root.pane.thumbState = Thumbs.remember(Thumbs.applied(root.pane.thumbState, { ask: [0, 1], drop: [] }), 0, "/cache/cat.png", 240)
        root.check("a file answered in one live answer draws its thumbnail", thumbCell("cat.png"), "/cache/cat.png")
        var seen = []
        root.afterStep = function () { seen.push(thumbCell("cat.png") + "|" + thumbCell("new.png")) }
        type("o")
        feedAnswer(root.pane.searchGen, [{ n: "new.png", t: true }, { n: "dog.png", t: true }, { n: "cat.png", t: true }])
        root.afterStep = null
        root.check("the file kept by the next live answer never falls back to its icon, at its new index",
            seen.filter(function (cell) { return cell.indexOf("icon|") === 0 }), [])
        root.check("the file kept by the next live answer draws the same thumbnail after it lands",
            thumbCell("cat.png"), "/cache/cat.png")
        root.check("a file new to the answer has no thumbnail", thumbCell("new.png"), "icon")
        root.check("an ask still pending is not carried", thumbCell("dog.png"), "icon")
        root.check("a thumbnail kept is answered, so only the new and unanswered files are asked for",
            Thumbs.plan(root.pane.thumbState, root.pane.rows, root.pane.held, 0, 2).ask.join(","), "0,1")
    }
    Timer {
        interval: 10
        repeat: false
        running: true
        onTriggered: {
            setup([{ n: "a" }, { n: "b" }, { n: "c" }], root.scope + "/sub", root.scope)
            checkDirSizesKept()
            checkThumbsKept()
            console.log("SEARCHKEEP DONE checks=" + root.checks + " failed=" + root.failures)
            Qt.exit(root.failures === 0 ? 0 : 1)
        }
    }
}
