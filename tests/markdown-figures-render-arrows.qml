//@ pragma ShellId flea-markdown-arrows-test
import QtQuick
import Quickshell
import Quickshell.Io
import "flea" as Flea

ShellRoot {
    id: shell
    property bool done: false
    property bool armed: false
    property int checks: 0
    property int failures: 0
    property string output: Quickshell.env("XDG_RUNTIME_DIR") + "/markdown-arrows.png"
    // A head at each end direction, a link marked at both ends, whose first mark is its start, and a back link that curves into its head.
    readonly property var arrowCases: ["flowchart TD\n    A --> B", "flowchart BT\n    A --> B", "flowchart LR\n    A --> B", "flowchart RL\n    A --> B",
        "flowchart TD\n    A <--> B", "flowchart TD\n    A --> B\n    B --> C\n    C --> A"]
    function quit() { Quickshell.execDetached(["kill", String(Quickshell.processId)]) }
    function check(ok, label) {
        checks++
        if (!ok) failures++
        console.log("MARKDOWN_ARROWS " + (ok ? "CHECK " : "FAIL ") + label)
    }
    FloatingWindow {
        implicitWidth: 1200
        implicitHeight: 640
        color: "#101315"
        Item {
            id: grab
            anchors.fill: parent
            Repeater {
                id: figures
                model: shell.arrowCases
                Flea.MarkdownFigure {
                    required property string modelData
                    required property int index
                    x: (index % 4) * 240
                    y: Math.floor(index / 4) * 320
                    width: 220
                    source: modelData
                    kind: "mermaid"
                    fontFamily: "monospace"
                    bodyPx: 14
                    bgHex: "#101315"
                    fgHex: "#c0caf5"
                    accentHex: "#7aa2f7"
                }
            }
        }
        Image { id: shot; width: 1; height: 1; opacity: 0 }
        Canvas {
            id: probe
            width: 1200
            height: 640
            opacity: 0
            onPaint: if (shell.armed) shell.analyze(getContext("2d"))
        }
    }
    // Sample input: one pele-edge group with a line and two marks answers one line per mark, each end its tip and each start the line end nearest it, so the two run along the head.
    // Every edge group is measured, so the C --> A back link of the last arrow case is never skipped for the first forward edge.
    function edges(svg) {
        var out = []
        var groups = svg.match(/<g class="pele-edge"[^>]*>[\s\S]*?<\/g>/g) || []
        for (var g = 0; g < groups.length; g++) {
            var id = (groups[g].match(/data-id="([^"]+)"/) || [null, ""])[1]
            var line = groups[g].match(/<path(?![^>]*pele-marker)[^>]*\sd="([^"]+)"/)
            var points = line ? line[1].match(/-?\d*\.?\d+/g).map(Number) : []
            var tipRe = /<path class="pele-marker" d="M(-?[\d.]+),(-?[\d.]+)/g
            var tip = tipRe.exec(groups[g])
            while (tip !== null && points.length >= 4) {
                var end = { x: Number(tip[1]), y: Number(tip[2]) }
                var first = { x: points[0], y: points[1] }, last = { x: points[points.length - 2], y: points[points.length - 1] }
                var near = Math.hypot(first.x - end.x, first.y - end.y) < Math.hypot(last.x - end.x, last.y - end.y) ? first : last
                out.push({ id: id, start: near, end: end })
                tip = tipRe.exec(groups[g])
            }
        }
        return out
    }
    function head(pixels, fig, line) {
        var at = fig.mapToItem(grab, 0, 0)
        var box = fig.svg.match(/viewBox="([^"]*)"/)[1].split(/\s+/).map(Number)
        var sx = fig.fitWidth / box[2], sy = fig.fitHeight / box[3]
        var x = at.x + (line.end.x - box[0]) * sx
        var y = at.y + (line.end.y - box[1]) * sy
        var dx = line.end.x - line.start.x, dy = line.end.y - line.start.y
        var length = Math.sqrt(dx * dx + dy * dy)
        dx /= length; dy /= length
        var count = 0, along = 0, cross = 0, tip = -Infinity, tipCross = 0
        var wide = 0, narrow = 0
        for (var py = Math.floor(y - 12); py <= Math.ceil(y + 12); py++)
            for (var px = Math.floor(x - 12); px <= Math.ceil(x + 12); px++) {
                var offset = (py * 1200 + px) * 4
                var r = pixels[offset], g = pixels[offset + 1], b = pixels[offset + 2]
                if (!(b > r + 70 && b > g + 60))
                    continue
                var a = (px + 0.5 - x) * dx + (py + 0.5 - y) * dy
                var c = (px + 0.5 - x) * -dy + (py + 0.5 - y) * dx
                count++; along += a; cross += c
                if (a > tip) { tip = a; tipCross = c }
                if (a >= -7 && a < -4) wide++
                if (a >= -2 && a <= 0) narrow++
            }
        return { count: count, along: along / count, cross: cross / count, tip: tip,
            tipCross: tipCross, wide: wide, narrow: narrow }
    }
    // One head per link end: a single edge, two for A <--> B, three where C --> A closes the loop.
    readonly property var edgeCounts: [1, 1, 1, 1, 2, 3]
    function analyze(ctx) {
        ctx.drawImage(shot, 0, 0)
        var pixels = ctx.getImageData(0, 0, 1200, 640).data
        for (var i = 0; i < figures.count; i++) {
            var fig = figures.itemAt(i)
            var label = fig.modelData.replace(/\n\s*/g, "; ")
            var lines = edges(fig.svg)
            check(lines.length === shell.edgeCounts[i], label + " measures every head, want " + shell.edgeCounts[i])
            for (var k = 0; k < lines.length; k++) {
                var line = lines[k]
                var tag = label + " " + line.id
                var h = head(pixels, fig, line)
                console.log("MARKDOWN_ARROWS PIXELS " + tag + " " + JSON.stringify(h))
                check(h.count >= 10 && Math.abs(h.cross) <= 1 && h.along < -2,
                    tag + " head centered behind edge end")
                check(h.tip >= -2 && h.tip <= 2 && Math.abs(h.tipCross) <= 1.5,
                    tag + " tip touches edge end")
                check(h.wide > h.narrow && h.narrow > 0, tag + " head tapers along edge direction")
            }
            // A head moved 18 px past its line end goes red, so a displaced back-link head cannot pass.
            if (lines.length > 0) {
                var back = lines[lines.length - 1]
                var bdx = back.end.x - back.start.x, bdy = back.end.y - back.start.y
                var blen = Math.sqrt(bdx * bdx + bdy * bdy)
                var off = head(pixels, fig, { start: back.start, end: { x: back.end.x + 18 * bdx / blen, y: back.end.y + 18 * bdy / blen } })
                check(!(off.count >= 10 && Math.abs(off.cross) <= 1 && off.along < -2)
                    && !(off.tip >= -2 && off.tip <= 2 && Math.abs(off.tipCross) <= 1.5)
                    && !(off.wide > off.narrow && off.narrow > 0), label + " displaced head goes red")
            }
        }
        shell.done = true
        console.log("MARKDOWN_ARROWS " + checks + " checks, " + failures + " failed")
        shell.quit()
    }
    Timer {
        id: settle
        interval: 700
        repeat: true
        running: !shell.done
        onTriggered: {
            if (figures.count !== shell.arrowCases.length)
                return
            for (var i = 0; i < figures.count; i++) {
                var fig = figures.itemAt(i)
                if (fig.failed) {
                    console.log("MARKDOWN_ARROWS FAIL helper " + fig.error)
                    shell.quit()
                    return
                }
                if (!fig.ready || fig.fitWidth <= 0 || fig.fitHeight <= 0)
                    return
            }
            settle.stop()
            grab.grabToImage(function(result) {
                if (!result.saveToFile(shell.output)) {
                    console.log("MARKDOWN_ARROWS FAIL capture")
                    shell.quit()
                }
                shot.source = "file://" + shell.output
            })
        }
    }
    Connections {
        target: shot
        function onStatusChanged() {
            if (shot.status === Image.Ready) {
                shell.armed = true
                probe.requestPaint()
            }
        }
    }
    Timer {
        interval: 15000
        running: !shell.done
        onTriggered: {
            console.log("MARKDOWN_ARROWS FAIL watchdog cases=" + shell.arrowCases.length + " delegates=" + figures.count)
            shell.quit()
        }
    }
}
