.import "../../ui/js/Reload.js" as Reload
.import "../../ui/js/Messages.js" as Messages
.import "sourcefixture.js" as Source

// F5 and Ctrl+R re-read the folder through the listing swap, saying the changed count only when rows changed.

function pane() {
    return {
        listInFlight: false,
        listingState: "ready",
        stateMessage: "",
        searchMode: "",
        recentMode: "",
        total: 10,
        held: 0,
        windowSize: 40,
        path: "/home/gm/Work",
        cursorIndex: 3,
        filterQuery: "",
        reloadFrom: -1,
        reloadChanged: -1,
        said: [],
        listed: [],
        refreshed: [],
        asked: [],
        windowed: [],
        rowFor: function () { return { n: "notes.txt" } },
        message: function (text) { this.said.push(text) },
        openWithoutHistory: function (path, options) {
            this.listed.push(path)
            this.asked.push(options && options.wantChanged === true)
            this.reloadFrom = -1
        },
        refresh: function (select) { this.refreshed.push(select) },
        backend: { window: function (start, count) { } }
    }
}

function wire() {
    return { anchor: "kept" }
}

function run(check) {
    check("slow decisions live in their own module", typeof Reload.line, "function")
    if (typeof Reload.line !== "function" || typeof Reload.begin !== "function" || typeof Reload.landed !== "function")
        return

    check("a reload names two changed rows", Reload.line(2), "Reloaded · 2 rows changed")
    check("and one changed row reads singular", Reload.line(1), "Reloaded · 1 row changed")
    check("and a thousand groups the count the way every other count does", Reload.line(1246), "Reloaded · 1,246 rows changed")

    var busy = pane()
    busy.listInFlight = true
    check("a reload refused while a listing is out lists nothing", Reload.begin(busy, wire()), false)
    check("and says why with the sentence every refused navigation gives",
          busy.said.join("|") + "|" + busy.listed.length, "A directory is already loading.|0")

    // A listing past its deadline is let go by a reload that lists and left standing by one that lists nothing.
    function waiting() {
        var p = pane()
        p.listInFlight = true
        p.listingState = "waiting"
        p.stateMessage = "That folder is not responding."
        return p
    }
    function standing(p) { return p.listInFlight + "|" + p.listingState + "|" + p.said.length }
    var held = waiting()
    check("a reload past the wait lists the folder", Reload.begin(held, wire()) + "|" + held.listed.join(",") + "|" + standing(held), "true|/home/gm/Work|false|loading|0")
    var heldSearch = waiting()
    heldSearch.searchMode = "results"
    check("over search results it lists nothing and keeps the wait", Reload.begin(heldSearch, wire()) + "|" + heldSearch.listed.length + "|" + standing(heldSearch), "false|0|true|waiting|0")
    var heldRecent = waiting()
    heldRecent.recentMode = "results"
    check("over Recent it re-reads the history past the wait", Reload.begin(heldRecent, wire()) + "|" + heldRecent.refreshed.length + "|" + standing(heldRecent), "true|1|false|loading|0")
    // A first listing that never landed has no path of its own yet, so the reload retries the folder it waits on.
    var heldFirst = waiting()
    heldFirst.path = ""
    heldFirst.listingPath = "/mnt/slow/"
    check("a reload of a first listing past the wait retries the folder it waits on", Reload.begin(heldFirst, wire()) + "|" + heldFirst.listed.join(",") + "|" + standing(heldFirst), "true|/mnt/slow|false|loading|0")
    var heldHop = waiting()
    heldHop.listingPath = "/mnt/slow"
    check("and a hop that never landed is retried, not the folder it left", Reload.begin(heldHop, wire()) + "|" + heldHop.listed.join(","), "true|/mnt/slow")

    var searching = pane()
    searching.searchMode = "results"
    check("a reload over search results asks for no listing", Reload.begin(searching, wire()), false)
    check("and says nothing, the way the sort keys go quiet there", searching.said.length + "|" + searching.listed.length, "0|0")

    var recent = pane()
    recent.recentMode = "results"
    var recentWire = wire()
    check("a reload over Recent re-reads its history", Reload.begin(recent, recentWire), true)
    check("through refresh rather than re-listing its base", recent.refreshed.length + "|" + recent.listed.length, "1|0")
    check("and remembers the count it is answering against", recent.reloadFrom, 10)
    check("and leaves the watched anchor alone", recentWire.anchor, "kept")

    var plain = pane()
    var w = wire()
    check("a reload of the open folder re-lists it", Reload.begin(plain, w), true)
    check("through the same anchored re-read a watched change takes", plain.listed.join(","), "/home/gm/Work")
    check("and asks the backend to count the rows a rename may have moved", plain.asked.join(","), "true")
    check("and remembers the count it is answering against", plain.reloadFrom, 10)
    check("and holds the cursor anchor for the rows reply", (w.anchor ? w.anchor.name : "") + "|" + (w.anchor ? w.anchor.index : "") + "|" + (w.anchor ? w.anchor.path : ""), "notes.txt|3|/home/gm/Work")

    var renamed = pane()
    renamed.reloadFrom = 10
    renamed.reloadChanged = 2
    renamed.total = 10
    Reload.landed(renamed)
    check("one added and one removed still say two changed", renamed.said.join("|"), "Reloaded · 2 rows changed")
    check("and spend the reload, so the next listing says nothing", renamed.reloadFrom + "|" + renamed.reloadChanged, "-1|-1")
    var recentLanded = pane()
    recentLanded.recentMode = "results"
    recentLanded.reloadFrom = 2
    recentLanded.reloadChanged = 2
    recentLanded.total = 2
    Reload.landed(recentLanded)
    check("a Recent re-read with one path replaced says two changed", recentLanded.said.join("|"), "Reloaded · 2 rows changed")
    var grown = pane()
    grown.said = []
    grown.reloadFrom = 10
    grown.reloadChanged = 4
    grown.total = 12
    Reload.landed(grown)
    check("three added and one removed say four", grown.said.join("|"), "Reloaded · 4 rows changed")
    var shrunk = pane()
    shrunk.reloadFrom = 10
    shrunk.reloadChanged = 1
    shrunk.total = 9
    Reload.landed(shrunk)
    check("one row lost reads singular", shrunk.said.join("|"), "Reloaded · 1 row changed")
    var same = pane()
    same.reloadFrom = 10
    same.reloadChanged = 0
    same.total = 10
    Reload.landed(same)
    check("a folder with nothing added or removed says nothing at all", same.said.length, 0)
    var legacy = pane()
    legacy.reloadFrom = 10
    legacy.reloadChanged = -1
    legacy.total = 12
    Reload.landed(legacy)
    check("a listing with no backend count falls back to net delta", legacy.said.join("|"), "Reloaded · 2 rows changed")
    var idle = pane()
    Reload.landed(idle)
    check("an ordinary navigation owes no notice", idle.said.length + "|" + idle.reloadFrom, "0|-1")

    sidebarReplyOrder(check)

    // The listed line's changed count reaches the listed signal PaneSwap reads; a missing one reads unknown.
    var routed = []
    var fake = { dirDev: 0, dirWritable: true,
        listed: function (total, readMs, sortMs, path, changed) { routed.push(total + "|" + path + "|" + changed) } }
    // Sample input: {"t":"listed","n":12,"read":1,"sort":2,"path":"/d","changed":2} routes 2.
    Messages.route(fake, { t: "listed", n: 12, read: 1, sort: 2, path: "/d", changed: 2 })
    check("a listed line with a count hands it to the listed signal", routed.join(";"), "12|/d|2")
    // Sample input: the same line with no changed field routes undefined, which PaneSwap reads as unknown.
    Messages.route(fake, { t: "listed", n: 12, read: 1, sort: 2, path: "/d" })
    check("and one without hands over no count", routed.join(";"), "12|/d|2;12|/d|undefined")
    // Each check reads the shipped QML source, so a live window is not needed.
    var swap = Source.source("ui/PaneSwap.qml")
    var swapListed = Source.slice(swap, "function applyListed(", "function ")
    check("PaneSwap keeps the listed line's count for the reload", swapListed.indexOf("pane.reloadChanged = (changed === undefined || changed === null) ? -1 : changed") >= 0, true)
    var backend = Source.source("ui/Backend.qml")
    var backendList = Source.slice(backend, "function listRequest(", "function ")
    check("the list request sends wantChanged only when asked", backendList.indexOf("wantChanged: wantChanged === true") >= 0, true)
    check("Backend declares changed and gen on its listed signal", backend.indexOf("signal listed(int total, real readMs, real sortMs, string path, var changed, real gen)") >= 0, true)
    check("PaneSwap applyListed declares the changed count", swapListed.indexOf("function applyListed(total, readMs, sortMs, path, changed)") === 0, true)
    returnChecks(check)
}

