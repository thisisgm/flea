//@ pragma ShellId flea-search-warm-test

import QtQuick
import Quickshell
import "flea" as Flea
import "flea/js/Swap.js" as Swap
import "flea/js/DirSizes.js" as DirSizes

// The first answer of a search over a folder shorter than the view builds nothing in any view: the strip's warm turn already built it, unseen.
ShellRoot {
    id: root

    readonly property string fixture: Quickshell.env("WARM_DIR")
    readonly property int tickMs: 20
    readonly property int giveUpMs: 8000
    readonly property int warmMs: 300
    readonly property var ops: [
        { view: "list" }, { start: true }, { key: "n" }, { exit: true }, { drop: true },
        { view: "columns" }, { start: true }, { key: "n" }, { exit: true }, { drop: true },
        { view: "grid" }, { start: true }, { key: "n" }, { exit: true }, { drop: true },
        { view: "list" }, { firstkey: "n" }, { exit: true }
    ]
    property int at: 0
    property var awaiting: null
    property double stamp: Date.now()
    property int checks: 0
    property int failures: 0
    property var pane: null
    property var keys: null
    property var lv: null
    property bool ticking: false
    property bool finished: false
    // Every item under a delegate carries a census tag once seen, so a later census names the items built since.
    property int seq: 0
    property var seen: ({})
    // What the strip's first frame drew, kept from its frameSwapped, before the warm turn runs.
    property var stripFrame: null
    property int gen: 0
    property var answerFrame: null
    // Every Loader inside the rows, pooled ones too, counts each item it builds or drops while the answer lands.
    property int flips: 0
    property var watched: []
    function flip() { root.flips += 1 }
    // Answers the first key landed and frames that drew one, from the key until it settled.
    property int answerLands: 0
    property int answerFrames: 0
    property bool counting: false
    property bool rowsMoved: false
    // Each searched line the first key drew, with its walking flag and its arrival after the key was sent.
    property var answers: []
    property double keySentAt: 0
    function watchLoaders() {
        root.unwatchLoaders()
        root.flips = 0
        var items = root.delegates()
        for (var i = 0; i < items.length; i++) {
            root.walk(items[i], function (item) {
                if (item.sourceComponent !== undefined && item.item !== undefined && item.itemChanged !== undefined) {
                    item.itemChanged.connect(root.flip)
                    root.watched.push(item)
                }
            })
        }
    }
    function unwatchLoaders() {
        for (var i = 0; i < root.watched.length; i++) if (root.watched[i]) root.watched[i].itemChanged.disconnect(root.flip)
        root.watched = []
    }

    function check(label, actual, expected) {
        root.checks += 1
        var ok = JSON.stringify(actual) === JSON.stringify(expected)
        if (!ok) root.failures += 1
        console.log("SEARCHWARM " + (ok ? "PASS " : "FAIL ") + label + " got=" + JSON.stringify(actual) + " expected=" + JSON.stringify(expected))
    }
    // Destroying the pane cancels the row builds a view still queues past its viewport, so the engine quits with none in flight.
    function finish() {
        if (root.finished) return
        root.finished = true
        console.log("SEARCHWARM DONE checks=" + root.checks + " failed=" + root.failures)
        if (root.pane) root.pane.destroy()
    }
    function delegates() {
        var out = []
        var kids = root.lv ? root.lv.contentItem.children : []
        for (var i = 0; i < kids.length; i++) if (kids[i].listingIndex !== undefined) out.push(kids[i])
        return out
    }
    function walk(item, visit) {
        visit(item)
        for (var i = 0; i < item.children.length; i++) root.walk(item.children[i], visit)
    }
    // Delegates and every item inside them built or destroyed since the last census.
    function census() {
        var now = {}
        var made = { rows: 0, items: 0 }
        var items = root.delegates()
        for (var i = 0; i < items.length; i++) {
            root.walk(items[i], function (item) {
                if (item.objectName === "") {
                    root.seq += 1
                    item.objectName = "warm" + root.seq
                    made.items += 1
                    if (item.listingIndex !== undefined) made.rows += 1
                }
                now[item.objectName] = true
            })
        }
        var gone = 0
        for (var name in root.seen) if (!now[name]) gone += 1
        root.seen = now
        return { rows: made.rows, items: made.items, gone: gone }
    }
    // Every text the drawn rows show, in row order: a hidden layout adds none, and a pooled row is culled, its index past the count.
    function drawn() {
        var out = []
        var items = root.delegates().filter(function (d) { return d.visible && d.index < root.lv.count }).sort(function (a, b) { return a.y - b.y || a.x - b.x })
        for (var i = 0; i < items.length; i++) {
            var texts = []
            root.walk(items[i], function (item) { if (item.visible && item.text !== undefined && String(item.text).length > 0) texts.push(String(item.text)) })
            out.push(texts.join("|"))
        }
        return out
    }
    function answered() { return root.pane.searchGen === root.gen && !root.pane.searchRunning && root.pane.searchSettledAt >= root.pane.searchKeyAt }
    // Every drawn directory row a view sizes shows its answer, which lands after the folder's rows.
    function sizesLanded() {
        if (!DirSizes.wantsSizes(root.pane.viewMode, Flea.ViewState.hiddenCols, root.pane.storageClass, root.pane.storageKnown)) return true
        return root.delegates().every(function (d) {
            var row = d.visible && d.index < root.lv.count && d.listingIndex >= 0 ? root.pane.rowFor(d.listingIndex) : null
            return row === null || !row.d || DirSizes.sizeFor(root.pane.dirSizeState, d.listingIndex) !== null
        })
    }

    FloatingWindow {
        id: win
        implicitWidth: 1400
        implicitHeight: 800
        color: Flea.Theme.color.background
        Item { id: probe; readonly property var qwin: Window.window }
    }
    Flea.Backend { id: realBackend }
    Component {
        id: paneComponent
        Flea.Pane {
            width: 1400
            height: 800
            backend: realBackend
        }
    }

    Connections {
        target: probe.qwin
        // The basic loop swaps before the turn the warm work is queued to, so this sees the strip's frame alone.
        function onFrameSwapped() {
            if (root.stripFrame === null && root.pane && root.pane.searchMode === "typing")
                root.stripFrame = root.drawn()
            if (root.rowsMoved) {
                root.rowsMoved = false
                root.answerFrames += 1
            }
        }
        // The first frame after the answer landed: what that frame built is what the answer cost.
        function onAfterAnimating() {
            if (root.answerFrame === null && root.pane && root.gen > 0 && root.pane.searchFirstRowsAt > 0 && root.pane.searchGen === root.gen) {
                var built = root.census()
                built.loaders = root.flips
                root.unwatchLoaders()
                root.answerFrame = built
            }
        }
    }

    Connections {
        target: root.pane
        function onRowsChanged() {
            if (!root.counting) return
            root.answerLands += 1
            root.rowsMoved = true
        }
    }

    Connections {
        target: realBackend
        function onSearched(total, scanned, ms, cancelled, gen, walking) {
            if (root.counting) root.answers = root.answers.concat([{ gen: gen, walking: walking, after: Date.now() - root.keySentAt }])
        }
    }

    Timer {
        interval: root.tickMs
        running: true
        repeat: true
        onTriggered: {
            if (root.ticking) return
            root.ticking = true
            try { root.tick() } finally { root.ticking = false }
        }
    }

    function tick() {
        if (root.finished) {
            if (root.pane === null) Qt.exit(root.failures > 0 ? 1 : 0)
            return
        }
        if (Date.now() - root.stamp > root.giveUpMs) {
            root.check("probe completes", "timeout at op " + root.at + " " + JSON.stringify(root.ops[root.at - 1]), "complete")
            root.finish()
            return
        }
        if (root.awaiting !== null && !root.awaiting()) return
        root.awaiting = null
        if (root.at >= root.ops.length) {
            root.finish()
            return
        }
        root.stamp = Date.now()
        root.run(root.ops[root.at])
        root.at += 1
    }

    function run(op) {
        if (op.drop) {
            // The next view starts on a fresh pane once this one is gone, so nothing it queued runs beside the next.
            console.log("SEARCHWARM NOTE dropping the " + root.pane.viewMode + " pane")
            root.pane.destroy()
            root.awaiting = function () { return root.pane === null }
        } else if (op.view !== undefined) {
            root.seen = {}
            Flea.ViewState.changeKey("view", op.view)
            root.pane = paneComponent.createObject(win.contentItem)
            root.keys = Qt.createQmlObject("import QtTest; TestEvent {}", root.pane.listArea)
            root.pane.open(root.fixture)
            var still = { frame: "", since: 0 }
            // The folder's sizes and icons land after its rows, so the strip opens only once every drawn folder shows its size and the drawn rows then stood still for the warm span.
            root.awaiting = function () {
                if (root.pane.listInFlight || root.pane.total === 0 || root.pane.viewMode !== op.view) return false
                root.lv = op.view === "columns" ? (root.pane.columnsArea ? root.pane.columnsArea.activeColumn().viewport : null) : root.pane.listArea
                if (root.lv === null || root.delegates().length === 0 || !root.sizesLanded()) return false
                var frame = JSON.stringify(root.drawn())
                if (frame !== still.frame) { still.frame = frame; still.since = Date.now() }
                return Date.now() - still.since >= root.warmMs
            }
        } else if (op.start) {
            root.stripFrame = null
            root.pane.listArea.forceActiveFocus()
            root.keys.keyClickChar("f", Qt.NoModifier, -1)
            var warmAt = 0
            root.awaiting = function () {
                if (root.stripFrame === null || !root.pane.swap.searchWarm) return false
                if (warmAt === 0) warmAt = Date.now()
                // An absence, judged from the warm turn itself: it fills the rows in that turn, and the window holds whatever it queued.
                if (Date.now() - warmAt < root.warmMs) return false
                var view = root.pane.viewMode
                root.check(view + " the folder lists fewer rows than the view holds", root.pane.total < root.pane.visibleRows, true)
                root.check(view + " the warm turn draws nothing the strip's frame did not", root.drawn(), root.stripFrame)
                root.census()
                return true
            }
        } else if (op.key !== undefined) {
            var view = root.pane.viewMode
            root.answerFrame = null
            root.gen = root.pane.searchGen + 1
            root.watchLoaders()
            root.keys.keyClickChar(op.key, Qt.NoModifier, -1)
            var after = 0
            root.awaiting = function () {
                if (!root.answered() || root.answerFrame === null) return false
                if (after === 0) {
                    root.check(view + " the answer fills the view", root.pane.total > root.pane.visibleRows, true)
                    root.check(view + " the first answer's frame builds no row and no layout, destroys none and no Loader flips",
                               root.answerFrame, { rows: 0, items: 0, gone: 0, loaders: 0 })
                    after = Date.now()
                }
                // What the view still builds in the frames after, its cache buffer's rows past a full layout, is said and not judged.
                if (Date.now() - after < root.warmMs) return false
                console.log("SEARCHWARM NOTE " + view + " built after the answer's frame " + JSON.stringify(root.census()))
                return true
            }
        } else if (op.firstkey !== undefined) {
            // The key rides behind f in one turn, so the backend reads it while the session's walk most likely still runs.
            root.answerLands = 0
            root.answerFrames = 0
            root.rowsMoved = false
            root.answers = []
            root.counting = true
            root.pane.listArea.forceActiveFocus()
            root.keys.keyClickChar("f", Qt.NoModifier, -1)
            root.gen = root.pane.searchGen + 1
            root.keySentAt = Date.now()
            root.keys.keyClickChar(op.firstkey, Qt.NoModifier, -1)
            var settled = 0
            root.awaiting = function () {
                if (!root.answered()) return false
                if (settled === 0) settled = Date.now()
                // An absence, judged from the settled answer: a second answer for this key would arrive behind it.
                if (Date.now() - settled < root.warmMs) return false
                root.counting = false
                var gens = root.answers.map(function (a) { return a.gen === root.gen })
                var walking = root.answers.map(function (a) { return a.walking })
                console.log("SEARCHWARM NOTE first key answers " + JSON.stringify(root.answers))
                // A key held to the landing and a key read after it write the same lines, so this pins one answer, not the hold; run_hold_tests.rs pins that.
                if (walking[0] === true) {
                    // A walk past the cap is a slow box, not a defect, once the partial answer came no sooner than the cap.
                    console.log("SEARCHWARM NOTE the walk outlasted the cap, so the cap answered first and the landing completed it")
                    root.check("past the cap the partial answer waits the cap, then the landing completes it",
                               [root.answerLands, walking, gens, root.answers[0].after >= Swap.HOLD_MS], [2, [true, false], [true, true], true])
                } else {
                    root.check("a first key answered after the walk landed lands one answer in one frame, the landed walk's",
                               [root.answerLands, root.answerFrames, walking, gens], [1, 1, [false], [true]])
                }
                root.check("that answer is the whole walk's", [root.pane.searchQuery, root.pane.total], [op.firstkey, 150])
                return true
            }
        } else if (op.exit) {
            root.keys.keyClick(Qt.Key_Escape, Qt.NoModifier, -1)
            root.awaiting = function () { return root.pane.searchMode.length === 0 && !root.pane.listInFlight }
        }
    }
}
