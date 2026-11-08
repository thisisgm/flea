//@ pragma ShellId flea-langtoggle-test
import QtQuick
import Quickshell
import Quickshell.Io
import "flea" as Flea
import "flea/js/Icons.js" as Icons
import "langmark-count.js" as LangCost

// Real WindowBody windows and a second process sharing the same state file.
ShellRoot {
    id: root
    readonly property string leg: Quickshell.env("LANGTOGGLE_LEG")
    readonly property bool peer: root.leg === "peer"
    readonly property bool again: root.leg === "again" || root.leg === "dual"
    readonly property bool dual: root.leg === "dual"
    readonly property bool savedOff: root.leg === "saved-off"
    property int phase: 0
    property int failures: 0
    property int checks: 0
    property double since: Date.now()
    property bool finished: false
    property var panes: []
    property var listingCounts: []
    property var walkCounts: []
    property var plain: []
    property int peerFlips: 0
    property bool peerState: true
    property var windows: []
    property int retainedStage: 0
    property bool menuControlsDone: false

    function check(ok, name) {
        checks++
        if (!ok) { failures++; console.log("LANGTOGGLE FAIL " + name) }
    }
    function next() { phase++; since = Date.now() }
    function finish() {
        if (finished) return
        finished = true
        console.log("LANGTOGGLE DONE leg=" + leg + " checks=" + checks + " failures=" + failures)
        quitter.start()
    }
    function objects() {
        var out = []
        for (var i = 0; i < windows.length; i++)
            if (windows[i].body) out = out.concat(LangCost.census(windows[i].body, true).objects)
        return out
    }
    function bar() {
        var bars = root.objects().filter(function (item) { return typeof item.closeState === "function" && item.langSlug !== undefined })
        return bars.length ? bars[0] : null
    }
    function codeRows() {
        return root.objects().filter(function (item) {
            return item.langSlug !== undefined && item.row && item.row.n === "a.rs"
        })
    }
    function ready() {
        if (windows.length !== (peer ? 1 : 3) || windows.some(function (window) { return !window.body })) return false
        var found = root.objects().filter(function (item) { return /^Pane_/.test(String(item)) })
        if (found.length !== (peer ? 1 : dual ? 6 : 3)) return false
        for (var i = 0; i < found.length; i++)
            if (found[i].listingState !== "ready" || found[i].listInFlight || found[i].total !== 2) return false
        panes = found
        return true
    }
    function steady() {
        return Flea.ViewState.saveStatus === "Saved" && JSON.stringify(Flea.ViewState.unsaved) === "{}" && Date.now() - since >= 700
    }
    function snapshot() {
        listingCounts = panes.map(function (pane) { return pane.backend.listRequests })
        walkCounts = panes.map(function (pane) { return pane.backend.langtoggleWalks })
        plain = codeRows().map(function (row) {
            return { row: row, objects: LangCost.census(row, false).total, full: LangCost.census(row, true).total,
                labelX: LangCost.state(row).labelX }
        })
    }
    function unchanged() {
        for (var i = 0; i < panes.length; i++) {
            check(panes[i].backend.listRequests === listingCounts[i], "toggle re-lists pane " + i)
            check(panes[i].backend.langtoggleWalks === walkCounts[i], "toggle walks a directory in pane " + i)
        }
        for (var j = 0; j < plain.length; j++)
            check(LangCost.state(plain[j].row).labelX === plain[j].labelX, "toggle moves the filename column")
    }
    function slots(on, label) {
        check(Flea.ViewState.langMarks === on, label + " setting")
        var rows = codeRows(), types = {}
        check(rows.length > 0, label + " has no code-row control")
        for (var i = 0; i < rows.length; i++) {
            var row = rows[i], state = LangCost.state(row)
            types[String(row).split("_")[0]] = true
            check(on ? state.built && state.slug === "rust" && state.markVisible === true && state.glyphVisible === false
                     : row.langLoader === null && state.glyphVisible && state.glyphName === Icons.glyphForRow(row.row.i, row.row.p),
                label + " glyph slot " + String(row))
            if (!on) check(LangCost.languageObjects(LangCost.census(row, true)) === 0, label + " retains row mark objects")
        }
        if (panes.some(function (pane) { return pane.viewMode === "list" }))
            check(types.Row === true, label + " missing list control")
        if (panes.some(function (pane) { return pane.viewMode === "grid" }))
            check(types.GridTile === true, label + " missing grid control")
        if (panes.some(function (pane) { return pane.viewMode === "columns" }))
            check(types.ColumnRow === true, label + " missing columns control")
        var strip = bar()
        if (!peer) {
            check(strip !== null, label + " missing Quick Look bar")
            if (strip) {
                var geometry = strip.geometry()
                check(on ? geometry.language !== null && geometry.language.slug === "rust" && geometry.language.visible === true && geometry.mark.visible === false
                         : strip.langLoader === null && geometry.mark.visible && geometry.mark.name === strip.markName,
                    label + " Quick Look slot")
                if (!on) check(LangCost.languageObjects(LangCost.census(strip, true)) === 0, label + " retains Quick Look mark objects")
            }
        }
        if (!on) for (var w = 0; w < windows.length; w++)
            check(LangCost.languageObjects(LangCost.census(windows[w].body, true)) === 0, label + " retains any mark objects")
    }
    function menuIndices(host) {
        var rows = host.entries, hidden = -1, code = -1
        for (var i = 0; i < rows.length; i++) {
            if (rows[i].action === "toggleHidden") hidden = i
            if (rows[i].action === "toggleLangMarks") code = i
        }
        return {hidden: hidden, code: code}
    }
    function menuOrder(host) {
        var indices = menuIndices(host)
        return indices.hidden >= 0 && indices.code === indices.hidden + 1
    }
    function menuControls(host) {
        var held = host.entries, at = menuIndices(host).code
        if (at < 0) { check(false, "missing-anchor control has no code-icons row to move"); return }
        var code = held[at]
        host.entries = [code].concat(held.filter(function(row) { return row.action !== "toggleHidden" && row.action !== "toggleLangMarks" }))
        var indices = menuIndices(host)
        check(indices.hidden === -1 && indices.code === 0 && indices.code === indices.hidden + 1,
            "missing-anchor control does not exercise the original false positive")
        check(menuOrder(host) === false, "menu reader accepts a missing hidden-files anchor")
        console.log("LANGTOGGLE CONTROL missing-anchor hidden=" + indices.hidden + " code=" + indices.code
            + " originalAccepted=true correctedAccepted=" + menuOrder(host))
        host.entries = held
        check(menuOrder(host), "missing-anchor control did not restore the menu")
    }
    function backgroundTranscript(host) {
        var readers = objects().filter(function(object) {
            return object.seam && object.pane && typeof object.pane.contextMenu === "function" && object.pane.contextMenu() === host
        })
        check(readers.length === 1, "background transcript has no unique real IPC reader")
        if (readers.length === 1) console.log("LANGTOGGLE BACKGROUND_IPC entries=" + readers[0].seam.contextMenuEntries()
            + " glyphs=" + readers[0].seam.contextMenuGlyphs())
    }
    // The shipped list takes both view rows off the file menu, and the background keeps them, so every toggle goes through the background.
    function menu(on, selected, activate) {
        var host = panes[1].contextMenu()
        if (selected) {
            host.openAt(Qt.point(100, 100))
            var filed = menuIndices(host)
            check(host.opened && filed.hidden === -1 && filed.code === -1, "file menu shows hidden files or code icons by default")
            host.close()
        }
        host.openBackground(Qt.point(100, 100))
        var rows = host.entries, code = menuIndices(host).code
        check(menuOrder(host), "menu position immediately after hidden files")
        if (code >= 0) check(rows[code].label === (on ? "Hide code icons" : "Show code icons")
            && rows[code].glyph === "code" && rows[code].checked === undefined, "menu label and existing glyph")
        if (leg === "fresh" && !menuControlsDone) {
            backgroundTranscript(host)
            menuControls(host)
            menuControlsDone = true
        }
        if (activate === false) host.close()
        else host.choose("toggleLangMarks")
        check(!host.opened, "menu activation closes the menu")
    }
    function added() {
        for (var i = 0; i < plain.length; i++) {
            var before = plain[i], after = LangCost.census(before.row, false), full = LangCost.census(before.row, true)
            check(after.total - before.objects === 5 && full.total - before.full === 8, "enabled mark object count")
            console.log("LANGTOGGLE OBJECTS " + String(before.row).split("_")[0] + " off=" + before.objects
                + " on=" + after.total + " added=" + (after.total - before.objects) + " fullAdded=" + (full.total - before.full))
        }
    }
    function sharedMenu(on) {
        var rows = panes[0].contextMenu().entries
        var code = rows.filter(function (row) { return row.action === "toggleLangMarks" })[0]
        check(code && code.label === (on ? "Hide code icons" : "Show code icons"), "shared open menu label")
    }
    function tick() {
        if (finished) return
        if (Date.now() - since > 15000) { check(false, "phase " + phase + " timed out"); finish(); return }
        if (phase === 0) {
            if (!ready()) return
            if (!dual) for (var w = 0; w < windows.length; w++)
                windows[w].body.currentPane.open(Quickshell.env("LANGTOGGLE_FIXTURE") + "/" + (peer ? "columns" : ["list", "grid", "columns"][w]))
            for (var p = 0; p < panes.length; p++) panes[p].setCursor(0)
            if (!peer) windows[0].body.currentPane.act("preview")
            next()
        } else if (phase === 1) {
            var firstBody = windows[0].body
            if (!ready() || !steady() || (!peer && (!firstBody.quickLook || !firstBody.quickLook.lookReady || !bar()))) return
            if (leg === "fresh" && retainedStage < 2) {
                if (retainedStage === 1) check(LangCost.census(firstBody.currentPane, true).types.GridTile > 0,
                    "view-switch control never built grid delegates")
                firstBody.currentPane.chooseView(retainedStage === 0 ? "grid" : "list")
                retainedStage++
                since = Date.now()
                return
            }
            if (leg === "fresh") {
                var retained = LangCost.census(firstBody.currentPane, true)
                check(retained.types.GridArea === 1 && !retained.types.GridTile, "hidden grid retains delegates")
                console.log("LANGTOGGLE RETAINED gridViews=" + retained.types.GridArea + " gridTiles="
                    + (retained.types.GridTile || 0) + " codeRows=" + codeRows().length)
            }
            slots(!savedOff, savedOff ? "saved-off" : again ? "relaunch-on" : "default-on")
            console.log("LANGTOGGLE " + (again ? "RELAUNCH" : "DEFAULT") + " on=" + Flea.ViewState.langMarks)
            snapshot()
            if (savedOff) { menu(false, false, false); finish(); return }
            if (peer) { panes[0].contextMenu().openBackground(Qt.point(100, 100)); sharedMenu(true) }
            else menu(true)
            next()
        } else if (peer) {
            var now = Flea.ViewState.langMarks
            if (now === peerState) return
            peerState = now
            peerFlips++
            slots(now, "shared-window")
            sharedMenu(now)
            unchanged()
            console.log("LANGTOGGLE SHARED flip=" + peerFlips + " on=" + now)
            since = Date.now()
            if (peerFlips === 4) finish()
        } else if (!steady()) {
            return
        } else if (again) {
            slots(false, "relaunch-off")
            unchanged()
            finish()
        } else if (phase === 2) {
            slots(false, "live-off")
            unchanged()
            snapshot()
            menu(false, true)
            next()
        } else if (phase === 3) {
            slots(true, "live-on")
            added()
            unchanged()
            menu(true)
            next()
        } else if (phase === 4) {
            slots(false, "released-off")
            unchanged()
            for (var i = 0; i < plain.length; i++)
                check(LangCost.census(plain[i].row, false).total === plain[i].objects, "off restores the delegate object baseline")
            snapshot()
            menu(false)
            next()
        } else {
            slots(true, "columns-on")
            added()
            unchanged()
            finish()
        }
        if (failures > 0) finish()
    }

    Component {
        id: windowFactory
        FloatingWindow {
            id: window
            readonly property alias body: body.item
            implicitWidth: 1200
            implicitHeight: 760
            Loader {
                id: body
                anchors.fill: parent
                source: "flea/WindowBody.qml"
                onLoaded: item.host = window
                onStatusChanged: if (status === Loader.Error) { root.check(false, "WindowBody failed to load"); root.finish() }
            }
        }
    }
    Component.onCompleted: {
        var held = Flea.ViewState.state
        Flea.ViewState.state = ({})
        root.check(Flea.ViewState.langMarks, "absent QML preference defaults on")
        var invalid = [null, "false", 1, [], {}]
        for (var v = 0; v < invalid.length; v++) {
            Flea.ViewState.state = ({langMarks: invalid[v]})
            root.check(Flea.ViewState.langMarks, "invalid QML preference heals on " + v)
        }
        Flea.ViewState.state = ({langMarks: false})
        root.check(!Flea.ViewState.langMarks, "saved QML false stays off")
        Flea.ViewState.state = held
        var made = []
        for (var i = 0; i < (peer ? 1 : 3); i++) made.push(windowFactory.createObject(root))
        windows = made
    }
    Timer {
        interval: 50; running: true; repeat: true
        onTriggered: {
            try { root.tick() }
            catch (error) { root.check(false, "harness exception: " + error); root.finish() }
        }
    }
    Timer { id: quitter; interval: 100; onTriggered: Quickshell.execDetached(["kill", String(Quickshell.processId)]) }
}
