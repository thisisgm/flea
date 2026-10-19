//@ pragma ShellId flea-markdown-ends-test
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
    property string output: Quickshell.env("XDG_RUNTIME_DIR") + "/markdown-ends.png"
    // Every end mark of a link: each case names the marks its source draws at the start and at the end of its first edge.
    readonly property var endCases: [
        { name: "flowchart <-->", source: "flowchart LR\n    a <--> b", start: true, end: true },
        { name: "flowchart <-.->", source: "flowchart LR\n    a <-.-> b", start: true, end: true },
        { name: "flowchart <==>", source: "flowchart LR\n    a <==> b", start: true, end: true },
        { name: "flowchart o--o", source: "flowchart LR\n    a o--o b", start: true, end: true },
        { name: "flowchart x--x", source: "flowchart LR\n    a x--x b", start: true, end: true },
        { name: "flowchart top down <-->", source: "flowchart TD\n    a <--> b", start: true, end: true },
        { name: "flowchart --> draws only an end", source: "flowchart LR\n    a --> b", start: false, end: true },
        { name: "class composition at the start", source: "classDiagram\n    A *-- B", start: true, end: false },
        { name: "class aggregation at the start", source: "classDiagram\n    A o-- B", start: true, end: false },
        { name: "class composition at the end", source: "classDiagram\n    A --* B", start: false, end: true },
        { name: "class inheritance at the start", source: "classDiagram\n    A <|-- B", start: true, end: false },
        { name: "class plain link draws no mark", source: "classDiagram\n    A -- B", start: false, end: false }
    ]
    readonly property int columns: 5
    readonly property int cellWidth: 240
    readonly property int cellHeight: 260
    readonly property int canvasWidth: 1200
    readonly property int canvasHeight: 800
    // A mark is visible when this many accent pixels sit within the radius of an edge end.
    readonly property int markInkMin: 6
    readonly property int markRadius: 10
    // The accent is blue with a third of its blue missing from red, and the foreground only a fifth, so a thin antialiased stroke still tells them apart.
    readonly property int accentBlueMin: 40
    readonly property real accentRedGap: 0.35
    function quit() { Quickshell.execDetached(["kill", String(Quickshell.processId)]) }
    function check(ok, label) {
        checks++
        if (!ok) failures++
        console.log("MARKDOWN_ENDS " + (ok ? "CHECK " : "FAIL ") + label)
    }
    FloatingWindow {
        implicitWidth: shell.canvasWidth
        implicitHeight: shell.canvasHeight
        color: "#101315"
        Item {
            id: grab
            anchors.fill: parent
            Repeater {
                id: ends
                model: shell.endCases
                Flea.MarkdownFigure {
                    required property var modelData
                    required property int index
                    x: (index % shell.columns) * shell.cellWidth
                    y: Math.floor(index / shell.columns) * shell.cellHeight
                    width: shell.cellWidth - 20
                    source: modelData.source
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
            width: shell.canvasWidth
            height: shell.canvasHeight
            opacity: 0
            onPaint: if (shell.armed) shell.analyze(getContext("2d"))
        }
    }
    // Sample input: '<g class="pele-edge" data-id="e"><path d="M10,20L30,40"/></g>' answers { start: { x: 10, y: 20 }, end: { x: 30, y: 40 } }, the first link's line
    // and not its marks; an svg with no link answers null.
    function edge(svg) {
        var group = svg.match(/<g class="pele-edge(?: [^"]*)?"[^>]*>([\s\S]*?)<\/g>/)
        var line = group ? group[1].match(/<path(?![^>]*pele-marker)[^>]*\sd="([^"]+)"/) : null
        var points = line ? line[1].match(/-?\d*\.?\d+/g).map(Number) : []
        if (points.length < 4)
            return null
        return { start: { x: points[0], y: points[1] }, end: { x: points[points.length - 2], y: points[points.length - 1] } }
    }
    // Sample input: a figure whose svg reads viewBox="0 0 200 100", drawn 400 by 200, reads point (10, 20) at 20, 40 past its corner.
    // The accent pixels within markRadius of an edge end: what is left of a mark once the node drawn over it has hidden its base.
    function markInk(pixels, fig, point) {
        var at = fig.mapToItem(grab, 0, 0)
        var box = fig.svg.match(/viewBox="([^"]*)"/)[1].split(/\s+/).map(Number)
        var x = at.x + (point.x - box[0]) * fig.fitWidth / box[2]
        var y = at.y + (point.y - box[1]) * fig.fitHeight / box[3]
        var count = 0
        for (var py = Math.floor(y - shell.markRadius); py <= Math.ceil(y + shell.markRadius); py++)
            for (var px = Math.floor(x - shell.markRadius); px <= Math.ceil(x + shell.markRadius); px++) {
                var offset = (py * shell.canvasWidth + px) * 4
                var r = pixels[offset], g = pixels[offset + 1], b = pixels[offset + 2]
                if (b > shell.accentBlueMin && b - r > shell.accentRedGap * b && Math.hypot(px + 0.5 - x, py + 0.5 - y) <= shell.markRadius)
                    count++
            }
        return count
    }
    function analyze(ctx) {
        ctx.drawImage(shot, 0, 0)
        var pixels = ctx.getImageData(0, 0, shell.canvasWidth, shell.canvasHeight).data
        for (var e = 0; e < ends.count; e++) {
            var fig = ends.itemAt(e)
            var spec = fig.modelData
            var line = edge(fig.svg)
            check(line !== null, spec.name + " has an edge")
            if (!line) continue
            var startInk = markInk(pixels, fig, line.start)
            var endInk = markInk(pixels, fig, line.end)
            console.log("MARKDOWN_ENDS MARKS " + spec.name + " start=" + startInk + " end=" + endInk)
            check(spec.start ? startInk >= shell.markInkMin : startInk === 0, spec.name + (spec.start ? " draws its start mark" : " draws no start mark"))
            check(spec.end ? endInk >= shell.markInkMin : endInk === 0, spec.name + (spec.end ? " draws its end mark" : " draws no end mark"))
        }
        shell.done = true
        console.log("MARKDOWN_ENDS " + checks + " checks, " + failures + " failed")
        shell.quit()
    }
    Timer {
        id: settle
        interval: 700
        repeat: true
        running: !shell.done
        onTriggered: {
            if (ends.count !== shell.endCases.length)
                return
            for (var i = 0; i < ends.count; i++) {
                var fig = ends.itemAt(i)
                if (fig.failed) {
                    console.log("MARKDOWN_ENDS FAIL helper " + fig.error)
                    shell.quit()
                    return
                }
                if (!fig.ready || fig.fitWidth <= 0 || fig.fitHeight <= 0)
                    return
            }
            settle.stop()
            grab.grabToImage(function(result) {
                if (!result.saveToFile(shell.output)) {
                    console.log("MARKDOWN_ENDS FAIL capture")
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
            console.log("MARKDOWN_ENDS FAIL watchdog delegates=" + ends.count)
            shell.quit()
        }
    }
}