// A FUSE, network or phone folder raises no inotify for a change made elsewhere, so the window coming back re-reads it.
function returnChecks(check) {
    function remote(fsName, storageClass) {
        var p = pane()
        p.fsName = fsName
        p.storageClass = storageClass
        p.wire = { stale: false, rereads: 0, reread: function () { this.rereads += 1 } }
        return p
    }
    function owed(p) { return Reload.returned(p) + "|" + p.wire.stale + "|" + p.wire.rereads }
    check("an rclone or sshfs folder is re-read when the window returns", owed(remote("fuse", "network")), "true|true|1")
    check("so is a FUSE folder the class calls local, as mergerfs is", owed(remote("fuse", "")), "true|true|1")
    check("and a kernel NFS share", owed(remote("nfs", "network")), "true|true|1")
    check("and a phone", owed(remote("fuse", "phone")), "true|true|1")
    check("a local folder keeps trusting its inotify watch", owed(remote("btrfs", "")), "false|false|0")
    var out = remote("fuse", "network")
    out.listInFlight = true
    check("a listing already out is fresh, so it owes nothing", owed(out), "false|false|0")
    var walk = remote("fuse", "network")
    walk.searchMode = "results"
    check("a search owns its rows", owed(walk), "false|false|0")
    var hidden = remote("fuse", "network")
    hidden.visible = false
    check("a pane not drawn owes nothing", owed(hidden), "false|false|0")
    var away = remote("fuse", "network")
    var steps = [[0, true], [1, true], [1, false], [2, false], [2, true], [1, true]].map(function (step) {
        return Reload.front(away, step[0], step[1]) + ":" + away.wire.rereads
    })
    check("only a return after leaving the front re-reads, never the launch or a repeat", steps.join(" "), "1:0 1:0 2:0 2:0 1:1 1:1")
    check("and an inactive window never seen at the front stays unseen", Reload.front(away, 0, false), 0)
    var hook = Source.slice(Source.source("ui/PaneSwap.qml"), "property int front: 0", "\n}")
    check("PaneSwap hands every activation change to Reload.front", hook.indexOf("Window.onActiveChanged: root.front = Reload.front(root.pane, root.front, Window.active)") >= 0, true)
}

