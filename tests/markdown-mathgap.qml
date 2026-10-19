//@ pragma ShellId flea-markdown-mathgap-test

import QtQuick
import Quickshell
import "flea" as Flea
import "markdown-figures-render.js" as Checks

// Real display formula images determine block bounds, after decoding and layout settle.
ShellRoot {
    id: shell
    property bool done: false
    property bool settling: false
    property bool larger: false
    property int checks: 0
    property int failures: 0
    property var capturedRects: []
    property string shotPath: Quickshell.env("XDG_RUNTIME_DIR") + "/markdown-mathgap.png"
    Component.onCompleted: Flea.ViewState.setTextSize({ mode: 14 })

    function log(line) { console.log("MARKDOWN_MATHGAP " + line) }
    function quit() { Quickshell.execDetached(["kill", String(Quickshell.processId)]) }
    function check(error, name) {
        shell.checks++
        if (error !== "") shell.failures++
        shell.log((error === "" ? "CHECK " : "FAIL ") + name + (error === "" ? "" : ": " + error))
    }

    FloatingWindow {
        implicitWidth: 560
        implicitHeight: 600
        color: "#101315"
        Item {
            id: grabRoot
            anchors.fill: parent
            Flea.PreviewMarkdown {
                id: md
                anchors.fill: parent
                active: true
                path: Quickshell.env("FLEA_MARKDOWN_FIGURE_FIXTURE")
                size: 1
                view: "rendered"
            }
        }
        Image {
            id: shot
            width: 1
            height: 1
            opacity: 0
            onStatusChanged: if (status === Image.Ready) probe.requestPaint()
        }
        Canvas {
            id: probe
            width: 560
            height: 600
            opacity: 0
            onPaint: if (shot.status === Image.Ready && !shell.larger) shell.pixels(getContext("2d"))
        }
    }

    function ready() {
        if (!md.contentReady || md.view !== "rendered" || md.blockList.length !== 9) return false
        for (var i = 0; i < 3; i++) {
            var index = [1, 2, 5][i]
            var fig = Checks.figure(md, index)
            var image = Checks.imageOf(fig)
            if (!fig || !fig.ready || fig.working || !image || image.status !== Image.Ready || image.height <= 0)
                return false
        }
        // Two consecutive mermaids settle like the display maths, and Pele draws each frame on its canvas edge, so the list's gap is the ink gap.
        for (var m = 7; m <= 8; m++) {
            var dia = Checks.figure(md, m)
            var poster = Checks.imageOf(dia)
            if (!dia || !dia.ready || dia.failed || dia.working || !poster || poster.status !== Image.Ready || poster.height <= 0)
                return false
        }
        return md.blockItem(8) !== null
    }

    // One figure's drawn image in the document frame: the rows no neighbour's ink can reach, since images never overlap.
    function imageRect(i) {
        var image = Checks.imageOf(Checks.figure(md, i))
        var at = image.mapToItem(md, 0, 0)
        return { x: at.x, y: at.y, w: image.width, h: image.height }
    }

    function geometry() {
        if (!shell.ready()) { shell.settling = false; return }
        var body = Flea.Theme.font.body
        var inset = Flea.Theme.spacing.gap
        shell.check(body === (shell.larger ? 16 : 14) ? "" : "unexpected text size " + body, "text size")
        shell.check(md.blockList.map(function (b) { return b.type }).join(",") === "run,figure,figure,fence,run,figure,run,figure,figure"
            ? "" : "fixture block types changed", "display block structure")
        var rects = []
        for (var i = 0; i < 9; i++)
            rects.push(Checks.drawnBlockRect(md, i, md, inset))
        var firstText = md.blockItem(0).children[0]
        shell.log("first paragraph rect=" + JSON.stringify(rects[0]) + " contentY=" + md.bodyItem.contentY
            + " visible=" + firstText.visible + " text=" + firstText.text)
        var pairs = [[1, 2, "consecutive display maths", md.blockGap], [2, 3, "maths to fence", md.blockGap],
            [4, 5, "paragraph to single maths", md.blockGap], [5, 6, "single maths to paragraph", md.blockGap]]
        for (var p = 0; p < pairs.length; p++) {
            var pair = pairs[p]
            var gap = rects[pair[1]].y - rects[pair[0]].y - rects[pair[0]].h
            shell.log(pair[2] + " gap=" + gap + " want=" + pair[3] + " body=" + body)
            shell.check(Checks.blockGapError(rects[pair[0]], rects[pair[1]], pair[3]),
                pair[2] + " at body " + body)
        }
        for (var f = 0; f < 3; f++) {
            var index = [1, 2, 5][f]
            var block = md.blockItem(index)
            var image = Checks.imageOf(Checks.figure(md, index))
            var at = image.mapToItem(block, 0, 0)
            shell.check(Math.abs(at.y - inset) <= 0.5
                && Math.abs(block.height - image.height - 2 * inset) <= 0.5 ? ""
                : "image y=" + at.y + " height=" + image.height + " block=" + block.height + " inset=" + inset,
                "maths " + index + " drawn height and inset at body " + body)
            shell.check(Math.abs(at.x + image.width / 2 - block.width / 2) <= 0.5 ? ""
                : "formula left its horizontal centre", "maths " + index + " centred at body " + body)
        }
        if (!shell.larger) {
            // Hold Rendered through the grab and pixel read, so capture cannot race the Source flip.
            shell.capturedRects = rects
            grabRoot.grabToImage(function (result) {
                shell.check(result.saveToFile(shell.shotPath) ? "" : "grab could not be saved", "base geometry grab")
                shot.source = "file://" + shell.shotPath
            })
        } else {
            shell.done = true
            shell.log(shell.checks + " checks, " + shell.failures + " failed")
            shell.quit()
        }
    }

    function pixels(ctx) {
        ctx.drawImage(shot, 0, 0)
        var data = ctx.getImageData(0, 0, 560, 600).data
        var indices = [0, 1, 2, 4, 5, 6, 7, 8]
        for (var i = 0; i < indices.length; i++) {
            var index = indices[i]
            var rect = shell.capturedRects[index]
            var ink = 0
            for (var y = Math.floor(rect.y); y < Math.ceil(rect.y + rect.h); y++)
                for (var x = Math.floor(rect.x); x < Math.ceil(rect.x + rect.w); x++) {
                    var offset = (y * 560 + x) * 4
                    var r = data[offset], g = data[offset + 1], b = data[offset + 2]
                    if ((r || g || b) && !(r === 16 && g === 19 && b === 21)) ink++
                }
            shell.check(ink > 2 ? "" : "captured item has no ink", "block " + index + " painted")
        }
        // Two consecutive figures stand one block gap apart, ink to ink: the scan runs over the disjoint image rows, never the padded rects.
        shell.check(Checks.inkGapError(data, 560, 600, imageRect(7), imageRect(8), md.blockGap), "consecutive mermaids stand one block gap apart")
        shell.larger = true
        md.view = "source"
        Flea.ViewState.setTextSize({ mode: 16 })
        flip.start()
    }

    Timer {
        interval: 100
        repeat: true
        running: !shell.done
        onTriggered: if (!shell.settling && shell.ready()) {
            shell.settling = true
            md.bodyItem.positionViewAtBeginning()
            settle.start()
        }
    }
    Timer { id: settle; interval: 300; onTriggered: shell.geometry() }
    Timer { id: flip; interval: 100; onTriggered: { md.view = "rendered"; shell.settling = false } }
    Timer {
        interval: 20000
        running: !shell.done
        onTriggered: {
            shell.done = true
            shell.log("FAIL display formula images never settled")
            shell.quit()
        }
    }
}
