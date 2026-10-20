//@ pragma ShellId flea-figure-store-test

import QtQuick
import Quickshell
import Quickshell.Io
import "flea/js/FigureDate.js" as FigureDate
import "flea/js/FigureWarm.js" as FigureWarm
import "flea" as Flea

// The persistent SVG cache through the real helper and store: a figure drawn once answers a repeat from disk with no helper, and no other theme or advance ever does.
ShellRoot {
    id: shell

    function log(line) { console.log("FIGURE_STORE " + line) }
    function quit() { Quickshell.execDetached(["kill", String(Quickshell.processId)]) }

    property int checks: 0
    property int failures: 0
    property bool done: false
    property int index: -1
    property bool waiting: false
    property int ticket: 0
    // An id no service ticket ever has, so a direct get on the store is told from the service's own.
    readonly property int probeId: 1000000
    property string drawn: ""
    property int sendsMark: 0
    property int exitsMark: 0
    property int hitsMark: 0
    property int missesMark: 0
    // The held warm case: the gate stub keeps the store from replying until the case lets it.
    property bool heldRan: false
    property var placedKeys: []
    property int placedDone: 0
    property int placedWant: 2
    // The nested and mixed documents' figures sit inside an item and a quote, two depths down.
    property string nestPath: ""
    readonly property string document: Quickshell.env("FLEA_FIGURE_STORE_DOC")
    readonly property string nestedDocument: Quickshell.env("FLEA_FIGURE_NEST_DOC")
    readonly property string mixedDocument: Quickshell.env("FLEA_FIGURE_MIXED_DOC")
    readonly property string holdPath: Quickshell.env("FLEA_STORE_HOLD")
    readonly property string gatePath: Quickshell.env("FLEA_STORE_GATE")
    readonly property int idleExitMs: 150
    readonly property int watchdogMs: 90000
    readonly property var theme: ({ bg: "#101315", fg: "#c0caf5", accent: "#7aa2f7", font: "monospace", bodyPx: 14, exPx: 7 })
    readonly property var otherTheme: ({ bg: "#101315", fg: "#c0caf5", accent: "#f7768e", font: "monospace", bodyPx: 14, exPx: 7 })
    readonly property var unseenTheme: ({ bg: "#101315", fg: "#c0caf5", accent: "#e0af68", font: "monospace", bodyPx: 14, exPx: 7 })
    readonly property var advancesA: ({ bg: "#101315", fg: "#c0caf5", accent: "#7aa2f7", font: "monospace", bodyPx: 14, exPx: 7, advances: [500, 600], boldAdvances: [550, 650] })
    readonly property var advancesB: ({ bg: "#101315", fg: "#c0caf5", accent: "#7aa2f7", font: "monospace", bodyPx: 14, exPx: 7, advances: [500, 601], boldAdvances: [550, 650] })
    readonly property string source: "\\frac{a}{b}"
    // Most of an entry's size limit, so a kill instead of a drain leaves the store with only a pipe's worth of it.
    readonly property int largeSvgChars: 3500000
    readonly property string largeSvg: "<svg>" + "x".repeat(shell.largeSvgChars) + "</svg>"
    readonly property string largeKey: Flea.FigureService.cacheKeyOf("math", "put then stop", shell.theme, true)
    Component.onCompleted: {
        Flea.ViewState.setTextSize({ mode: 14 })
        Flea.FigureService.idleExitMs = shell.idleExitMs
    }

    function check(passed, why) {
        shell.checks++
        if (!passed)
            shell.failures++
        shell.log((passed ? "PASS " : "FAIL ") + why)
    }

    function finish() {
        if (shell.done)
            return
        shell.done = true
        shell.log(shell.checks + " checks, " + shell.failures + " failed")
        shell.quit()
    }

    // Forgets the in-memory answers, so only the disk can answer the next ask.
    function forgetMemory() {
        Flea.FigureService.answerCache = ({})
        Flea.FigureService.answerOrder = []
    }

    function mark() {
        shell.sendsMark = Flea.FigureService.sends
        shell.exitsMark = Flea.FigureService.helperExits
        shell.hitsMark = Flea.FigureService.persistent.hits
        shell.missesMark = Flea.FigureService.persistent.misses
    }

    function ask(kind, source, theme) {
        shell.forgetMemory()
        shell.ticket = Flea.FigureService.ask(kind, source, true, theme)
    }

    readonly property var figureBlock: ({ type: "figure", kind: "math", source: shell.source, display: true })
    readonly property var unknownBlock: ({ type: "figure", kind: "math", source: "never drawn", display: true })
    readonly property string ganttSource: "gantt\n    dateFormat YYYY-MM-DD\n    section Work\n    Build :a1, 2001-10-01, 30d"
    readonly property var ganttBlock: ({ type: "figure", kind: "mermaid", source: shell.ganttSource, display: true })
    // The warm query through the production key building, so a Gantt regression in it goes red here.
    function warmBlocks(blocks, themes) {
        function keyOf(kind, source, theme) {
            return Flea.FigureService.cacheKeyOf(kind, source, theme, true);
        }
        var query = FigureWarm.query(blocks, themes, keyOf);
        var ok = Flea.FigureService.warm(query.keys, query.kinds);
        if (!ok)
            shell.check(false, "warm query refused while the helper stood idle");
        return ok;
    }

    // A case asks or warms; its verdict comes from the answer (cases with a verify) or from the store's known reply (cases with a known).
    readonly property var cases: [
        { act: function () { shell.ask("math", shell.source, shell.theme) },
          verify: function (svg, error, service, disk) {
              shell.drawn = svg
              shell.check(shell.drew(svg, error) && service.sends === shell.sendsMark + 1 && disk.misses === shell.missesMark + 1, "a figure never drawn misses the disk and the helper draws it")
              shell.check(disk.puts === 1, "the helper's answer is written to the disk cache once")
          } },
        { act: function () { shell.ask("math", shell.source, shell.theme) },
          verify: function (svg, error, service, disk) {
              shell.check(shell.drew(svg, error) && svg === shell.drawn, "a repeat answers the drawn bytes")
              shell.check(service.sends === shell.sendsMark && service.helperExits === shell.exitsMark && !service.helperRunning && disk.hits === shell.hitsMark + 1,
                  "a repeat from disk starts no helper and sends it nothing")
          } },
        { act: function () { shell.ask("math", shell.source, shell.otherTheme) },
          verify: function (svg, error, service, disk) {
              shell.check(shell.drew(svg, error) && service.sends === shell.sendsMark + 1 && disk.hits === shell.hitsMark, "another theme is never served the first theme's figure")
          } },
        { act: function () { shell.ask("math", shell.source, shell.advancesA) },
          verify: function (svg, error, service, disk) {
              shell.check(shell.drew(svg, error) && service.sends === shell.sendsMark + 1 && disk.hits === shell.hitsMark, "a first advance table is drawn by the helper")
          } },
        { act: function () { shell.ask("math", shell.source, shell.advancesB) },
          verify: function (svg, error, service, disk) {
              shell.check(shell.drew(svg, error) && service.sends === shell.sendsMark + 1 && disk.hits === shell.hitsMark, "another advance table is never served the first one's figure")
          } },
        { act: function () { shell.ask("math", shell.source, shell.advancesA) },
          verify: function (svg, error, service, disk) {
              shell.check(shell.drew(svg, error) && service.sends === shell.sendsMark && disk.hits === shell.hitsMark + 1, "the first advance table's figure is still on disk")
          } },
        // Every figure of this document is on disk under the theme it is asked under, so warming it asks the store and starts no helper.
        { act: function () { shell.warmBlocks([shell.figureBlock], { math: shell.theme }) },
          known: function (all, service) {
              shell.check(all === true && !service.helperRunning && !service.starting && service.sends === shell.sendsMark, "a document whose figures are all on disk starts no helper")
          } },
        // The same figure under a theme never drawn is not known, so the helper starts warm; the gate stub holds the store's reply, so the refusal is judged with the reply owed.
        { hold: true,
          act: function () { shell.warmBlocks([shell.figureBlock], { math: shell.unseenTheme }) },
          held: function (disk) {
              shell.check(!!disk.pid && !disk.starting && !disk.stopping && disk.owed > 0 && disk.queued.length === 0, "the store is running, not starting, with the warm reply owed and nothing queued")
              shell.check(disk.stop() === false, "an idle stop is refused while the store owes the warm query's reply")
              shell.check(!!disk.pid && !disk.stopping, "the refused stop leaves the store running")
          },
          known: function (all, service) {
              shell.check(all === false && (service.helperRunning || service.starting), "a figure drawn under another accent does not keep the helper from starting warm")
          } },
        // One figure never drawn: the answer is not all, and the helper starts warm.
        { act: function () { shell.warmBlocks([shell.figureBlock, shell.unknownBlock], { math: shell.theme }) },
          known: function (all, service) {
              shell.check(all === false, "a document with a figure never drawn is not all known")
              shell.check(service.helperRunning || service.starting, "the helper is started warm for it")
          } },
        // A Gantt chart is asked for today, then warming its document finds it on disk and starts no helper.
        { act: function () { shell.ask("mermaid", shell.ganttSource, FigureDate.datedTheme("mermaid", shell.ganttSource, shell.theme)) },
          verify: function (svg, error, service, disk) {
              shell.check(shell.drew(svg, error) && service.sends === shell.sendsMark + 1, "a Gantt chart is drawn by the helper under its dated theme")
          } },
        { act: function () { shell.warmBlocks([shell.ganttBlock], { mermaid: shell.theme }) },
          known: function (all, service) {
              shell.check(all === true && !service.helperRunning && !service.starting && service.sends === shell.sendsMark, "a Gantt document warmed from disk starts no helper")
          } },
        // A get starts the store, then a large put and an idle stop land in one turn: stdin closes and the store drains, where a kill would lose the entry.
        { act: function () { Flea.FigureService.persistent.get(shell.probeId, "math\nnever stored\nfalse\nx") },
          answered: function (disk) {
              disk.put(shell.largeKey, shell.largeSvg)
              shell.check(disk.stop() === true, "an idle stop with only a put in flight is accepted")
              shell.verdictDone()
          } },
        { act: function () { shell.ask("math", "put then stop", shell.theme) },
          verify: function (svg, error, service, disk) {
              shell.check(svg === shell.largeSvg && service.sends === shell.sendsMark && disk.hits === shell.hitsMark + 1, "the put that an idle stop followed at once is on disk")
          } },
        // A document's maths and Mermaid figures are drawn by a real pane and land on disk under the keys its placed figures asked.
        { act: function () { shell.placedWant = 2; shell.forgetMemory(); drawPane.active = true },
          drawn: function (service) {
              shell.placedDone++
              if (shell.placedDone < shell.placedWant)
                  return
              shell.placedKeys = Object.keys(service.answerCache).sort()
              shell.check(shell.placedKeys.length === 2 && shell.placedKeys[0].indexOf("math\n") === 0 && shell.placedKeys[1].indexOf("mermaid\n") === 0, "a pane's placed maths and Mermaid figures are both drawn, under two keys")
              shell.verdictDone()
          } },
        // A fresh pane of the same document, with nothing in memory, warms by the keys the placed figures put: the store knows them all and no helper starts.
        { act: function () { shell.forgetMemory(); drawPane.active = false; probePane.active = true },
          known: function (all, service) {
              shell.check(all === true, "a fresh pane's warm query finds every placed figure already on disk")
              shell.check(service.warmKeys.slice().sort().join("\u0001") === shell.placedKeys.join("\u0001"), "the warm query names exactly the keys the placed figures put")
              shell.check(!service.helperRunning && !service.starting && service.sends === shell.sendsMark, "a document whose figures are on disk starts no helper")
          } },
        // A document whose only figures sit inside an item, a quote, a quote in an item, an item in that quote and a quote in an item in a quote, draws them all.
        { act: function () { shell.drawNested(shell.nestedDocument, 5) },
          drawn: function (service) { shell.nestedDrawn(service, 5, "nested-only") } },
        { act: function () { shell.warmNested() },
          known: function (all, service) { shell.nestedWarmed(all, service, "nested-only") } },
        // A mixed document, a top-level figure and nested ones, draws them all too.
        { act: function () { shell.drawNested(shell.mixedDocument, 6) },
          drawn: function (service) { shell.nestedDrawn(service, 6, "mixed") } },
        { act: function () { shell.warmNested() },
          known: function (all, service) { shell.nestedWarmed(all, service, "mixed") } }
    ]

    function drawNested(path, want) {
        shell.placedWant = want
        shell.nestPath = path
        shell.forgetMemory()
        nestProbe.active = false
        nestDraw.active = true
    }

    function nestedDrawn(service, want, label) {
        shell.placedDone++
        if (shell.placedDone < want)
            return
        shell.placedKeys = Object.keys(service.answerCache).sort()
        shell.check(shell.placedKeys.length === want, "a " + label + " pane draws its " + want + " figures, under " + want + " keys")
        shell.verdictDone()
    }

    function warmNested() {
        shell.forgetMemory()
        nestDraw.active = false
        nestProbe.active = true
    }

    // The probe pane has no warm request once its document is parsed when the warm path never saw a figure, and then no reply comes.
    function nestedGuard() {
        var current = shell.cases[shell.index]
        if (!current || !current.known || nestProbe.item === null || nestProbe.item.warmRequest !== null)
            return
        shell.check(false, "a warmed pane's document with figures makes a warm request")
        shell.verdictDone()
    }

    function nestedWarmed(all, service, label) {
        shell.check(all === true, "a fresh pane's warm query finds every " + label + " figure already on disk")
        shell.check(service.warmKeys.slice().sort().join("\u0001") === shell.placedKeys.join("\u0001"), "the " + label + " warm query names exactly the keys the placed figures put")
        shell.check(!service.helperRunning && !service.starting && service.sends === shell.sendsMark, "a " + label + " document whose figures are on disk starts no helper")
    }

    function drew(svg, error) { return svg.indexOf("<svg") === 0 && error === "" }

    function runNext() {
        shell.waiting = false
        shell.index++
        if (shell.index >= shell.cases.length) {
            shell.finish()
            return
        }
        shell.mark()
        shell.heldRan = false
        shell.placedDone = 0
        var current = shell.cases[shell.index]
        // The hold is armed by a marker file the gate stub consumes at its next store start, so the arming lands before the case acts.
        if (current.hold)
            arm.running = true
        else
            current.act()
    }

    // The next case starts once the helper and the store have both stopped at their idle exit, so each case begins from nothing running.
    function settle() {
        var service = Flea.FigureService
        if (shell.waiting && !service.helperRunning && !service.starting && !service.persistent.active)
            shell.runNext()
    }

    function verdictDone() {
        shell.waiting = true
        Qt.callLater(shell.settle)
    }

    Connections {
        target: Flea.FigureService
        function onDone(ticket, svg, error) {
            var current = shell.cases[shell.index]
            if (current && current.drawn) {
                if (svg !== "" && error === "")
                    current.drawn(Flea.FigureService)
                return
            }
            if (ticket !== shell.ticket || !current || !current.verify)
                return
            current.verify(svg, error, Flea.FigureService, Flea.FigureService.persistent)
            shell.verdictDone()
        }
        function onHelperRunningChanged() { Qt.callLater(shell.settle) }
    }

    Connections {
        target: Flea.FigureService.persistent
        // Deferred, so the next case never starts inside the change that announced the stop.
        function onActiveChanged() { Qt.callLater(shell.settle) }
        // Deferred past onStarted, which writes the queued lines after it clears starting, so the held state has nothing queued.
        function onStartingChanged() {
            var current = shell.cases[shell.index]
            if (Flea.FigureService.persistent.starting || !current || !current.held || shell.heldRan)
                return
            shell.heldRan = true
            Qt.callLater(function () {
                current.held(Flea.FigureService.persistent)
                Quickshell.execDetached(["sh", "-c", "echo > \"$1\"", "sh", shell.gatePath])
            })
        }
        function onAnswered(id, svg) {
            var current = shell.cases[shell.index]
            if (id === shell.probeId && current && current.answered)
                current.answered(Flea.FigureService.persistent)
        }
        function onKnown(id, all) {
            var current = shell.cases[shell.index]
            if (!current || !current.known)
                return
            // The service's own handler has acted by the time this runs, so a helper it started is already starting.
            Qt.callLater(function () {
                current.known(all, Flea.FigureService)
                shell.verdictDone()
            })
        }
    }

    Process {
        id: arm
        command: ["touch", shell.holdPath]
        onExited: shell.cases[shell.index].act()
    }

    // The same document in two panes, each inactive until its case: the first draws its figures, the second is a fresh pane that only warms.
    FloatingWindow {
        implicitWidth: 1120
        implicitHeight: 600
        color: "#101315"

        Flea.PreviewMarkdown {
            id: drawPane
            width: 560
            height: 600
            active: false
            view: "rendered"
            path: shell.document
            size: 1
        }
        Flea.PreviewMarkdown {
            id: probePane
            x: 560
            width: 560
            height: 600
            active: false
            view: "rendered"
            path: shell.document
            size: 1
        }
    }

    FloatingWindow {
        implicitWidth: 1120
        implicitHeight: 600
        color: "#101315"

        Loader {
            id: nestDraw
            active: false
            width: 560
            height: 600
            sourceComponent: Flea.PreviewMarkdown {
                active: true
                view: "rendered"
                path: shell.nestPath
                size: 1
            }
        }
        Loader {
            id: nestProbe
            active: false
            x: 560
            width: 560
            height: 600
            sourceComponent: Flea.PreviewMarkdown {
                active: true
                view: "rendered"
                path: shell.nestPath
                size: 1
            }
        }
        Connections {
            target: nestProbe.item
            function onContentReadyChanged() {
                if (nestProbe.item.contentReady)
                    Qt.callLater(shell.nestedGuard)
            }
        }
    }

    Timer {
        interval: shell.watchdogMs
        running: !shell.done
        onTriggered: {
            shell.check(false, "the watchdog outlived the verdict at case " + shell.index)
            shell.finish()
        }
    }

    Timer {
        interval: 1
        running: true
        onTriggered: shell.runNext()
    }
}
