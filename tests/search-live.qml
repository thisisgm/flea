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

// Replays the session tests/search-live.sh recorded: each key goes through Focus.handleKey and must send the S lines, and the R lines land in the shipped ui/PaneSwap.qml, built from its own text.
Item {
    id: root
    property int checks: 0
    property int failures: 0
    property int blanks: 0
    // What the shipped per-frame counter would have counted on each landed turn.
    property int frameBlanks: 0
    property bool hadRows: false
    property var sent: []
    property string relisted: ""
    property string opened: ""
    property string scope: ""
    property string folder: ""
    property var folderRows: []
    property var joinedBases: []
    property var said: []
    property var pane: null
    property var swap: null
    property string shipSrc: ""
    property var sidebar: ({ renameEditor: function () { return null } })
    // The singletons the shipped landing reads, stubbed: the row height its anchor math needs and the sort-leaf writer only a directory landing reaches.
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
    function eq(a, b) {
        if (typeof a !== typeof b)
            return false
        if (a === null || b === null || typeof a !== "object")
            return a === b
        if (Array.isArray(a) !== Array.isArray(b) || Object.keys(a).length !== Object.keys(b).length)
            return false
        for (var k in a)
            if (!eq(a[k], b[k]))
                return false
        return true
    }
    function key(code, text) { return { key: code, text: text, modifiers: Qt.NoModifier } }
    function type(text) {
        for (var i = 0; i < text.length; i++)
            Focus.handleKey(key(Qt.Key_A + text.charCodeAt(i) - 97, text.charAt(i)), root.pane, root.sidebar)
    }
    function press(code) { Focus.handleKey(key(code, ""), root.pane, root.sidebar) }
    function names() { return root.pane.rows.map(function (r) { return r.n }) }
    function sample() {
        if (root.pane === null)
            return
        if (root.hadRows && root.pane.rows.length === 0 && root.pane.total > 0 && root.pane.searchMode.length > 0)
            root.blanks++
        var live = Search.live(root.pane)
        if (Swap.frameKind(root.pane.listInFlight, root.pane.listingState, root.swap.phase.fellBack === true, root.pane.rows.length, root.pane.total, live) === "blank")
            root.frameBlanks++
        if (root.pane.rows.length > 0)
            root.hadRows = true
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
        // The shipped hold, minus its cap timer: the replay answers every listing at once.
        swap.hold = function (ask) {
            swap.phase = Swap.begin(swap.phase, swap.pane.listingState, swap.pane.searchMode, ask, Date.now())
            swap.holding = swap.phase.holding
            if (swap.holding)
                swap.holds += 1
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
        p.cursorIndex = 2
        p.refresh()
        p.setCursor = function (index) { Filter.setCursor(p, index) }
        p.renameEditor = function () { return null }
        p.restoreRecentSort = function () {}
        p.clearSelection = function () { p.selection.clear() }
        p.selectionCount = function () { return p.selection.count() }
        p.applyPendingSelect = function () { Nav.applyPendingSelect(p) }
        p.history = []
        p.forwardHistory = []
        p.opened = function (target) { root.opened = target }
        p.message = function (text, isError) { root.said.push(text) }
        p.join = function (base, name) { root.joinedBases.push(base); return base + "/" + name }
        p.listArea = { restartSettle: function () {}, restartCoalesce: function () {}, primeSettle: function () {} }
        p.act = function (action) { Focus.act(action, p) }
        p.escapePressed = function () { p.escaped = true }
        // The exit runs the shipped navigation, so the hold it takes is the product's own.
        p.openWithoutHistory = function (next, options) { Nav.openWithoutHistory(p, next, options) }
        p.open = function (target) {
            root.opened = target
            Nav.open(p, target)
        }
        p.backend = {
            heldListing: 0,
            sortBy: "name",
            sortDesc: false,
            searchOpen: function (scope, folder, hidden) {
                root.sent.push({ c: "searchopen", path: scope, folder: folder, hidden: hidden })
            },
            searchQuery: function (query, gen, count) {
                root.sent.push({ c: "searchquery", query: query, gen: gen, count: count })
            },
            searchClose: function () { root.sent.push({ c: "searchclose" }) },
            resetSort: function (path) {},
            // The exit's own listing is recorded, never sent: the transcript owns every send.
            list: function (path) { root.relisted = path },
            askFsInfo: function () {}
        }
        root.pane = p
        root.swap = root.makeSwap(p)
        p.swap = root.swap
    }
    function stroke(k) {
        if (k === "BS")
            press(Qt.Key_Backspace)
        else if (k === "Down")
            press(Qt.Key_Down)
        else if (k === "Up")
            press(Qt.Key_Up)
        else if (k === "Return")
            press(Qt.Key_Return)
        else if (k === "Tab")
            press(Qt.Key_Tab)
        else if (k === "Escape")
            press(Qt.Key_Escape)
        else if (k === "Reveal")
            Focus.act("reveal", root.pane)
        else
            type(k)
    }
    // One transcript key block: the keystroke, its sends against the S lines, then its R lines; a list line is the escape relist the stub records instead of sending.
    function runBlock(block) {
        var before = root.sent.length
        stroke(block.key)
        sample()
        if (block.key === "Escape" && exitHolds(block))
            checkExitHeld()
        var made = root.sent.slice(before)
        var want = []
        for (var i = 0; i < block.sends.length; i++) {
            if (block.sends[i].c === "list") {
                root.check("escape re-lists " + block.sends[i].path, root.relisted, block.sends[i].path)
                continue
            }
            want.push(block.sends[i])
        }
        root.check("the " + block.key + " keystroke sends what the wire sent",
            made.every(function (m, j) { return eq(m, want[j]) }) && made.length === want.length, true)
        for (var r = 0; r < block.receives.length; r++) {
            Messages.route(msgRoot, JSON.parse(block.receives[r]))
            sample()
        }
        root.pane.refresh()
        checkKey(block.key, block.count)
        if (block.key === "Reveal")
            settleReveal()
    }
    function checkKey(k, n) {
        if (n === 0)
            root.check("f opens the session on the scope above the folder", root.pane.searchMode, "typing")
        if (n === 1 || n === 10 || n === 17) {
            root.check("ranked result rows land with no Enter", names(),
                ["notes.md", "north.txt", "docs/notes-old.md", "sub/photo.png"])
            var stamps = Search.stamps(root.pane)
            root.check("the stopwatch names the newest gen in order",
                stamps.gen === root.pane.searchGen && stamps.keyAt > 0
                && stamps.keyAt <= stamps.firstRowsAt && stamps.firstRowsAt <= stamps.settledAt, true)
        }
        if (n === 2)
            root.check("the next keystroke replaces the window whole", names(),
                ["notes.md", "north.txt", "docs/notes-old.md"])
        if (n === 3)
            root.check("down moves the result cursor without leaving the query",
                root.pane.cursorIndex + "|" + root.pane.searchMode, "1|typing")
        if (n === 4)
            root.check("and up moves it back", root.pane.cursorIndex, 0)
        if (n === 5)
            root.check("down again stands on the second match", root.pane.cursorIndex, 1)
        if (n === 6)
            root.check("a new query parks the cursor on its best match", root.pane.cursorIndex, 0)
        if (n === 6)
            root.check("backspace narrows through the index", names()[0] + "|" + root.pane.searchQuery, "notes.md|n")
        if (n === 7 || n === 9)
            root.check("backspace to empty brings the folder rows back, strip open",
                names().length + "|" + root.pane.searchMode, "4|typing")
        if (n === 8)
            root.check("a query with no match lands an honest empty window",
                names().length + "|" + root.pane.total, "0|0")
        if (n === 11)
            root.check("tab re-opens the folder scope and its row lands", names(), ["photo.png"])
        if (n === 12)
            root.check("tab back re-opens the scope and scoped rows land", names(),
                ["notes.md", "north.txt", "docs/notes-old.md", "sub/photo.png"])
        if (n === 13)
            root.check("enter keeps the results and moves the keys to them", root.pane.searchMode, "results")
        if (n === 14)
            root.check("keys now walk the results", root.pane.cursorIndex, 1)
        if (n === 15) {
            root.check("escape returns to the folder and cursor", root.pane.cursorIndex + "|" + names().length, "2|4")
            root.check("with the file still marked", root.pane.selectedIndices().join(","), "2")
        }
        if (n === 16)
            root.check("f opens the session on the scope above the folder", root.pane.searchMode, "typing")
        if (n === 18)
            root.check("enter keeps the scoped results", root.pane.searchMode, "results")
        if (n === 19) {
            var row = root.pane.rowFor(root.pane.cursorIndex)
            root.check("reveal joins the cursor row against the scope, not the folder",
                root.joinedBases[root.joinedBases.length - 1], root.scope)
            root.check("and opens the folder holding it", root.opened, root.scope)
            root.check("naming the revealed file", root.pane.pendingSelect, root.scope + "/notes.md")
        }
        if (n === 20)
            root.check("escape with no session leaves rows alone", names().length + "|" + root.pane.escaped, "4|true")
    }
    function load(path) {
        var xhr = new XMLHttpRequest()
        xhr.onreadystatechange = function () {
            if (xhr.readyState !== XMLHttpRequest.DONE)
                return
            parse(xhr.responseText)
        }
        xhr.open("GET", "file://" + path)
        xhr.send()
    }
    // Sample input: "# key n" opens a block, "S {"c": "searchquery", "count": 200, "gen": 1, "query": "n"}" is a send, "R {"cancelled": false, "gen": 1, "ms": 0.04, "n": 4, "scanned": 9, "t": "searched", "walking": false}" a reply.
    function parse(text) {
        var block = null
        var blocks = []
        var lines = text.split("\n")
        for (var i = 0; i < lines.length; i++) {
            var line = lines[i]
            if (line.indexOf("# key ") === 0) {
                block = { key: line.substring(6), count: blocks.length, sends: [], receives: [] }
                blocks.push(block)
            } else if (line.indexOf("S ") === 0)
                block.sends.push(JSON.parse(line.substring(2)))
            else if (line.indexOf("R ") === 0)
                block.receives.push(line.substring(2))
        }
        // The folder rows seed the pane; the scope is what the first session opens.
        var folder = blocks[0].sends[0].folder
        root.scope = blocks[0].sends[0].path
        root.folder = folder
        var folderRows = []
        for (var e = 0; e < blocks.length && folderRows.length === 0; e++) {
            for (var s = 0; s < blocks[e].sends.length; s++) {
                if (blocks[e].sends[s].c === "searchquery" && blocks[e].sends[s].query === "") {
                    for (var r = 0; r < blocks[e].receives.length; r++) {
                        var m = JSON.parse(blocks[e].receives[r])
                        if (m.t === "rows")
                            folderRows = m.rows
                    }
                }
            }
        }
        setup(folderRows, folder, root.scope)
        root.folderRows = folderRows
        for (var k = 0; k < blocks.length; k++)
            runBlock(blocks[k])
        // A new session's first answer parks on its best match, and the closed session's late line at its newest gen drops.
        var tailRows = [{ n: "notes.md" }, { n: "north.txt" }, { n: "docs/notes-old.md" }]
        Search.start(root.pane)
        type("n")
        feedAnswer(root.pane.searchGen, tailRows)
        type("o")
        feedAnswer(root.pane.searchGen, tailRows)
        Search.closeSession(root.pane)
        Search.start(root.pane)
        feedAnswer(root.pane.searchGen, [{ n: "stale-answer.md" }])
        root.check("the closed session's late answer lands no row on the next session", names().indexOf("stale-answer.md"), -1)
        press(Qt.Key_Down)
        type("n")
        type("o")
        feedAnswer(root.pane.searchGen, tailRows)
        root.check("a new session parks on its best match after the closed session's late answer dropped", root.pane.cursorIndex, 0)
        root.check("no turn drew an empty listing beside a count naming rows", root.blanks, 0)
        root.check("the shipped counter counts no blank frame on any turn either", root.frameBlanks, 0)
        console.log("SEARCHLIVE DONE checks=" + root.checks + " failed=" + root.failures)
        Qt.exit(root.failures === 0 ? 0 : 1)
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
    // The stub backend answers the reveal's own listing at once, so the exit lands like any held one.
    function settleReveal() {
        Messages.route(msgRoot, { t: "listed", n: root.folderRows.length, read: 1, sort: 1, path: root.opened, changed: 0, v: 1, w: true, gen: 0 })
        sample()
        Messages.route(msgRoot, { t: "rows", start: 0, rows: root.folderRows, ms: 1, kinds: [], listing: 901, gen: 0 })
        sample()
        root.pane.refresh()
    }
    // An exit holds the whole search state, strip and rows, until the folder lands in one turn.
    function exitHolds(block) {
        for (var i = 0; i < block.sends.length; i++)
            if (block.sends[i].c === "list")
                return true
        return false
    }
    function checkExitHeld() {
        root.check("the exit holds the strip while its rows stand", root.pane.searchMode.length > 0, true)
        root.check("the exit holds the result rows until the folder lands", names().join(","),
            "notes.md,north.txt,docs/notes-old.md,sub/photo.png")
    }
    Timer {
        interval: 10
        repeat: false
        running: true
        onTriggered: {
            var args = Qt.application.arguments
            root.load(args[args.length - 1])
        }
    }
}