// Execute the suite's reload steps with a reply inside keyClick and with a reply delivered later.
function sidebarReplyOrder(check) {
    var source = Source.source("tests/sidebar-flows.qml")
    var steps = Source.slice(source, "function reloadChecks(mode) {", "\n    Timer {")
    // Sample input: function onReloadFromChanged() { if (pane.reloadFrom >= 0) root.reloadNotice = { from: 0, total: 1 } }
    var observer = source.match(/function onReloadFromChanged\(\) \{([^\n]*)\}/)
    // Sample input: function ready(path) { return !pane.listInFlight && pane.path === path }
    var readySource = source.match(/function ready\(path\) \{([^\n]*)\}/)[1]
    function scenario(inline, omitCtrlArm) {
        var root = { fixture: "/fixture", reloadStage: 0, beforeLists: 0,
            reloadNotice: { from: -1, total: 0 }, observedMessages: [], pick: function () {} }
        var pane = { path: root.fixture, viewMode: "list", listingState: "ready", listInFlight: false,
            total: 4, held: 0, windowSize: 40, cursorIndex: 0, filterQuery: "", searchMode: "", recentMode: "",
            reloadChanged: -1, cursorRow: { n: "a.txt" }, rowFor: function () { return this.cursorRow },
            backend: { listRequests: 0 },
            openWithoutHistory: function () { this.listInFlight = true; this.reloadFrom = -1; this.backend.listRequests++ },
            message: function (text) { root.observedMessages.push(text) } }
        var from = -1
        var heard = observer ? new Function("root", "pane", observer[1]) : function () {}
        Object.defineProperty(pane, "reloadFrom", {
            get: function () { return from },
            set: function (value) { if (value !== from) { from = value; heard(root, pane) } }
        })
        var failures = []
        var assertions = 0
        var qt = { Key_F5: 1, Key_R: 2, ControlModifier: 4 }
        function verify(label, actual, expected) {
            assertions++
            if (JSON.stringify(actual) !== JSON.stringify(expected)) failures.push(label + ": got " + JSON.stringify(actual) + ", expected " + JSON.stringify(expected))
        }
        var ready = new Function("pane", "return function(path) {" + readySource + "}")(pane)
        var runStep = new Function("root", "pane", "Qt", "ready", "check", "fixture", "return " + steps)(root, pane, qt, ready, verify, root.fixture)
        function land() {
            pane.reloadChanged = pane.backend.listRequests === 1 ? 1 : 0
            pane.total = 5
            Reload.landed(pane)
            pane.listInFlight = false
        }
        root.press = function (key) {
            if (omitCtrlArm && key === qt.Key_R) pane.openWithoutHistory()
            else Reload.begin(pane, { anchor: null })
            if (inline) land()
        }
        check("reload probe keeps the F5 step pending", runStep("list"), false)
        if (!inline) {
            var before = assertions
            check("reload probe waits on listInFlight before F5 assertions", runStep("list"), false)
            check("reload probe makes no assertions before F5 completes", assertions, before)
            land()
        }
        check("reload probe keeps the CtrlR step pending", runStep("list"), false)
        if (!inline) {
            before = assertions
            check("reload probe waits on listInFlight before CtrlR assertions", runStep("list"), false)
            check("reload probe makes no assertions before CtrlR completes", assertions, before)
            land()
        }
        check("reload probe ends after CtrlR completion", runStep("list"), true)
        if (omitCtrlArm) {
            check("reload probe rejects a CtrlR that never armed its own notice", failures.length, 1)
            check("reload probe names the missing CtrlR arm", failures[0].indexOf("reload-notice-CtrlR-list"), 0)
        } else check("reload probe handles " + (inline ? "reply before key return" : "reply after key return"), failures.join(" | "), "")
    }
    scenario(true, false)
    scenario(false, false)
    scenario(true, true)
}
