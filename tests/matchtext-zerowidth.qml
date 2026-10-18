//@ pragma ShellId flea-matchtext-zerowidth-test

import QtQuick
import Quickshell
import "flea" as Flea
import "flea/js/Focus.js" as Focus
import "matchtext-zerowidth.js" as Probe

// A late match whose head fills a MatchText slot never draws a zero-width run over the cells after the name. The Columns view, the list, a search row and a row with no match each get a geometry read of every drawn run and a pixel compare of the cells right of the name, against the same row without the match. The late keystroke goes through the real Focus and Filter path. Stub pane: tests/matchtext-pane.qml.
ShellRoot {
    id: root

    readonly property string longName: "This is a visual test for where the : symbol is searched.md"
    readonly property int stamp: 1758835200
    // The channel difference a pixel may move between two grabs of one drawing.
    readonly property int tolerance: 2
    property var failures: []
    property var steps: []
    property var snaps: ({})
    property var grabs: ({})
    property string pendingTag: ""
    property string pendingUrl: ""
    property string grabDir: ""
    // The config directory's parent name: green or revert.
    property string configName: ""
    property bool finished: false

    readonly property var longRow: ({ n: root.longName, d: false, i: "text-x-generic", p: 420, s: 13, m: root.stamp, t: false, k: 0 })
    readonly property var searchRow: ({ n: "docs/" + root.longName, d: false, i: "text-x-generic", p: 420, s: 13, m: root.stamp, t: false, k: 0 })
    readonly property var fixtureRows: [
        root.longRow,
        { n: "notes.txt", d: false, i: "text-x-generic", p: 420, s: 2048, m: root.stamp, t: false, k: 0 },
        { n: "photo.png", d: false, i: "text-x-generic", p: 420, s: 4096, m: root.stamp, t: false, k: 0 }
    ]

    Component {
        id: backendStub
        QtObject {
            signal peeked(string path, bool hidden, int total, var rows, bool readFailed, int mode, bool hiddenLast, int first)
            signal metaResult(var message)
            signal codeResult(var message)
            signal changed(string path)
            signal shebang(string path, bool hasShebang, int id)
            property int dirDev: 0
            function peek(path, size, hidden) {}
            function askMeta(index, wantLines, wantMedia, wantArchive) { return 0 }
            function thumb(rows, cacheOnly) {}
            function thumbcancel(rows) {}
            function dirsize(rows) {}
            function dirsizecancel() {}
            function window(start, count) {}
        }
    }
    Component { id: paneStub; MatchtextPane {} }
    Component { id: menuStub; QtObject { function close() {} function openBackground(point) {} } }
    Component { id: statusStub; QtObject { property var activities: [] } }

    property var stubBackend: backendStub.createObject(root)
    property var stubMenu: menuStub.createObject(root)
    property var stubStatus: statusStub.createObject(root)
    // The Columns view and the list read one pane, so one filter key narrows both.
    property var shared: paneStub.createObject(root, { backend: root.stubBackend, statusBar: root.stubStatus, rows: root.fixtureRows })
    // The rows that do not share it: none holds a query no name carries, the search pair answers by query.
    property var nonePlain: paneStub.createObject(root, { backend: root.stubBackend, statusBar: root.stubStatus })
    property var noneQuery: paneStub.createObject(root, { backend: root.stubBackend, statusBar: root.stubStatus, filterQuery: "zq" })
    property var searchEarly: paneStub.createObject(root, { backend: root.stubBackend, statusBar: root.stubStatus, searchMode: "files", swap: ({ answeredQuery: "This", searchWarm: true }) })
    property var searchLate: paneStub.createObject(root, { backend: root.stubBackend, statusBar: root.stubStatus, searchMode: "files", swap: ({ answeredQuery: ":", searchWarm: true }) })

    FloatingWindow {
        id: win
        implicitWidth: 1000
        implicitHeight: 560
        color: Flea.Theme.color.background

        // The Columns view: the active column draws the shared listing's rows as ColumnRows.
        Flea.ColumnsArea {
            id: area
            x: 0
            y: 0
            width: 720
            height: 200
            pane: root.shared
            menu: root.stubMenu
        }

        // The list view: each row draws its name as a MatchText through the same shared pane.
        Flea.List {
            id: list
            x: 0
            y: 210
            width: 240
            height: 120
            pane: root.shared
            menu: root.stubMenu
        }

        Item {
            id: rowsBox
            x: 0
            y: 340
            width: 260
            height: 4 * Flea.Theme.fileRowHeight

            Flea.ColumnRow {
                id: nonePlainRow
                y: 0
                width: rowsBox.width
                height: Flea.Theme.fileRowHeight
                row: root.longRow
                pane: root.nonePlain
                showSize: true
            }
            Flea.ColumnRow {
                id: noneQueryRow
                y: Flea.Theme.fileRowHeight
                width: rowsBox.width
                height: Flea.Theme.fileRowHeight
                row: root.longRow
                pane: root.noneQuery
                showSize: true
            }
            Flea.ColumnRow {
                id: searchEarlyRow
                y: 2 * Flea.Theme.fileRowHeight
                width: rowsBox.width
                height: Flea.Theme.fileRowHeight
                row: root.searchRow
                pane: root.searchEarly
                showSize: true
            }
            Flea.ColumnRow {
                id: searchLateRow
                y: 3 * Flea.Theme.fileRowHeight
                width: rowsBox.width
                height: Flea.Theme.fileRowHeight
                row: root.searchRow
                pane: root.searchLate
                showSize: true
            }
        }

        // Grab readback: the saved grab is loaded into the Canvas itself and read with getImageData. It stays invisible.
        Canvas {
            id: canvas
            x: 0
            y: 500
            opacity: 0
            onImageLoaded: canvas.requestPaint()
            onPaint: {
                if (root.pendingTag === "" || !canvas.isImageLoaded(root.pendingUrl)) return
                var ctx = canvas.getContext("2d")
                ctx.drawImage(root.pendingUrl, 0, 0)
                var g = root.grabs
                g[root.pendingTag] = { w: canvas.width, h: canvas.height, px: ctx.getImageData(0, 0, canvas.width, canvas.height).data }
                root.grabs = g
                root.pendingTag = ""
                root.pendingUrl = ""
                // Deferred, so the next grab never starts inside this paint.
                Qt.callLater(root.next)
            }
        }
    }

    Timer { id: waiter; interval: 1; repeat: false; onTriggered: root.next() }
    Timer { id: watchdog; interval: 30000; running: true; repeat: false; onTriggered: { root.fail("harness: no result within 30 s"); root.finish() } }

    function fail(text) { root.failures.push(text) }
    function wait(ms) { waiter.interval = ms; waiter.restart() }

    // Every key goes through the product route; the stub recomputes the shared listing behind it.
    function press(key, text) {
        Focus.handleKey({ key: key, text: text, modifiers: Qt.NoModifier }, root.shared, null)
        root.shared.refresh()
    }
    function type(text) {
        for (var i = 0; i < text.length; i++) root.press(text.charCodeAt(i), text.charAt(i))
    }

    // Grabs item. The probe, if any, reads the drawing after the frame that grabbed it.
    function grab(item, tag, probe) {
        item.grabToImage(function (result) {
            if (probe) {
                var s = root.snaps
                s[tag] = probe()
                root.snaps = s
            }
            var file = root.grabDir + "/" + tag + ".png"
            if (!result.saveToFile(file)) {
                root.fail("harness: " + tag + " did not save")
                root.next()
                return
            }
            root.pendingTag = tag
            root.pendingUrl = "file://" + file
            // The grab is the item at 1x, so the canvas takes the item's size before the file loads.
            canvas.width = Math.round(item.width)
            canvas.height = Math.round(item.height)
            canvas.loadImage(root.pendingUrl)
        })
    }

    function next() {
        if (root.finished) return
        var step = root.steps.shift()
        if (step === undefined) { root.finish(); return }
        step()
    }

    function probeView(view) { return function () { return Probe.markGeom(view, view, root.longName) } }

    function probeRows() {
        var none = Probe.markGeom(noneQueryRow, rowsBox, root.longName)
        var late = Probe.markGeom(searchLateRow, rowsBox, root.longName)
        var early = Probe.markGeom(searchEarlyRow, rowsBox, root.longName)
        return {
            noneQuery: none,
            noneBase: Probe.plainBox(nonePlainRow, rowsBox, none.x0),
            searchLate: late,
            searchEarly: early,
            searchEarlyBox: Probe.plainBox(searchEarlyRow, rowsBox, late.x0)
        }
    }

    // One case: its run geometry and its pixel result (null for a baseline, which compares nothing).
    function verdict(name, geom, pixels) {
        var reasons = geom === undefined ? ["not captured"] : geom.fails.slice()
        if (pixels === -1 || pixels === -2) reasons.push("pixels not captured")
        else if (pixels !== null && pixels > 0) reasons.push("pixels differ at " + pixels)
        var info = (geom === undefined || geom.info === undefined ? "" : " " + geom.info) + " config=" + root.configName
        if (reasons.length === 0) {
            console.log("MATCHTEXT PASS " + name + (pixels === null ? " pixels=baseline" : " pixels-differ=0") + info)
            return true
        }
        console.log("MATCHTEXT FAIL " + name + " " + reasons.join("; ") + info)
        return false
    }

    // Each case compares its late or early grab with the baseline grab of the same view, cell for cell right of the name.
    function finish() {
        if (root.finished) return
        root.finished = true
        var s = root.snaps, g = root.grabs, t = root.tolerance
        var cases = 0, passed = 0
        function run(name, geom, pixels) { cases += 1; if (root.verdict(name, geom, pixels)) passed += 1 }
        // The box of ref, the row as the late drawing has it, compared against the same box in grab tag.
        function pixelsOf(base, ref, tag, geom) {
            if (ref === undefined) return -1
            var box = Probe.boxOf(ref)
            return Probe.diff(g[base], box, g[tag], geom === undefined ? box : Probe.boxOf(geom), t)
        }
        var la = s["area-late"], ea = s["area-early"], ll = s["list-late"], el = s["list-early"], snap = s["rows"]
        run("columns-late", la, pixelsOf("area-base", la, "area-late", la))
        run("columns-early", ea, pixelsOf("area-base", la, "area-early", ea))
        run("list-late", ll, pixelsOf("list-base", ll, "list-late", ll))
        run("list-early", el, pixelsOf("list-base", ll, "list-early", el))
        if (snap === undefined) {
            run("nomatch", undefined, -1)
            run("search-late", undefined, -1)
            run("search-early", undefined, -1)
        } else {
            run("nomatch", snap.noneQuery, Probe.diff(g.rows, snap.noneBase, g.rows, Probe.boxOf(snap.noneQuery), t))
            run("search-late", snap.searchLate, Probe.diff(g.rows, snap.searchEarlyBox, g.rows, Probe.boxOf(snap.searchLate), t))
            run("search-early", snap.searchEarly, null)
        }
        // Positive control for the pixel path: the late box shifted 40 px left must read a difference, or the readback is blind.
        if (la !== undefined) {
            var box = Probe.boxOf(la)
            var shifted = { x0: box.x0 - 40, x1: box.x1 - 40, y0: box.y0, y1: box.y1 }
            var det = Probe.diff(g["area-base"], box, g["area-base"], shifted, t)
            cases += 1
            if (det > 0) {
                passed += 1
                console.log("MATCHTEXT PASS pixels-detect shifted-differ=" + det + " config=" + root.configName)
            } else {
                console.log("MATCHTEXT FAIL pixels-detect shift of 40 px read " + det + " differing pixels, the readback is blind config=" + root.configName)
            }
        }
        for (var f = 0; f < root.failures.length; f++) console.log("MATCHTEXT FAIL harness " + root.failures[f])
        console.log("MATCHTEXT DONE config=" + root.configName + " cases=" + cases + " passed=" + passed + " failed=" + (cases - passed + root.failures.length))
        Quickshell.execDetached(["kill", String(Quickshell.processId)])
    }

    // The steps run one after another. Each grab resumes the chain once its frame is saved and drawn.
    Component.onCompleted: {
        root.grabDir = String(Qt.resolvedUrl(".")).replace("file://", "").replace(/\/$/, "")
        root.configName = root.grabDir.split("/").slice(-2, -1)[0]
        root.steps = [
            function () { root.wait(500) },
            function () { root.grab(rowsBox, "rows", root.probeRows) },
            function () { root.grab(area, "area-base", null) },
            function () { root.grab(list, "list-base", null) },
            function () {
                root.press(Qt.Key_Slash, "/")
                root.type(":")
                if (root.shared.filterQuery !== ":") root.fail("harness: typing reached " + root.shared.filterQuery + ", want :")
                root.grab(area, "area-late", root.probeView(area))
            },
            function () { root.grab(list, "list-late", root.probeView(list)) },
            function () {
                root.press(Qt.Key_Escape, "")
                if (root.shared.filterQuery !== "") root.fail("harness: escape kept query " + root.shared.filterQuery)
                root.press(Qt.Key_Slash, "/")
                root.type("This")
                if (root.shared.filterQuery !== "This") root.fail("harness: typing reached " + root.shared.filterQuery + ", want This")
                root.grab(area, "area-early", root.probeView(area))
            },
            function () { root.grab(list, "list-early", root.probeView(list)) },
            function () { root.finish() }
        ]
        waiter.restart()
    }
}
