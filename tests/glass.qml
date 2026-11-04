//@ pragma ShellId flea-glass-test
import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons as Commons
import "flea" as Flea
import "glass.js" as Pixels

// The real WindowBody grabbed over magenta and over green; each ground region must solve to one plane (tests/glass.js).
ShellRoot {
    id: root
    readonly property string home: Quickshell.env("HOME")
    readonly property string fixture: root.home + "/fixture"
    // "parse" reads the launch value in the list view; "sweep" also walks every plane and overlay, then changes the value live.
    readonly property string mode: Quickshell.env("GLASS_MODE")
    // The 8-bit alpha the planes must carry at launch: 87 is 0.34, 255 is opaque.
    readonly property int expect: Number(Quickshell.env("GLASS_EXPECT"))
    readonly property bool stock: Quickshell.env("GLASS_STOCK") === "1"
    readonly property int listedAlpha: Pixels.listAlpha(Flea.Theme.color.opaquePalette, root.expect, root.stock && Flea.Glass.split && Flea.Glass.usesDefault && Flea.Glass.mode === "glassy")
    // Parse mode: a user value written after launch, which an opaque launch must not take until the next one.
    readonly property string lateUser: Quickshell.env("GLASS_LATE_USER") || ""
    // The 8-bit alpha a late value must reach: its own on a clear launch, 255 on a launch a value pinned opaque.
    readonly property int lateExpect: Number(Quickshell.env("GLASS_LATE_EXPECT") || root.expect)
    // Where grabs are saved as PNG for the identity comparison against the base tree, or "" for none.
    readonly property string shots: Quickshell.env("GLASS_SHOTS") || ""
    readonly property var backdrops: ["#ff00ff", "#00ff00"]
    property int backdropIndex: 0
    // Solved colour channels carry the division by alpha, about 1/alpha of an 8-bit step at most.
    readonly property int colourSlack: 3
    readonly property int alphaSlack: 1
    readonly property int stallTicks: 200
    property int checks: 0
    property var failures: []
    property var steps: []
    property int stepIndex: 0
    property int stepTicks: 0
    property bool busy: false
    property var grabs: []
    property var grabDone: null
    property string grabName: ""

    function check(cond, label) {
        root.checks++
        if (!cond) {
            root.failures.push(label)
            console.log("GLASS FAIL " + label)
        }
    }
    function find(item, type) {
        if (!item) return null
        if (String(item).indexOf(type + "_") === 0) return item
        var kids = item.children || []
        for (var i = 0; i < kids.length; i++) {
            var found = root.find(kids[i], type)
            if (found) return found
        }
        return null
    }
    function hex(c) { return String(c).toLowerCase() }
    function rgb(c) { return [Math.round(c.r * 255), Math.round(c.g * 255), Math.round(c.b * 255)] }
    function rectOf(item) {
        var p = item.mapToItem(stage, 0, 0)
        return { x: Math.round(p.x), y: Math.round(p.y), w: Math.round(item.width), h: Math.round(item.height) }
    }
    readonly property var pane: body.currentPane
    function ready(p) { return !p.listInFlight && p.listingState === "ready" }

    // Both grabs, then done(pairs) with pairs a function (x, y) answering [r1, g1, b1, r2, g2, b2].
    function grabBoth(name, done) {
        root.busy = true
        root.grabs = []
        root.grabName = name
        root.grabDone = done
        root.backdropIndex = 0
        Qt.callLater(root.grabNext)
    }
    function grabNext() {
        stage.grabToImage(function (result) {
            var file = root.shots + "/" + root.grabName + "-" + root.backdropIndex + ".png"
            if (root.shots.length > 0)
                root.check(result.saveToFile(file), "grab " + file + " was not saved")
            shot.source = result.url
        })
    }
    function painted(ctx) {
        ctx.drawImage(shot, 0, 0)
        root.grabs.push(ctx.getImageData(0, 0, stage.width, stage.height).data)
        if (root.grabs.length < 2) {
            root.backdropIndex = 1
            Qt.callLater(root.grabNext)
            return
        }
        var a = root.grabs[0], b = root.grabs[1], w = stage.width
        var done = root.grabDone
        root.grabDone = null
        root.busy = false
        done(function (x, y) {
            var o = (y * w + x) * 4
            return [a[o], a[o + 1], a[o + 2], b[o], b[o + 1], b[o + 2]]
        })
    }

    // One plane of this colour at exactly the expected alpha; at 255 the colour must be today's to the bit.
    function plane(px, tag, item, role, want) {
        if (!item || !item.visible || item.width <= 0 || item.height <= 0) { root.check(false, tag + " has no drawn region"); return }
        root.planeIn(px, tag, root.rectOf(item), role, want === undefined && role === "background" ? root.listedAlpha : want)
    }
    function planeIn(px, tag, box, role, want) {
        var target = root.rgb(role === "surface" ? Flea.Theme.color.surface : Flea.Theme.color.background)
        Pixels.checkPlane(root, stage, target, px, tag, box, want)
    }
    // An overlay hides what is under it: its modal pixel is the same over both backdrops.
    function opaque(px, tag, box) {
        var s = Pixels.solve(px, box, stage.width, stage.height)
        root.check(s !== null && s.alpha === 255, tag + " is not opaque: alpha " + (s ? s.alpha : "none"))
    }
    // No pixel lets the backdrop through untouched, so nothing in the window lacks a ground.
    function noHoles(px, tag) {
        var n = Pixels.holes(px, stage.width, stage.height)
        root.check(n === 0, tag + " has " + n + " pixels with no ground")
    }
    // Every pixel is the same over both backdrops, which is what an opaque window draws.
    function allOpaque(px, tag) {
        var n = Pixels.seeThrough(px, stage.width, stage.height)
        root.check(n === 0, tag + " lets the desktop through at " + n + " pixels")
    }
    // The written value reached Omarchy's own reader, so a check that follows it is not vacuous.
    function read(value, tag) {
        var got = Commons.Color.pickAlpha("flea.background-alpha", -1)
        root.check(Math.abs(got - value) < 0.001, tag + " reached Omarchy's reader as " + got)
    }
    function header(px, tag, a) {
        var item = root.find(root.pane, "Header")
        var wash = Commons.Style.normalFill.a
        var want = Math.round(255 * (1 - (1 - a / 255) * (1 - Math.round(wash * 255) / 255)))
        var s = item ? Pixels.solve(px, root.rectOf(item), stage.width, stage.height) : null
        root.check(s !== null && Math.abs(s.alpha - want) <= 2, tag + " header alpha " + (s ? s.alpha : "none") + " is not one plane plus its wash, " + want)
    }
    // The listing, the rail, the chrome, the header and the status bar at the expected alpha.
    function listPlanes(px, tag, a) {
        var listed = root.stock && Flea.Glass.split && Flea.Glass.usesDefault && Flea.Glass.mode === "glassy" ? root.listedAlpha : a
        root.plane(px, tag + " listing", root.pane.listSlot, "background", listed)
        root.plane(px, tag + " rail", root.find(root.pane, "Sidebar"), "surface", a)
        root.plane(px, tag + " chrome", root.find(body, "ChromeBar"), "surface", a)
        root.plane(px, tag + " status", root.find(body, "StatusBar"), "surface", a)
        root.header(px, tag, listed)
    }

    function wait(test) { return function () { return test() } }
    function act(fn) { return function () { fn(); return true } }
    function measure(name, fn) {
        return function () {
            if (root.busy) return false
            if (root.measured === name) { root.measured = ""; return true }
            root.grabBoth(name, function (px) { fn(px); root.measured = name })
            return false
        }
    }
    property string measured: ""
    function settle(ms) {
        var until = 0
        return function () {
            if (until === 0) until = Date.now() + ms
            if (Date.now() < until) return false
            until = 0
            return true
        }
    }

    function buildSteps() {
        var s = [root.wait(function () { return root.ready(root.pane) && root.pane.path === root.fixture }),
                 root.settle(200),
                 root.measure("list", function (px) {
                     root.listPlanes(px, "list", root.expect)
                     if (root.expect > 0) root.noHoles(px, "list")
                     if (root.expect === 255) root.allOpaque(px, "list")
                 })]
        if (root.mode === "parse" && root.lateUser.length > 0) {
            s.push(root.act(function () { userFile.setText("[flea]\nbackground-alpha = " + root.lateUser + "\n") }))
            s.push(root.settle(800))
            s.push(root.measure("late", function (px) {
                root.read(Number(root.lateUser), "late user value")
                root.listPlanes(px, "late user value", root.lateExpect)
                if (root.lateExpect > 0) root.noHoles(px, "late user value")
                if (root.lateExpect === 255) root.allOpaque(px, "late user value")
            }))
        }
        if (root.mode === "sweep")
            s = s.concat(root.sweepSteps())
        root.steps = s
    }

    function sweepSteps() {
        var tabs = function () { return root.find(body, "TabBar") }
        return [
            // The filter strip sits on the pane's own ground.
            root.act(function () { root.pane.filterTyping = true }),
            root.settle(150),
            root.measure("filter", function (px) { root.plane(px, "filter", root.find(root.pane, "FilterStrip"), "background") }),
            root.act(function () { root.pane.filterTyping = false; root.pane.filterQuery = "" }),
            // The search strip takes the header's slot on the chrome's surface.
            root.act(function () { root.pane.searchMode = "typing" }),
            root.settle(150),
            root.measure("search", function (px) {
                root.plane(px, "search strip", root.find(root.pane, "SearchStrip"), "surface")
                root.underStrip(root.find(root.pane, "Header"))
                root.plane(px, "search listing", root.pane.listSlot, "background")
                root.noHoles(px, "search")
            }),
            root.act(function () { root.pane.searchMode = "" }),
            // Two tabs: the strip on the surface, the current tab's plate on the listing's ground.
            root.act(function () { root.pane.act("tabNew") }),
            root.wait(function () { var t = tabs(); return t && t.visible && t.height > 0 && root.ready(root.pane) }),
            root.settle(200),
            root.measure("tabs", function (px) {
                var t = tabs()
                root.plane(px, "tab strip", t, "surface")
                root.planeIn(px, "current tab plate", root.plateBox(t), "background")
                root.listPlanes(px, "tabs", root.expect)
                root.noHoles(px, "tabs")
            }),
            // A dragged tab draws ghosted; the current one must not ghost its plate into a hole.
            root.act(function () { var t = tabs(); t.dragFrom = t.currentIndex }),
            root.settle(150),
            root.measure("tab drag", function (px) {
                root.planeIn(px, "dragged current tab", root.plateBox(tabs()), "surface")
                root.noHoles(px, "tab drag")
            }),
            root.act(function () { tabs().dragFrom = -1 }),
            root.act(function () { root.pane.act("tabClose") }),
            root.wait(function () { var t = tabs(); return t && !t.visible && root.ready(root.pane) }),
            // Dual: both path strips, both listings, one ground each.
            root.act(function () { Flea.ViewState.changeKey("view", "dual") }),
            root.wait(function () { return body.dualMode && root.otherPane() !== null && root.ready(root.otherPane()) }),
            root.settle(300),
            root.measure("dual", function (px) {
                var left = root.primary(), right = root.otherPane()
                root.plane(px, "dual left path", root.find(left, "PanePath"), body.focusSide === 0 ? "surface" : "background", root.expect)
                root.plane(px, "dual right path", root.find(right, "PanePath"), body.focusSide === 1 ? "surface" : "background", root.expect)
                root.plane(px, "dual left listing", left.listSlot, "background")
                root.plane(px, "dual right listing", right.listSlot, "background")
                root.noHoles(px, "dual")
            }),
            root.act(function () { Flea.ViewState.changeKey("view", "list") }),
            root.wait(function () { return !body.dualMode && root.ready(root.pane) }),
            // Columns and grid stand on the same pane ground.
            root.act(function () { root.pane.chooseView("columns") }),
            root.wait(function () { return root.pane.viewMode === "columns" && root.pane.listSlot.item && root.ready(root.pane) }),
            root.settle(400),
            root.measure("columns", function (px) {
                root.plane(px, "columns listing", root.pane.listSlot, "background")
                root.plane(px, "columns rail", root.find(root.pane, "Sidebar"), "surface")
                root.noHoles(px, "columns")
            }),
            root.act(function () { root.pane.chooseView("grid") }),
            root.wait(function () { return root.pane.viewMode === "grid" && root.pane.listSlot.item && root.ready(root.pane) }),
            root.settle(400),
            root.measure("grid", function (px) {
                root.plane(px, "grid listing", root.pane.listSlot, "background")
                root.noHoles(px, "grid")
            }),
            root.act(function () { root.pane.chooseView("list") }),
            root.wait(function () { return root.pane.viewMode === "list" && root.ready(root.pane) }),
            // Auto-hide reveals the rail over the rows, so it is opaque the way a menu is.
            root.act(function () { Flea.ViewState.changeLeaf("places", { autoHide: true }) }),
            root.wait(function () { return Flea.ViewState.railAutoHide }),
            root.act(function () { root.find(root.pane, "PaneRail").revealed = true }),
            root.wait(function () { var s = root.find(root.pane, "Sidebar"); return s && s.visible && s.width > 0 }),
            root.settle(200),
            root.measure("overlay rail", function (px) { root.opaque(px, "overlay rail", root.rectOf(root.find(root.pane, "Sidebar"))) }),
            root.act(function () { root.find(root.pane, "PaneRail").revealed = false; Flea.ViewState.changeLeaf("places", { autoHide: false }) }),
            root.wait(function () { return !Flea.ViewState.railAutoHide && root.find(root.pane, "Sidebar") !== null }),
            // A menu and a dialog stay opaque.
            root.act(function () { var p = root.pane.listSlot.mapToItem(null, 300, 300); root.pane.contextMenu().openBackground(Qt.point(p.x, p.y)) }),
            root.wait(function () { var m = root.pane.contextMenu(); return m.opened && m.frameItem && m.frameItem.width > 0 }),
            root.settle(300),
            root.measure("menu", function (px) { root.opaque(px, "context menu", root.rectOf(root.pane.contextMenu().frameItem)) }),
            root.act(function () { root.pane.contextMenu().close() }),
            root.act(function () { root.find(body, "ChromeBar").settingsRequested() }),
            root.wait(function () { var s = root.settingsCard(); return s && s.width > 0 }),
            root.settle(400),
            root.measure("dialog", function (px) { root.opaque(px, "settings dialog", root.rectOf(root.settingsCard())) }),
            root.act(function () { root.find(body, "SettingsPanel").close() }),
            root.settle(300),
            // A glass launch follows the value live: a theme switch first, its palette with it, then the user's file, then opaque.
            root.act(function () {
                themeColors.setText(root.switched)
                themeShell.setText("[flea]\nbackground-alpha = 0.5\n")
                themeName.setText("glass-switched\n")
            }),
            root.wait(function () { return root.hex(Flea.Theme.color.background) === "#283040" }),
            root.settle(800),
            root.measure("theme switch", function (px) { root.read(0.5, "theme switch 0.5"); root.listPlanes(px, "theme switch", 128) }),
            root.act(function () { userFile.setText("[flea]\nbackground-alpha = 0.6\n") }),
            root.settle(800),
            root.measure("live", function (px) { root.read(0.6, "live 0.6"); root.listPlanes(px, "live 0.6", 153) }),
            root.act(function () { userFile.setText("[flea]\nbackground-alpha = 1\n") }),
            root.settle(800),
            root.measure("live opaque", function (px) {
                root.read(1, "live 1")
                root.listPlanes(px, "live 1", 255)
                root.allOpaque(px, "live 1")
            })
        ]
    }
    function primary() { return root.find(body, "Pane") }
    // The current tab's box less its accent edge, in stage pixels.
    function plateBox(t) {
        var r = root.rectOf(t.itemAt(t.currentIndex))
        return { x: r.x, y: r.y, w: r.w, h: r.h - Flea.Theme.accentEdge }
    }
    // A translucent strip would show the header's titles, wash and rule, so each one under it is down.
    function underStrip(header) {
        var kids = header.children
        for (var i = 0; i < kids.length; i++) {
            var type = String(kids[i]).split("(")[0]
            if (type.indexOf("SearchStrip") === 0 || type.indexOf("QQuickLoader") === 0) continue
            root.check(!kids[i].visible, "the header's " + type + " still draws under the search strip")
        }
    }
    function otherPane() {
        var panes = []
        function walk(item) {
            if (!item) return
            if (String(item).indexOf("Pane_") === 0) panes.push(item)
            var kids = item.children || []
            for (var i = 0; i < kids.length; i++) walk(kids[i])
        }
        walk(body)
        return panes.length > 1 ? panes[1] : null
    }
    function settingsCard() { var s = root.find(body, "SettingsPanel"); return s && s.opened ? s.cardItem : null }

    function report() {
        stepper.running = false
        console.log("GLASS DONE checks=" + root.checks + " failed=" + root.failures.length)
        Quickshell.execDetached(["kill", String(Quickshell.processId)])
    }

    Component.onCompleted: root.buildSteps()

    // The palette a theme switch lands, with a background and a surface of its own.
    readonly property string switched: 'background = "#283040"\ndark_background = "#384055"\nforeground = "#d0d4e0"\naccent = "#7aa2f7"\n'
    readonly property string themeDir: root.home + "/.local/state/omarchy/current/theme"
    // Blocking writes, so theme.name lands after the theme's files the way omarchy-theme-set orders them.
    FileView { id: themeColors; path: root.themeDir + "/colors.toml"; printErrors: false; blockWrites: true }
    FileView { id: themeShell; path: root.themeDir + "/shell.toml"; printErrors: false; blockWrites: true }
    FileView { id: themeName; path: root.home + "/.local/state/omarchy/current/theme.name"; printErrors: false; blockWrites: true }
    FileView {
        id: userFile
        path: root.home + "/.config/omarchy/shell.toml"
        printErrors: false
        blockWrites: true
    }

    FloatingWindow {
        id: window
        implicitWidth: 1000
        implicitHeight: 650
        Item {
            id: stage
            anchors.fill: parent
            Rectangle { anchors.fill: parent; color: root.backdrops[root.backdropIndex] }
            Flea.WindowBody { id: body; host: window }
        }
        Image {
            id: shot
            cache: false
            opacity: 0
            onStatusChanged: if (status === Image.Ready) probe.requestPaint()
        }
        Canvas {
            id: probe
            width: stage.width
            height: stage.height
            opacity: 0
            onPaint: if (shot.status === Image.Ready && root.grabDone) root.painted(getContext("2d"))
        }
    }

    Timer {
        id: stepper
        interval: 50
        running: true
        repeat: true
        onTriggered: {
            if (root.stepIndex >= root.steps.length) { root.report(); return }
            if (root.steps[root.stepIndex]() === true) {
                root.stepIndex++
                root.stepTicks = 0
                return
            }
            if (++root.stepTicks > root.stallTicks) {
                root.check(false, "step " + root.stepIndex + " never completed")
                root.report()
            }
        }
    }
}
