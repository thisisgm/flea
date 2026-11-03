//@ pragma ShellId flea-thumbs-qlprefetch-test

import Quickshell
import QtQuick
import "flea/js/Thumbs.js" as Thumbs
import "flea/js/ExtThumbs.js" as ExtThumbs

// Quick Look's bounded prefetch through the stub backend: one cache-only ask for the next
// row per settled rest, none while a held key is still bursting.
ShellRoot {
    id: shell

    readonly property string uiDir: Quickshell.env("PREVIEW_UI")
    property var failures: []
    property int burstMark: -1
    property int metaMark: -1
    // Null unless Preview exposes exactly one follow settle timer.
    property var settle: null

    function log(line) { console.log("THUMBPREFETCH " + line) }
    function quit() { Quickshell.execDetached(["kill", String(Quickshell.processId)]) }
    function check(name, cond) {
        if (!cond) failures.push(name)
        log((cond ? "PASS " : "FAIL ") + name)
    }
    function done() {
        log("DONE failures=" + failures.length + (failures.length > 0 ? " [" + failures.join(",") + "]" : ""))
        shell.quit()
    }

    QtObject {
        id: backend
        property var calls: []
        property var metaCalls: []
        property var metaShown: []
        property var metaSettling: []
        property int dirDev: 0
        signal meta(int row, int w, int h, int orient, real durationMs, int sampleRate, int entries, real unpacked, bool archiveFailed, var names, real lines, bool partial, bool linesFailed, string target, bool targetDir, string owner)
        signal thumbed(int row, string file)
        // Each ask records the overlay path and the follow settle state.
        function askMeta(index, wantText, wantMedia, wantArchive) {
            metaCalls.push(index)
            metaShown.push(quick.item.path)
            metaSettling.push(shell.settle ? shell.settle.running : true)
            return 0
        }
        function thumb(ask, cacheOnly) {
            calls.push({ ask: ask, cacheOnly: cacheOnly === true, shown: quick.item.path, settling: shell.settle ? shell.settle.running : true })
        }
        function thumbcancel(rows) {}
    }

    QtObject {
        id: pane
        property string path: "/t"
        property int cursorIndex: 0
        property var rows: [
            { n: "a.jpg", d: false, i: "image-x-generic", p: 33188, s: 100, m: 1000, t: true, k: 0 },
            { n: "b.jpg", d: false, i: "image-x-generic", p: 33188, s: 101, m: 1001, t: true, k: 0 },
            { n: "c.jpg", d: false, i: "image-x-generic", p: 33188, s: 102, m: 1002, t: true, k: 0 },
            { n: "d.jpg", d: false, i: "image-x-generic", p: 33188, s: 103, m: 1003, t: true, k: 0 }
        ]
        property var kindNames: []
        property var thumbState: ({ file: { 0: "/cache/a.png" }, order: [0] })
        property string storageClass: ""
        property bool storageKnown: true
        property bool listInFlight: false
        property var backend: backend
        property string searchMode: ""
        property string recentMode: ""
        property string viewMode: "list"
        property string dropPath: "/t"
        property int thumbCap: 240
        property int renamingIndex: -1
        property bool renamePending: false
        property bool menuVisible: false
        property bool filterTyping: false
        property var selectionBand: null
        property var collide: null
        property var menuActions: null
        property var trash: ({ opened: false })
        property var listSlot: ({ x: 0, y: 0, width: 100, height: 100 })
        // The settled view behind Quick Look runs the production planner on this fixture.
        property var listArea: ({ forceActiveFocus: function () {}, restartSettle: function () { shell.settleCalls.push(1); shell.replan() } })
        function rowFor(index) { return (index >= 0 && index < rows.length) ? rows[index] : null }
        function join(base, name) { return base + "/" + name }
        function selectionCount() { return 0 }
    }

    FloatingWindow {
        implicitWidth: 800
        implicitHeight: 600

        Loader {
            id: quick
            anchors.fill: parent
            active: true
            source: "file://" + shell.uiDir + "/Preview.qml"
            onLoaded: {
                item.pane = pane
                shell.findSettle()
                kick.restart()
            }
            onStatusChanged: if (status === Loader.Error) { shell.check("the overlay loads", false); shell.done() }
        }

        // The production wire, so a thumbed reply runs the real onThumbed.
        Loader {
            id: wire
            anchors.fill: parent
            active: true
            source: "file://" + shell.uiDir + "/PaneWire.qml"
            onLoaded: { item.pane = pane }
            onStatusChanged: if (status === Loader.Error) { shell.check("the wire loads", false); shell.done() }
        }
    }

    function cachedOnly() {
        for (var i = 0; i < backend.calls.length; i++)
            if (backend.calls[i].cacheOnly !== true) return false
        return true
    }

    property var settleCalls: []
    property int raceCacheOnly1: 0

    // The follow settle names itself: the rest of a load made at once shares its interval.
    function findSettle() {
        var found = null
        var count = 0
        var list = quick.item ? quick.item.resources : []
        for (var i = 0; i < list.length; i++) {
            var t = list[i]
            if (t && t.objectName === "followSettle") {
                found = t
                count += 1
            }
        }
        shell.settle = count === 1 ? found : null
    }

    // The settled re-plan runs production Thumbs.plan over the full viewport.
    function replan() {
        var work = Thumbs.plan(pane.thumbState, pane.rows, 0, 0, pane.rows.length - 1, "media")
        work.cacheOnly = ExtThumbs.cacheOnly(pane.storageClass, null)
        if (work.drop.length > 0) pane.backend.thumbcancel(work.drop)
        if (work.ask.length > 0) {
            pane.backend.thumb(work.ask, work.cacheOnly)
            pane.thumbState = Thumbs.applied(pane.thumbState, work)
        }
    }

    function cacheOnlyFor(row) {
        var n = 0
        for (var i = 0; i < backend.calls.length; i++) {
            if (backend.calls[i].cacheOnly === true && backend.calls[i].ask.indexOf(row) >= 0) n += 1
        }
        return n
    }

    Timer {
        id: kick
        interval: 200
        repeat: false
        onTriggered: {
            // A settled rest asks the next row's cache entry once, and only cache-only.
            quick.item.open("/t/a.jpg", "image-x-generic", 100, "", "/cache/a.png")
            shell.check("a rest asks once", backend.calls.length === 1)
            shell.check("for the next row", backend.calls.length === 1 && backend.calls[0].ask.join(",") === "1")
            shell.check("cache-only", shell.cachedOnly())
            shell.check("the rest asks its row's meta once", backend.metaCalls.join(",") === "0")
            // A burst an instant later: closed, so neither follow takes a hold; the first
            // loads at once and the second trails it, asking nothing until the settle lands.
            quick.item.close()
            quick.item.lastMoveAt = 0
            quick.item.lastMoveKey = ""
            pane.cursorIndex = 1
            quick.item.follow("/t/b.jpg", "image-x-generic", 101, "", "/cache/b.png")
            pane.cursorIndex = 2
            quick.item.follow("/t/c.jpg", "image-x-generic", 102, "", "/cache/c.png")
            // The trailing follow trails synchronously, so it asks nothing at once either.
            // The first follow loaded at once above, so its row's meta is already asked.
            shell.check("the trailing follow asks nothing at once", backend.calls.length === 2 && shell.settle !== null && shell.settle.running)
            shell.check("and no meta beyond the loaded row", backend.metaCalls.join(",") === "0,1")
            shell.burstMark = backend.calls.length
            shell.metaMark = backend.metaCalls.length
            trailPoll.waited = 0
            trailPoll.restart()
        }
    }

    Timer {
        id: trailPoll
        interval: 50
        repeat: true
        property int waited: 0
        // The trailing rest asks its own next row once, and still cache-only.
        onTriggered: {
            waited += interval
            if (backend.calls.length > shell.burstMark) {
                stop()
                shell.check("nothing asks mid-burst", backend.calls[shell.burstMark].shown === "/t/c.jpg" && backend.calls[shell.burstMark].settling === false)
                shell.check("no meta asks mid-burst", backend.metaShown[shell.metaMark] === "/t/c.jpg" && backend.metaSettling[shell.metaMark] === false)
                shell.check("the trailing rest asks once more", backend.calls.length === shell.burstMark + 1)
                shell.check("for the row after it",
                    backend.calls[backend.calls.length - 1].ask.join(",") === "3")
                shell.check("still cache-only", shell.cachedOnly())
                shell.check("the trailing rest asks its row's meta once",
                    backend.metaCalls.join(",") === "0,1,2")
                // A reply for another row is dropped; the shown row's own reply lands.
                backend.meta(1, 640, 480, 1, 0, 0, 0, 0, false, [], 0, false, false, "", false, "")
                shell.check("a reply for another row is dropped", quick.item.imageW === 0)
                backend.meta(2, 640, 480, 1, 0, 0, 0, 0, false, [], 0, false, false, "", false, "")
                shell.check("its own row's reply lands", quick.item.imageW === 640)
                shell.startRace()
            } else if (waited > 2000) {
                stop()
                shell.check("the trailing rest asks once more", false)
                shell.done()
            }
        }
    }

    // F10: a view plan runs between the prefetch ask and its miss, so the row stays
    // iconless unless the miss re-plans it. Fails before the re-plan: no settle runs.
    function startRace() {
        if (wire.status !== Loader.Ready) {
            shell.check("the wire loads", false)
            shell.done()
            return
        }
        pane.rows = [
            { n: "a.jpg", d: false, i: "image-x-generic", p: 33188, s: 100, m: 1000, t: true, k: 0 },
            { n: "b.jpg", d: false, i: "image-x-generic", p: 33188, s: 101, m: 1001, t: true, k: 0 },
            { n: "c.jpg", d: false, i: "image-x-generic", p: 33188, s: 102, m: 1002, t: true, k: 0 },
            { n: "d.jpg", d: false, i: "image-x-generic", p: 33188, s: 103, m: 1003, t: true, k: 0 }
        ]
        pane.cursorIndex = 0
        pane.thumbState = ({ file: { 0: "/cache/a.png", 2: "/cache/2.png", 3: "/cache/3.png" }, order: [0, 2, 3] })
        shell.settleCalls = []
        shell.raceCacheOnly1 = shell.cacheOnlyFor(1)
        quick.item.close()
        quick.item.open("/t/a.jpg", "image-x-generic", 100, "", "/cache/a.png")
        var prefetch = backend.calls[backend.calls.length - 1]
        shell.check("the race prefetches row 1 cache-only",
            prefetch.ask.join(",") === "1" && prefetch.cacheOnly === true)
        // The interleaving plan sees row 1 defined, so it asks nothing new.
        var before = backend.calls.length
        shell.replan()
        shell.check("a plan between ask and reply asks nothing new", backend.calls.length === before)
        // The miss arrives through the real wire: the row is forgotten and re-planned.
        var settles = shell.settleCalls.length
        backend.thumbed(1, "")
        shell.check("the miss re-plans the row", shell.settleCalls.length === settles + 1)
        var last = backend.calls[backend.calls.length - 1]
        shell.check("and the re-plan asks it in full",
            last.ask.join(",") === "1" && last.cacheOnly !== true)
        // The answer lands; the same rest never prefetches row 1 again.
        backend.thumbed(1, "/cache/1.png")
        shell.check("no second prefetch follows in the same rest",
            shell.cacheOnlyFor(1) === shell.raceCacheOnly1 + 1)
        shell.startF11A()
    }

    // F11A: an insert above the shown file between capture and ask. The follow captures
    // index 0 while bursting; the anchor keeps the file at index 1; the ask re-resolves.
    function startF11A() {
        quick.item.close()
        pane.rows = [{ n: "s.jpg", d: false, i: "image-x-generic", p: 33188, s: 200, m: 2000, t: true, k: 0 }]
        pane.cursorIndex = 0
        pane.thumbState = ({ file: { 0: "/cache/s.png" }, order: [0] })
        shell.f11aMetaMark = backend.metaCalls.length
        // Still bursting, so the follow trails instead of loading at once.
        quick.item.lastMoveAt = Date.now()
        quick.item.lastMoveKey = ""
        quick.item.follow("/t/s.jpg", "image-x-generic", 200, "", "/cache/s.png")
        // The insert lands before the settle does; the cursor follows its file.
        pane.rows = [
            { n: "new.jpg", d: false, i: "image-x-generic", p: 33188, s: 201, m: 2001, t: true, k: 0 },
            { n: "s.jpg", d: false, i: "image-x-generic", p: 33188, s: 200, m: 2000, t: true, k: 0 }
        ]
        pane.cursorIndex = 1
        f11aPoll.waited = 0
        f11aPoll.restart()
    }

    Timer {
        id: f11aPoll
        interval: 50
        repeat: true
        property int waited: 0
        onTriggered: {
            waited += interval
            if (backend.metaCalls.length > shell.f11aMetaMark) {
                stop()
                shell.check("the ask re-resolves to the shown file",
                    backend.metaCalls.slice(shell.f11aMetaMark).join(",") === "1")
                backend.meta(1, 111, 222, 1, 0, 0, 0, 0, false, [], 0, false, false, "", false, "")
                shell.check("the interim takes the shown file's size",
                    quick.item.imageW === 111 && quick.item.interimVisible === true)
                shell.startF11B()
            } else if (waited > 2000) {
                stop()
                shell.check("the ask re-resolves to the shown file", false)
                shell.done()
            }
        }
    }

    // F11B: the insert lands between ask and reply. The reply for the drifted index names the neighbour.
    function startF11B() {
        quick.item.close()
        pane.rows = [{ n: "s.jpg", d: false, i: "image-x-generic", p: 33188, s: 200, m: 2000, t: true, k: 0 }]
        pane.cursorIndex = 0
        pane.thumbState = ({ file: { 0: "/cache/s.png" }, order: [0] })
        var mark = backend.metaCalls.length
        quick.item.open("/t/s.jpg", "image-x-generic", 200, "", "/cache/s.png")
        var asked = backend.metaCalls.slice(mark).join(",") === "0"
        pane.rows = [
            { n: "new.jpg", d: false, i: "image-x-generic", p: 33188, s: 201, m: 2001, t: true, k: 0 },
            { n: "s.jpg", d: false, i: "image-x-generic", p: 33188, s: 200, m: 2000, t: true, k: 0 }
        ]
        pane.cursorIndex = 1
        backend.meta(0, 999, 888, 1, 0, 0, 0, 0, false, [], 0, false, false, "", false, "")
        shell.check("a reply for a drifted index is dropped", asked && quick.item.imageW === 0
            && quick.item.interimVisible !== true)
        shell.f11bMark = mark + 1
        f11bWait.restart()
    }

    // Two follow settles, so a deferred re-ask through it is counted before done.
    Timer {
        id: f11bWait
        interval: shell.f11bWaitMs
        repeat: false
        onTriggered: {
            shell.check("no re-ask follows the drop", backend.metaCalls.length === shell.f11bMark)
            shell.done()
        }
    }

    property int f11aMetaMark: -1
    property int f11bMark: -1
    property int f11bWaitMs: quick.item ? 2 * quick.item.followSettleMs : 0
}
