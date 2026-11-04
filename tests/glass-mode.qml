//@ pragma ShellId flea-glass-mode-test
import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons as Commons
import "flea" as Flea
import "flea/js/Contrast.js" as Contrast

// Omarchy's Window Transparency precedence through Glass, and a Hyprland config reload reaching Style and Glass with no theme switch.
ShellRoot {
    id: root
    readonly property string home: Quickshell.env("HOME")
    readonly property string toggles: Quickshell.env("XDG_STATE_HOME") + "/omarchy/toggles/hypr"
    // "precedence" decides at launch and checks the 8-bit alpha; "reload" then moves the mode and Hyprland's rounding under a running window.
    readonly property string mode: Quickshell.env("GLASSM_MODE")
    readonly property int expect: Number(Quickshell.env("GLASSM_EXPECT"))
    // Set only by a row that proves the glassy-bare path; "-" keeps the shipped default of Glass.glassAlpha.
    readonly property string glass: Quickshell.env("GLASSM_GLASS")
    readonly property string roundingFile: Quickshell.env("GLASSM_ROUNDING")
    property int checks: 0
    property var failures: []
    property var steps: []
    property int stepIndex: 0
    property int stepTicks: 0
    // The stepper ticks every 50 ms, so 200 ticks bound one step to 10 s before it is reported as never completed.
    readonly property int stepTickLimit: 200
    // Style's own startup refresh lands up to this long after the first answer, so nothing is changed before it.
    readonly property int startupSettleMs: 600
    // How long a raw event or a reload gets to reach Style and Glass before a "did not move" check reads them.
    readonly property int eventSettleMs: 600
    // How long the detached rm of the toggle file gets to finish before the mode is read.
    readonly property int toggleRemoveSettleMs: 400

    function check(cond, label) {
        root.checks++
        if (!cond) {
            root.failures.push(label)
            console.log("GLASSM FAIL " + label)
        }
    }
    function eight() { return Math.round(Flea.Glass.alpha * 255) }
    function reload() { Flea.Theme.hyprEvent("configreloaded") }
    function act(fn) { return function () { fn(); return true } }
    function wait(test) { return function () { return test() } }
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
        var s = [root.act(function () { Flea.Glass.focused = true; if (root.glass !== "-") Flea.Glass.glassAlpha = Number(root.glass); Flea.Glass.launch() }),
                 root.act(function () {
                     root.check(root.eight() === root.expect, root.mode + " launch alpha " + root.eight() + " is not " + root.expect)
                     root.check(Flea.Glass.on === (root.expect < 255), "on does not follow the alpha " + root.expect)
                     if (!Flea.Glass.usesDefault || Flea.Glass.mode === "opaque") {
                         var c = Flea.Theme.color, opaque = c.opaquePalette
                         root.check(String(c.foreground) === String(opaque.foreground) && String(c.muted) === String(opaque.muted) && String(c.metadata) === String(opaque.metadata), "an override or Opaque lifted the palette's text")
                     }
                 })]
        if (root.mode === "reload") s = s.concat(root.reloadSteps())
        if (root.mode === "socket") s = s.concat(root.socketSteps())
        if (root.mode === "tag") s = s.concat(root.tagSteps())
        if (root.mode === "tag-stable") s = s.concat(root.stableSteps())
        root.steps = s
    }

    // The launch was glassy at 191; a reload that finds no toggle goes to Omarchy's default, and one that finds it comes back.
    function reloadSteps() {
        return [
            root.wait(function () { return Commons.Style.cornerRadius === 10 }),
            root.settle(root.startupSettleMs),
            root.act(function () { rounding.setText("0\n"); Flea.Theme.hyprEvent("workspace") }),
            root.settle(root.eventSettleMs),
            root.act(function () { root.check(Commons.Style.cornerRadius === 10, "a workspace event moved the corner radius") }),
            root.act(function () { Quickshell.execDetached(["rm", "-f", root.toggles + "/window-glassy.lua"]) }),
            root.settle(root.toggleRemoveSettleMs),
            root.act(function () { root.check(Flea.Glass.mode === "glassy", "the mode moved before any reload"); root.reload() }),
            root.wait(function () { return Commons.Style.cornerRadius === 0 }),
            root.act(function () {
                root.check(Flea.Glass.mode === "transparent", "a reload without the toggle left mode " + Flea.Glass.mode)
                root.check(root.eight() === 255, "bare transparent without a theme value drew " + root.eight())
                opaque.setText("return\n")
            }),
            root.act(function () { root.reload() }),
            root.wait(function () { return Flea.Glass.mode === "opaque" }),
            root.act(function () { root.check(root.eight() === 255, "opaque drew " + root.eight()); glassy.setText("return -- written again\n") }),
            root.act(function () { root.reload() }),
            root.wait(function () { return Flea.Glass.mode === "glassy" }),
            root.act(function () { root.check(root.eight() === 191, "glassy again drew " + root.eight()) })
        ]
    }

    // Hyprland's own event stream: a workspace event sent after the rounding changed moves nothing, and configreloaded moves the radius.
    function socketSteps() {
        return [
            root.wait(function () { return Commons.Style.cornerRadius === 10 }),
            root.settle(root.startupSettleMs),
            // The stand-in sends its workspace event only once ws exists, so it arrives after the rounding file reads 0.
            root.act(function () { rounding.setText("0\n"); ws.setText("ws\n") }),
            root.wait(function () { wsSent.reload(); return wsSent.text() === "sent\n" }),
            root.settle(root.eventSettleMs),
            root.act(function () { root.check(Commons.Style.cornerRadius === 10, "a workspace event moved the corner radius"); go.setText("go\n") }),
            root.wait(function () { return Commons.Style.cornerRadius === 0 })
        ]
    }

    function report() {
        stepper.running = false
        console.log("GLASSM DONE checks=" + root.checks + " failed=" + root.failures.length)
        Quickshell.execDetached(["kill", String(Quickshell.processId)])
    }
    Component.onCompleted: root.buildSteps()

    FileView { id: go; path: root.roundingFile.replace(/rounding$/, "go"); printErrors: false; blockWrites: true }
    // Each dispatch the hyprctl stand-in logged, one per line.
    function dispatches() { dispatchLog.reload(); return dispatchLog.text().split("\n").filter(function (l) { return l.length > 0 }) }
    // Each opacity read the stand-in answered, so a check that nothing was tagged waits until the window was asked about.
    function probes() { probeLog.reload(); return probeLog.text().split("\n").filter(function (l) { return l.length > 0 }) }
    function opened(cls) { Flea.Theme.hyprEvent("openwindow", "abc123,1," + cls + ",Flea") }

    // Bare Transparent with Hyprland at 1: another app's window asks nothing, Flea's own takes the tag, Glassy takes it off, Transparent puts it back.
    function tagSteps() {
        return [
            root.act(function () { root.opened("org.gnome.Nautilus") }),
            root.settle(root.eventSettleMs),
            root.act(function () { root.check(root.probes().length === 0 && root.dispatches().length === 0, "another app's window was asked about"); root.opened("com.thisisgm.flea") }),
            root.wait(function () { return root.dispatches().length === 1 }),
            root.act(function () {
                var d = root.dispatches()[0]
                root.check(d.indexOf("address:0xabc123") >= 0 && d.indexOf("+transparent") >= 0, "the open did not tag the window: " + d)
                glassy.setText("return\n")
            }),
            root.act(function () { root.reload() }),
            root.wait(function () { return root.dispatches().length === 2 }),
            root.act(function () { root.check(root.dispatches()[1].indexOf("-transparent") >= 0, "Glassy did not take the tag off: " + root.dispatches()[1]) }),
            root.act(function () { Quickshell.execDetached(["rm", "-f", root.toggles + "/window-glassy.lua"]) }),
            root.settle(root.toggleRemoveSettleMs),
            root.act(function () { root.reload() }),
            root.wait(function () { return root.dispatches().length === 3 }),
            root.act(function () { root.check(root.dispatches()[2].indexOf("+transparent") >= 0, "Transparent did not put the tag back: " + root.dispatches()[2]) }),
            root.settle(root.eventSettleMs),
            root.act(function () { root.check(root.dispatches().length === 3, "a settled window was dispatched again") })
        ]
    }
    // Stable Omarchy's catch-all rule already draws the window at 0.985, so Flea adds no tag of its own and nothing stacks.
    function stableSteps() {
        return [
            root.act(function () { root.opened("com.thisisgm.flea") }),
            root.wait(function () { return root.probes().length >= 2 }),
            root.settle(root.eventSettleMs),
            root.act(function () { root.check(root.dispatches().length === 0, "a window Hyprland already draws below 1 was tagged") })
        ]
    }

    FileView { id: dispatchLog; path: Quickshell.env("GLASSM_DISPATCH") || ""; printErrors: false; blockLoading: true }
    FileView { id: probeLog; path: Quickshell.env("GLASSM_PROBE") || ""; printErrors: false; blockLoading: true }
    FileView { id: ws; path: root.roundingFile.replace(/rounding$/, "ws"); printErrors: false; blockWrites: true }
    FileView { id: wsSent; path: root.roundingFile.replace(/rounding$/, "ws-sent"); printErrors: false; blockLoading: true }
    FileView { id: rounding; path: root.roundingFile; printErrors: false; blockWrites: true }
    FileView { id: opaque; path: root.toggles + "/window-opaque.lua"; printErrors: false; blockWrites: true }
    FileView { id: glassy; path: root.toggles + "/window-glassy.lua"; printErrors: false; blockWrites: true }

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
            if (++root.stepTicks > root.stepTickLimit) {
                root.check(false, "step " + root.stepIndex + " never completed")
                root.report()
            }
        }
    }
}
