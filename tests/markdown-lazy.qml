//@ pragma ShellId flea-markdown-lazy-test

import QtQuick
import Quickshell
import "flea" as Flea

// Render about 560 KiB from 2500 sections of five source blocks through the real lazy preview.
ShellRoot {
    id: shell

    function log(line) { console.log("MARKDOWN_LAZY " + line) }
    function quit() { Quickshell.execDetached(["kill", String(Quickshell.processId)]) }

    property string fixture: Quickshell.env("FLEA_MARKDOWN_FIXTURE")
    property bool done: false

    FloatingWindow {
        id: window
        implicitWidth: 560
        implicitHeight: 1120
        color: "#101315"

        Flea.PreviewMarkdown {
            id: md
            anchors.top: parent.top
            anchors.left: parent.left
            anchors.right: parent.right
            height: 1060
            active: true
            path: shell.fixture
            size: 1
            view: "rendered"
        }
    }

    Connections {
        target: md
        function onContentReadyChanged() { if (md.contentReady) Qt.callLater(shell.report) }
    }

    function report() {
        if (shell.done || !md.contentReady) return
        md.bodyItem.forceLayout()
        if (md.delegateCount() === 0) {
            shell.fail("no delegates after forceLayout")
            return
        }
        shell.log("blocks=" + md.blockList.length + " delegates=" + md.delegateCount()
            + " offthread=" + md.parsedOffThread)
        md.view = "source"
        Qt.callLater(shell.reportSource)
    }

    // Sample input: '<font color="#ffffff">a &amp; b</font>' reads back as 'a & b'.
    function decoded(markup) {
        return String(markup).replace(/<[^>]*>/g, "").replace(/&nbsp;/g, " ").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, "\"").replace(/&amp;/g, "&")
    }
    function reportSource() {
        var list = md.sourceItem
        list.forceLayout()
        var first = md.sourceChars
        list.positionViewAtEnd()
        list.forceLayout()
        var tail = md.rawText.split("\n").filter(function (l) { return l.indexOf("var section") === 0 }).pop()
        var reached = false
        for (var i = 0; i < list.contentItem.children.length; i++) {
            var row = list.contentItem.children[i]
            if (row.children !== undefined && row.children.length === 2 && shell.decoded(row.children[1].text).indexOf(tail) >= 0)
                reached = true
        }
        shell.log("source chars=" + first + " total=" + md.rawText.length + " endlaid=" + md.sourceChars + " lastline=" + reached)
        // Every row built in turn, read back to its line, is the file exactly.
        var rows = []
        var numbered = true
        for (var c = 0; c < list.count; c++) {
            var built = list.itemAtIndex(c)
            if (built === null) {
                list.positionViewAtIndex(c, ListView.Beginning)
                list.forceLayout()
                built = list.itemAtIndex(c)
            }
            numbered = numbered && built.children[0].text === String(c + 1)
            rows.push(shell.decoded(built.children[1].text))
        }
        // A terminal newline ends the last physical row; it adds no empty row, and reconstruction puts that byte back.
        var terminalLF = md.rawText.endsWith("\n")
        var physical = terminalLF ? md.rawText.slice(0, -1).split("\n") : md.rawText.split("\n")
        var noExtraRow = list.count === physical.length && rows[rows.length - 1] === physical[physical.length - 1] && rows[rows.length - 1] !== ""
        var joined = rows.join("\n") + (terminalLF ? "\n" : "")
        shell.log("source roundtrip chunks=" + list.count + " numbered=" + numbered + " terminalLF=" + terminalLF
            + " noExtraRow=" + noExtraRow + " same=" + (terminalLF && noExtraRow && numbered && joined === md.rawText))
        shell.done = true
        shell.quit()
    }

    Component.onCompleted: {
        if (shell.fixture.length === 0) shell.fail("no fixture arrived in FLEA_MARKDOWN_FIXTURE")
        else if (md.contentReady) Qt.callLater(shell.report)
    }

    // A hang bound on the wait for contentReady alone, about six times the slowest wait measured on a loaded host.
    readonly property int watchdogMs: 300000
    Timer {
        interval: shell.watchdogMs
        repeat: false
        running: !shell.done && !md.contentReady
        onTriggered: shell.fail("watchdog waiting for Markdown contentReady")
    }

    function fail(why) {
        if (shell.done)
            return
        shell.done = true
        shell.log("FAIL " + why)
        shell.quit()
    }
}
