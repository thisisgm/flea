//@ pragma ShellId flea-menu-clipboard-hunt-test

import QtQuick
import Quickshell
import Quickshell.Io
import "flea" as Flea
import "flea/js/Keymap.js" as Keymap
import "flea/js/Menu.js" as Menu
import "flea/js/Search.js" as Search

// Each real window observes native clipboard refusals; the driver records Copy as text separately.
ShellRoot {
    id: shell
    property int stage: 0
    property int ticks: 0
    property string action: Quickshell.env("FLEA_HUNT_ACTION")
    property string source: Quickshell.env("FLEA_HUNT_SOURCE")
    property string destination: Quickshell.env("FLEA_HUNT_DEST")
    property var messages: []
    property var clipReplies: []
    property var pasteRequest: null
    property int pasteReadSequence: 0
    readonly property int probeTickMs: 100
    readonly property int clipboardReadTimeoutMs: 5000
    readonly property int clipboardReadStage: 14
    readonly property string clipboardRefusal: "WAYLAND_DISPLAY is not set, so there is no clipboard to use"
    property bool quitting: false
    property int leafIndex: 0
    readonly property int artifactStage: 20
    readonly property var linkLeaves: ({pasteas: "pasteLink", "pasteas-absolute": "pasteAbsoluteLink", "pasteas-hard": "pasteHardLink"})
    readonly property var copyLeaves: ["copyPath", "copyRelative", "copyName", "copyStem", "copydirpath", "copyUri", "copyQuoted"]
    readonly property var pane: body.currentPane

    function log(line) { console.log("CLIPHUNT " + line) }
    // One line of menu evidence for a stall, so a timeout names the state it waited on.
    function menuState() {
        var menu = pane.contextMenu()
        var openRow = menu.openSubmenuRow >= 0 && menu.entries[menu.openSubmenuRow] ? menu.entries[menu.openSubmenuRow].action : ""
        return "opened=" + menu.opened + " submenuOpen=" + menu.submenuOpen + " submenu="
            + (menu.loneFlyoutAction.length > 0 ? "lone:" + menu.loneFlyoutAction : openRow)
            + " ready=" + pane.menuActions.ready + " req=" + pane.menuActions.requestId
            + " pending=" + pane.menuActions.pendingAction + "/" + pane.menuActions.pendingActivation
            + " provRefreshing=" + pane.menuActions.providersRefreshing
            + " entries=" + menu.entries.length + " subEntries=" + menu.submenuEntries.length
            + " clipPaths=" + pane.clipboard.paths.length + " cursor=" + pane.cursorIndex
            + " listInFlight=" + pane.listInFlight + " messages=" + JSON.stringify(messages.slice(-3))
    }
    function checkPublication() {
        var sets = clipReplies.filter(function (reply) { return reply.op === "set" })
        log((sets.length === 1 && sets[0].ok === false && sets[0].error === clipboardRefusal
             && pane.clipboardState.sets.length === 0 ? "PASS" : "FAIL")
            + " local-" + action + " clipSet-refused replies=" + JSON.stringify(sets))
        var notices = messages.filter(function (message) { return message.text.indexOf("Copied in this window only:") === 0 })
        log((notices.length === 1 && notices[0].error === true
             && notices[0].text === "Copied in this window only: " + clipboardRefusal ? "PASS" : "FAIL")
            + " local-" + action + " window-only-message-once notices=" + JSON.stringify(notices))
    }
    function quit() {
        if (quitting) return
        quitting = true
        body.quitBackends()
    }

    FloatingWindow {
        id: window
        implicitWidth: 900
        implicitHeight: 600
        color: "#303030"
        Flea.WindowBody { id: body; host: window }
    }
    Connections {
        target: shell.pane
        function onMessage(text, isError) { shell.messages.push({text: text, error: isError}) }
    }
    Connections {
        target: shell.pane.backend
        function onClipResult(message) { shell.clipReplies.push(message) }
    }
    Connections {
        target: shell.pane.collide
        // An empty destination answers immediately, so keep the real request before pending clears.
        function onPendingChanged() { if (shell.pane.collide.pending) shell.pasteRequest = shell.pane.collide.pending }
    }
    Process {
        id: artifactCheck
        command: ["python3", Quickshell.env("FLEA_HUNT_CHECKS"), "files", shell.action, shell.source,
                  shell.destination, Quickshell.env("FLEA_HUNT_SOURCE_BYTES")]
        stdout: StdioCollector { onStreamFinished: shell.log(this.text.trim()) }
        onExited: function(exitCode, exitStatus) {
            shell.checkPublication()
            shell.log((exitCode === 0 ? "PASS" : "FAIL") + " local-" + shell.action + " filesystem-result")
            shell.log("DONE action=" + shell.action)
            shell.quit()
        }
    }
    Timer {
        interval: shell.probeTickMs
        repeat: true
        running: !shell.quitting
        onTriggered: shell.advance()
    }
    function advance() {
        ticks += 1
        if (ticks > 200) { log("FAIL timeout waiting for stage " + stage + " " + menuState()); quit(); return }
        if (stage === 0 && action === "terminal" && !pane.listInFlight && pane.sidebar) {
            pane.contextMenu().openBackground(Qt.point(100, 100))
            pane.contextMenu().choose("openTerminal")
            log((pane.opener.terminalCurrent === pane.path ? "PASS" : "FAIL")
                + " background-terminal-path actual=" + pane.opener.terminalCurrent + " expected=" + pane.path)
            stage = 10
            ticks = 0
        } else if (stage === 10 && ticks >= 5) {
            // This is the same rail menu and chosen signal the real Places row raises.
            var entries = Menu.placeEntries({hiddenActions: [], placeFavourite: true})
            pane.contextMenu().openForRail("place:0:" + destination, entries, Qt.point(100, 100))
            pane.contextMenu().choose("openTerminal")
            log((pane.opener.terminalCurrent === destination ? "PASS" : "FAIL")
                + " place-terminal-path actual=" + pane.opener.terminalCurrent + " expected=" + destination)
            stage = 11
            ticks = 0
        } else if (stage === 11 && ticks >= 5) {
            log("DONE action=" + action)
            quit()
        } else if (stage === 0 && action === "original" && !pane.listInFlight && pane.total === 2 && pane.visibleItemFor(0)) {
            pane.cursorIndex = 0
            pane.openCursorMenu()
            stage = 15
            ticks = 0
        } else if (stage === 15 && pane.menuActions.ready) {
            pane.contextMenu().choose("showOriginal")
            stage = 16
            ticks = 0
        } else if (stage === 16 && !pane.listInFlight && pane.path !== destination && pane.cursorRow && pane.cursorRow.n === "alpha.txt") {
            log("PASS show-original-file-control path=" + pane.path + " cursor=" + pane.cursorRow.n)
            pane.open(destination)
            stage = 17
            ticks = 0
        } else if (stage === 17 && !pane.listInFlight && pane.path === destination && pane.visibleItemFor(1)) {
            pane.cursorIndex = 1
            pane.openCursorMenu()
            stage = 18
            ticks = 0
        } else if (stage === 18 && pane.menuActions.ready) {
            pane.contextMenu().choose("showOriginal")
            stage = 19
            ticks = 0
        } else if (stage === 19 && ticks >= 10) {
            log((pane.path === "/" ? "PASS" : "FAIL") + " show-original-root-target path="
                + pane.path + " messages=" + JSON.stringify(messages))
            log("DONE action=" + action)
            quit()
        } else if (stage === 0 && action.indexOf("copyas-") === 0 && !pane.listInFlight && pane.total === 1) {
            pane.chooseView(action.substring("copyas-".length))
            stage = 21
            ticks = 0
        } else if (stage === 21 && ticks >= 5 && !pane.listInFlight) {
            // Search from the listed directory so a nested file copies as sub/name on Relative path.
            pane.searchHere = true
            Search.start(pane)
            pane.searchQuery = "txt"
            Search.run(pane)
            stage = 22
            ticks = 0
        } else if (stage === 22 && !pane.searchRunning && pane.searchMode === "results"
                   && pane.total === 2 && pane.visibleItemFor(0)) {
            stage = 6
            ticks = 0
        } else if (stage === 6 && ticks >= 5 && pane.visibleItemFor(0)) {
            pane.selectAll()
            if (leafIndex === 0) {
                var invert = Keymap.lookup(Qt.Key_V, "V", Qt.ShiftModifier, "listing")
                pane.act(invert)
                log((pane.selectedIndices().length === 0 ? "PASS" : "FAIL")
                    + " " + action + " invert-clears-all action=" + invert + " count=" + pane.selectedIndices().length)
                pane.act(invert)
                log((pane.selectedIndices().length === 2 ? "PASS" : "FAIL")
                    + " " + action + " invert-marks-all count=" + pane.selectedIndices().length)
            }
            pane.act(Keymap.lookup(Qt.Key_C, "c", 0, "listing"))
            stage = 7
            ticks = 0
        } else if (stage === 7 && pane.menuActions.ready && pane.contextMenu().submenuOpen) {
            pane.contextMenu().chooseSub(copyLeaves[leafIndex])
            stage = 8
            ticks = 0
        } else if (stage === 8 && ticks >= 5) {
            log("INFO " + action + " dispatched=" + copyLeaves[leafIndex] + " messages=" + JSON.stringify(messages))
            leafIndex += 1
            if (leafIndex < copyLeaves.length) stage = 6
            else {
                pane.act(Keymap.lookup(Qt.Key_C, "", Qt.ControlModifier | Qt.ShiftModifier, "listing"))
                stage = 9
            }
            ticks = 0
        } else if (stage === 9 && ticks >= 5) {
            log("DONE action=" + action)
            quit()
        } else if (stage === 0 && pane.total === 2 && !pane.listInFlight && pane.visibleItemFor(0)) {
            pane.selectAll()
            var key = action === "cut" ? Qt.Key_X : Qt.Key_C
            var resolved = Keymap.lookup(key, "", Qt.ControlModifier, "listing")
            var expected = action === "cut" ? "cut" : "copy"
            if (resolved !== expected) { log("FAIL key resolved " + resolved + ", want " + expected); quit(); return }
            pane.act(resolved)
            stage = 1
            ticks = 0
        } else if (stage === 1 && pane.clipboard.paths.length === 2) {
            if (pane.clipboard.moving !== (action === "cut")) {
                log("FAIL local-" + action + " wrong moving=" + pane.clipboard.moving)
                quit()
                return
            }
            log("INFO local-" + action + " paths=" + pane.clipboard.paths.join("|") + " moving=" + pane.clipboard.moving)
            stage = 2
            ticks = 0
        } else if (stage === 2 && (clipReplies.some(function (reply) { return reply.op === "set" })
                                  || ticks * probeTickMs >= clipboardReadTimeoutMs)) {
            var mark = action === "cut" ? "scissors" : "copy"
            log((pane.visibleItemFor(0).clipMark === mark && pane.visibleItemFor(1).clipMark === mark ? "PASS" : "FAIL")
                + " local-" + action + " selected-file-marks expected=" + mark)
            pane.open(destination)
            stage = 3
            ticks = 0
        } else if (stage === 3 && !pane.listInFlight && pane.path === destination) {
            // P must offer the link flyout in an empty destination as well as over a row.
            var linkAction = Keymap.lookup(Qt.Key_P, "P", Qt.ShiftModifier, "listing")
            pane.act(linkAction)
            log((pane.contextMenu().opened && pane.contextMenu().submenuOpen ? "PASS" : "FAIL")
                + " paste-as-destination action=" + linkAction + " opened=" + pane.contextMenu().opened
                + " flyout=" + pane.contextMenu().submenuOpen + " messages=" + JSON.stringify(messages))
            pane.contextMenu().close()
            if (action.indexOf("pasteas") === 0) {
                pane.act("pasteAs")
                log("INFO pasteas-reopen " + menuState())
                stage = 12
                ticks = 0
                return
            }
            // Positive control drives the real transfer path with this window's internal clipboard.
            pasteReadSequence = pane.clipboardState.getSequence
            pasteRequest = null
            pane.act("paste")
            stage = clipboardReadStage
            ticks = 0
        } else if (stage === clipboardReadStage && (pane.clipboardState.gets.length === 0
                                                    || ticks * probeTickMs >= clipboardReadTimeoutMs)) {
            var gets = clipReplies.filter(function (reply) { return reply.op === "get" })
            log((pane.clipboardState.getSequence === pasteReadSequence + 1 && pane.clipboardState.gets.length === 0
                 && gets.length === 1 && gets[0].ok === false && gets[0].error === clipboardRefusal ? "PASS" : "FAIL")
                + " local-paste-read-answered replies=" + JSON.stringify(gets))
            var paths = [source + "/alpha.txt", source + "/beta.txt"]
            log((pasteRequest && pasteRequest.c === "transfer" && pasteRequest.op === (action === "cut" ? "move" : "copy")
                 && pasteRequest.dest === destination && JSON.stringify(pasteRequest.paths) === JSON.stringify(paths) ? "PASS" : "FAIL")
                + " local-paste-asks-for-files pending=" + JSON.stringify(pasteRequest))
            stage = 4
            ticks = 0
        } else if (stage === 4 && ticks >= 10) {
            if (pane.total !== 2) log("FAIL local-paste-lands-files total=" + pane.total)
            stage = artifactStage
            artifactCheck.running = true
        } else if (stage === 12 && pane.menuActions.ready && pane.contextMenu().submenuOpen) {
            pane.contextMenu().chooseSub(linkLeaves[action])
            stage = 13
            ticks = 0
        } else if (stage === 13 && ticks >= 10) {
            if (pane.total !== 3) log("FAIL hidden-paste-as-links total=" + pane.total + " messages=" + JSON.stringify(messages))
            stage = artifactStage
            artifactCheck.running = true
        }
    }
}
