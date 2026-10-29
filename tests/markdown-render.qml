//@ pragma ShellId flea-markdown-render-test

import QtQuick
import QtTest
import Quickshell
import "flea" as Flea
import "flea/js/Focus.js" as Focus
import "markdown-mark.js" as Mark
import "markdown-render.js" as Checks
import "markdown-centre.js" as Centre
import "markdown-board.js" as Board
import "markdown-bar.js" as Bar

// tests/markdown-render.sh's harness: the real ui/PreviewMarkdown.qml over a fixture
// document, grabbed and judged on pixel facts, offscreen. Quits itself, pass or fail.
ShellRoot {
    id: shell

    function log(line) { console.log("MARKDOWN_RENDER " + line) }
    function quit() { Quickshell.execDetached(["kill", String(Quickshell.processId)]) }

    property string fixture: Quickshell.env("FLEA_MARKDOWN_FIXTURE")
    property string longFixture: Quickshell.env("FLEA_MARKDOWN_LONG")
    property string shotPath: Quickshell.env("XDG_RUNTIME_DIR") + "/markdown-render-" + Quickshell.processId
        + (shell.larger ? "-large.png" : "-base.png")
    property bool done: false
    // The Canvas paints on its own when it loads, so analysis runs only once armed behind a grab.
    property bool armed: false
    // The native cap_markdown case flips Rendered/Source twice (r, r); the driver
    // below replays both flips before the grab, or the contentHeight loop never
    // fires offscreen. A resize alone does not trigger it.
    property int driveStep: 0
    property int failures: 0
    property var savedState: ({})
    // The probe's word is wider than any two of its letters beside a space, so each of its three copies wraps onto its own line.
    readonly property string probeWord: "HEHEH"
    readonly property int probeLines: 3
    readonly property int probeGap: 6
    readonly property int probeColumns: 4
    property int suiteBody: 0
    property bool larger: false
    // The narrowest Quick Look the bar is held to.
    readonly property int quickLookMinWidth: 480
    // What Focus.handleKey reads of a pane while Quick Look is open: the open Quick Look and a listing that owns nothing.
    readonly property var keyHost: ({ preview: quick, renameEditor: function () { return null } })

    FontMetrics { id: listMetrics; font.family: Flea.Theme.font.family; font.pixelSize: Flea.Theme.font.body }

    function listSamples() {
        var samples = []
        for (var i = 0; i < md.blockList.length; i++) {
            var b = md.blockList[i]
            if (b.type === "list")
                for (var r = 0; r < b.items.length; r++)
                    samples.push({ marker: b.ordered ? (b.start + r) + "." : String.fromCharCode(8226), text: b.items[r] })
        }
        return samples
    }

    function referenceLine(marker, text) {
        for (var i = 0; i < references.count; i++) {
            var item = references.itemAt(i)
            if (item.sample.marker === marker && item.sample.text === text)
                return { item: item, split: listMetrics.advanceWidth(marker + "  ") }
        }
        return null
    }
    // Exercise FileView load completion even when the new path leaves its text unchanged.
    property var loadCases: [
        { suffix: ".empty.md", text: "", name: "empty Markdown" },
        { suffix: ".first.md", text: "# Identical\n", name: "first identical Markdown" },
        { suffix: ".second.md", text: "# Identical\n", name: "second identical Markdown" },
        { suffix: ".gaps.md", text: "before\n\n## \n\n>\n\nafter\n", name: "empty heading and quote", empties: [1, 2] }
    ]
    property int loadStep: 0
    property int loadSeq: 0
    // Match the existing render settle allowance for each native FileView load.
    readonly property int settleMs: 1200

    FloatingWindow {
        id: window
        implicitWidth: 560
        implicitHeight: 1560
        color: "#101315"

        Item {
            id: grabRoot
            anchors.top: parent.top
            anchors.left: parent.left
            anchors.right: parent.right
            height: 1080

            Flea.PreviewMarkdown {
                id: md
                anchors.top: parent.top
                anchors.left: parent.left
                anchors.right: parent.right
                height: 1060
                active: true
                Component.onCompleted: {
                    shell.loadSeq = md.parseSeq
                    md.path = shell.fixture + shell.loadCases[0].suffix
                }
                size: 1
                view: "rendered"
            }

            Column {
                id: referenceColumn
                anchors.bottom: parent.bottom
                anchors.left: parent.left
                anchors.leftMargin: md.insetX
                Repeater {
                    id: references
                    model: shell.listSamples()
                    delegate: Flea.MarkdownText {
                        required property var modelData
                        readonly property var sample: modelData
                        textFormat: Text.RichText
                        text: sample.marker + "&nbsp;&nbsp;&nbsp;&nbsp;" + sample.text
                        wrapMode: Text.NoWrap
                    }
                }
            }

            // Flat capitals, one word a line: the real MarkdownText as a body, heading, quote and table cell set it, judged on every wrapped line.
            Grid {
                id: centreProbes
                anchors.bottom: referenceColumn.top
                anchors.left: referenceColumn.left
                columns: shell.probeColumns
                spacing: shell.probeGap
                Repeater {
                    id: centres
                    // px 0 follows the suite's text size; the others are the board's own 14 px body and 20 px heading.
                    model: [{ name: "body", px: 0, pad: 0, bold: false }, { name: "h1", px: -1, pad: 0, bold: true },
                        { name: "quote", px: 0, pad: 0, bold: false }, { name: "cell", px: 0, pad: 2, bold: false },
                        { name: "board body", px: 14, pad: 0, bold: false }, { name: "board h1", px: 20, pad: 0, bold: true },
                        { name: "board quote", px: 14, pad: 0, bold: false }, { name: "board cell", px: 14, pad: 2, bold: false }]
                    delegate: Flea.MarkdownText {
                        id: centreText
                        required property var modelData
                        readonly property real centred: (box - probeMetrics.height) / 2 + probeMetrics.ascent
                        width: Math.ceil(probeMetrics.advanceWidth(shell.probeWord)) + shell.probeGap
                        bodyPx: modelData.px > 0 ? modelData.px : modelData.px < 0 ? md.headingPx(1) : Flea.Theme.font.body
                        cellPad: modelData.pad
                        font.bold: modelData.bold
                        color: Flea.Theme.color.foreground
                        text: [shell.probeWord, shell.probeWord, shell.probeWord].join(" ")
                        FontMetrics { id: probeMetrics; font: centreText.font }
                    }
                }
            }
        }

        // Loaders only: the grab reads md, so neither paints into it.
        // Quick Look's own component: the board's bar order and code surface live there, so the suite reads through it.
        Flea.MarkdownPane {
            id: pane
            width: 900
            height: 1000
            visible: false
            active: true
            path: shell.fixture
            size: 1
            view: "rendered"
        }

        // The preview column's own setting of the document, one token under Quick Look's body, built beside the grab and not under it.
        Flea.PreviewMarkdown {
            id: colMd
            x: window.implicitWidth
            width: 560
            height: 600
            active: true
            compact: true
            path: shell.fixture
            size: 1
            view: "rendered"
        }

        // The bar without its segment, as a code or text pane draws it, over a name too long for it; shown beside the grab, because an item reports its effective visibility.
        Flea.QuickLookBar {
            id: plainBar
            x: window.implicitWidth
            y: colMd.height
            width: 900
            markName: "markdown"
            name: shell.longFixture.substring(shell.longFixture.lastIndexOf("/") + 1)
            countLabel: "12 lines"
        }

        // A name too long for any bar, so it elides and the room the name may take is tested.
        Flea.MarkdownPane {
            id: longNamePane
            width: 900
            height: 1000
            visible: false
            active: true
            path: shell.longFixture
            size: 1
        }

        // Quick Look itself, open on the fixture under the grab: a bar click and the r and Tab keys reach its pane through ui/Preview.qml's own wiring.
        Item {
            id: quickHost
            y: grabRoot.height
            width: shell.quickLookMinWidth
            height: window.implicitHeight - grabRoot.height

            Flea.Preview {
                id: quick
                active: true
                kind: "text"
                path: shell.fixture
                size: 1
            }
        }

        // The key sink the window's Pane is: its Keys.onPressed hands the event to Focus.handleKey.
        Item {
            id: keySink
            focus: true
            Keys.onPressed: function (event) { event.accepted = Focus.handleKey(event, shell.keyHost, null) }
        }

        TestEvent { id: events }

        Flea.MarkdownFigure {
            id: fallbackProbe
            width: 420
            visible: false
            askArmed: false
            fencePadX: md.fencePadX
            fencePadY: md.fencePadY
            source: "first source line\nsecond source line"
        }

        Image { id: shot; width: 1; height: 1; opacity: 0 }

        Canvas {
            id: probe
            width: 560
            height: 1080
            opacity: 0
            onPaint: {
                if (shell.armed && !shell.done)
                    shell.analyze(getContext("2d"))
            }
        }
    }

    function blockIndex(type, text, wantOrdered) {
        for (var i = 0; i < md.blockList.length; i++) {
            var b = md.blockList[i]
            if (b.type !== type)
                continue
            if (wantOrdered !== undefined && b.ordered !== wantOrdered)
                continue
            if (text !== undefined && String(b.text || "").indexOf(text) < 0)
                continue
            return i
        }
        return -1
    }

    // Window coordinates, which are the grab's pixels: the list's top margin moves its content below the frame's top.
    function rectOf(i) {
        var item = md.blockItem(i)
        var at = item.mapToItem(grabRoot, 0, 0)
        return { x: Math.round(at.x), y: Math.round(at.y),
                 w: Math.round(item.width), h: Math.round(item.height) }
    }

    // A list block's first marker, in window coordinates.
    function markerX(i) {
        var mark = Checks.markerOf(md.blockItem(i))
        return mark === null ? -1 : Math.round(mark.mapToItem(grabRoot, 0, 0).x)
    }

    function check(error, name) {
        shell.log((error === "" ? "CHECK " : "FAIL ") + name + (error === "" ? "" : ": " + error))
        if (error !== "")
            shell.failures++
    }

    // Both lists are measured at the suite's size and a larger supported text-size stop.
    function listBaselines(inkAt) {
        var body = Flea.Theme.font.body
        for (var i = 0; i < md.blockList.length; i++) {
            var block = md.blockList[i]
            if (block.type === "list")
                shell.check(Checks.listBaselineError(md.blockItem(i), md, block.items.length, body, grabRoot, inkAt, shell.referenceLine),
                    (block.ordered ? "ordered" : "bullet") + " baselines at body " + body)
        }
    }

    // CSS half-leading: the baseline sits half the leftover box above the font's ascent, read off the item's own font.
    function centredBaseline(t) {
        realMetrics.font = t.font
        return (t.box - realMetrics.height) / 2 + realMetrics.ascent
    }

    FontMetrics { id: realMetrics }

    // Every line of the probes, and the first line of the real h1, paragraph, quote and table cells, sits where CSS centres it in its 1.7 box.
    function lineCentres(inkAt) {
        for (var i = 0; i < centres.count; i++) {
            var probe = centres.itemAt(i)
            shell.check(Centre.lineCentreError(probe, grabRoot, inkAt, shell.probeLines, probe.centred, "probe " + probe.modelData.name),
                "centred lines of the " + probe.modelData.name + " probe at " + probe.font.pixelSize + " px")
        }
        var h1 = md.blockItem(shell.blockIndex("heading"))
        var quote = md.blockItem(shell.blockIndex("quote"))
        var para = md.blockItem(shell.blockIndex("run"))
        var table = md.blockItem(shell.blockIndex("table"))
        var cells = Centre.cellsOf(table)
        var real = [{ name: "heading", text: Checks.textOf(h1) }, { name: "paragraph", text: Checks.textOf(para) },
            { name: "quote", text: Centre.quoteTextOf(quote) }, { name: "table header cell", text: cells[0] }, { name: "table body cell", text: Centre.bodyCellOf(table, cells) }]
        for (var r = 0; r < real.length; r++) {
            var t = real[r].text
            shell.check(t ? Centre.lineCentreError(t, grabRoot, inkAt, 1, shell.centredBaseline(t), "real " + real[r].name) : "real " + real[r].name + " is absent",
                "centred first line of the real " + real[r].name)
        }
    }

    // The probes and the reference column live in the grab below the document, so a long document must never reach them.
    function probesClear() {
        var last = md.blockItem(md.blockList.length - 1)
        var top = centreProbes.mapToItem(grabRoot, 0, 0).y
        shell.check(last ? Centre.probeOverlapError(top, last.mapToItem(grabRoot, 0, 0).y + last.height) : "the document's last block is absent",
            "probe grid lies below the document's last block")
    }

    // The board's geometry against the live tree: insets, rhythm, headings, line boxes, the code surface and the bar.
    function geometry() {
        var rects = []
        var texts = []
        var body = Flea.Theme.font.body
        // md_rendered is drawn at body 14, so the board's own numbers are judged only where the box runs at 14.
        shell.log("body=" + body + " inset=" + md.insetY + "," + md.insetX + " gap=" + md.blockGap)
        var ink = String(Flea.Theme.color.foreground)
        var h1 = null
        var h2 = null
        var para = null
        for (var i = 0; i < md.blockList.length; i++) {
            var type = md.blockList[i].type
            rects.push(shell.rectOf(i))
            var text = type === "run" || type === "heading" ? Checks.textOf(md.blockItem(i)) : null
            if (text === null)
                continue
            texts.push({ name: type + " " + i, text: text, h: Math.round(md.blockItem(i).height) })
            if (type === "heading" && md.blockList[i].level === 1) h1 = text
            else if (type === "heading" && md.blockList[i].level === 2) h2 = text
            else if (type === "run" && para === null) para = text
        }
        shell.check(Checks.insetError(rects, md.width, md.insetX, md.insetY, body), "document inset")
        shell.check(Checks.rhythmError(rects, Board.boardPx(Board.BOARD_BLOCK_GAP, Flea.Theme.font.body)), "block rhythm")
        shell.check(Checks.headingError(h1, h2, para, body, ink, String(Flea.Theme.color.foregroundBright)), "heading sizes and ink")
        shell.check(Checks.lineBoxError(texts), "line boxes")
        var ruleTable = md.blockItem(shell.blockIndex("table"))
        shell.check(Checks.ruleError(ruleTable, pane.barGeometry ? pane.barGeometry().bar : null, Flea.Theme.spacing.hairline),
            "table rules take the bar's hairline")
        shell.check(Checks.quoteBoxError(md.blockItem(shell.blockIndex("quote")), md), "quote bar spans line box")
        var tableIndex = shell.blockIndex("table")
        var table = md.blockList[tableIndex]
        shell.check(Checks.tableBaselineError(md.blockItem(tableIndex), md, table.rows.length + 1, table.cols),
            "table header and body baselines")
        shell.check(Checks.remoteLineError(md.blockItem(shell.blockIndex("remote")), md), "remote placeholder centres")
        shell.check(Checks.fencePadError(Checks.fallbackOf(fallbackProbe), Board.boardPx(Board.BOARD_FENCE_PAD_X, body), Board.boardPx(Board.BOARD_FENCE_PAD_Y, body)),
            "figure fallback padding")
        var fence = Checks.fenceOf(md.blockItem(shell.blockIndex("fence")))
        shell.check(Checks.surfaceError(fence, String(Flea.Theme.color.surface), String(Flea.Theme.color.background)), "column fence surface")
        shell.check(Checks.fencePadError(fence, Board.boardPx(Board.BOARD_FENCE_PAD_X, body), Board.boardPx(Board.BOARD_FENCE_PAD_Y, body)), "fence padding")
        shell.check(Board.headerInkError(md.blockItem(shell.blockIndex("table")), String(Flea.Theme.color.foregroundBright), ink),
            "table header cells take the heading ink")
        var columnHeads = { h1: null, h2: null, para: null }
        for (var c = 0; c < colMd.blockList.length; c++) {
            var colType = colMd.blockList[c].type
            var colText = colMd.blockItem(c) ? Checks.textOf(colMd.blockItem(c)) : null
            if (colText === null)
                continue
            if (colType === "heading" && colMd.blockList[c].level === 1) columnHeads.h1 = colText
            else if (colType === "heading" && colMd.blockList[c].level === 2) columnHeads.h2 = colText
            else if (colType === "run" && columnHeads.para === null) columnHeads.para = colText
        }
        shell.check(Board.compactHeadingError(columnHeads.h1, columnHeads.h2, columnHeads.para, body, Flea.Theme.font.bodySmall),
            "column headings keep the board's sizes over the smaller body")
        // The close mark carries the keyboard's state out to the IPC, and rests unfocused.
        var restFocus = pane.closeState().focused
        pane.closeFocused = true
        var keyFocus = pane.closeState().focused && pane.barGeometry().close.keyboardFocused
        pane.closeFocused = false
        shell.check(restFocus === false && keyFocus === true && pane.closeState().focused === false ? ""
            : "focused at rest " + restFocus + ", with the keyboard " + keyFocus, "the close mark reports the keyboard focus")
        var qlFence = pane.blockItem ? Checks.fenceOf(pane.blockItem(shell.blockIndex("fence"))) : null
        shell.check(Checks.surfaceError(qlFence, String(Flea.Theme.color.background), String(Flea.Theme.color.surface)), "Quick Look fence surface")
        var g = pane.barGeometry ? pane.barGeometry() : null
        var tokens = shell.barTokens()
        shell.check(Bar.barError(g, tokens), "Quick Look bar order")
        shell.check(Bar.pressError(g.bar, g.close, g.select, g.copy), "Quick Look press targets belong to selection, copy and close")
        shell.check(Bar.nameRoomError(longNamePane.barGeometry ? longNamePane.barGeometry() : null, tokens), "Quick Look name takes the freed room")
        shell.check(Mark.visibilityControlError(plainBar.geometry(), tokens,
            function (g, t) { return Bar.plainBarChecks(g, longNamePane.barGeometry(), t) }), "Quick Look bar without the segment keeps its count one gap before close")
        shell.barSqueezed(tokens)
        shell.barDriven(tokens)
    }

    // The tokens the bar is drawn from, read live so the larger text size is judged against its own numbers.
    function barTokens() {
        return { chromeMark: Flea.Theme.chromeMarkSize, padX: Flea.Theme.spacing.rowPaddingX, gap: Flea.Theme.spacing.gap,
            chromeHeight: Flea.Theme.chromeHeight, controlHeight: Flea.Theme.chromeControlHeight, hairline: Flea.Theme.spacing.hairline,
            caption: Flea.Theme.font.caption,
            foreground: String(Flea.Theme.color.foreground), muted: String(Flea.Theme.color.muted) }
    }

    // At 480 px the long name elides; below that the count hides before close moves.
    function barSqueezed(tokens) {
        var was = Bar.fixedSizes(longNamePane.barGeometry())
        var wide = longNamePane.width
        longNamePane.width = shell.quickLookMinWidth
        shell.check(Bar.squeezeError(was, longNamePane.barGeometry(), tokens, false) || Bar.nameRoomError(longNamePane.barGeometry(), tokens),
            "Quick Look bar at " + shell.quickLookMinWidth + " px keeps close clear of the name")
        longNamePane.width = Bar.hidingWidth(was, tokens)
        shell.check(Bar.squeezeError(was, longNamePane.barGeometry(), tokens, true), "Quick Look bar hides the count before it moves close")
        longNamePane.width = wide
    }

    // Plain labels cannot flip a view; r flips only the body, and Tab visits close alone.
    function barDriven(tokens) {
        var shown = quick.markdownShown
        var g = shown.barGeometry()
        keySink.forceActiveFocus()
        function reads(view, name) {
            var why = shown.shownView === view ? Bar.barError(shown.barGeometry(), tokens)
                : "the body shows " + shown.shownView + ", want " + view
            shell.check(why, name)
        }
        reads("rendered", "Quick Look starts rendered with no view control")
        events.mouseClick(g.name, g.name.width / 2, g.name.height / 2, Qt.LeftButton, Qt.NoModifier, 0)
        reads("rendered", "a click on the filename keeps rendered")
        events.mouseClick(g.lines, g.lines.width / 2, g.lines.height / 2, Qt.LeftButton, Qt.NoModifier, 0)
        reads("rendered", "a click on the count keeps rendered")
        events.keyClickChar("r", Qt.NoModifier, -1)
        reads("source", "r flips the body to Source and adds no bar control")
        events.keyClickChar("r", Qt.NoModifier, -1)
        reads("rendered", "r returns the body to rendered and adds no bar control")
        var rest = quick.markdownCloseFocused || shown.closeState().focused
        events.keyClick(Qt.Key_Tab, Qt.NoModifier, -1)
        var onClose = quick.markdownCloseFocused && shown.closeState().focused && shown.barGeometry().close.keyboardFocused
        shell.check(rest ? "the close mark held the keyboard before any Tab" : !onClose ? "Tab did not put the keyboard on the close mark"
            : !keySink.activeFocus ? "Tab moved Qt focus off the key sink" : Bar.tabWalkError(g), "Tab lands on close and no other bar item takes focus")
        events.keyClick(Qt.Key_Tab, Qt.NoModifier, -1)
        shell.check(quick.markdownCloseFocused || shown.closeState().focused ? "a second Tab left the keyboard on the close mark" : "", "a second Tab takes the keyboard off the close mark")
    }

    function fail(why) {
        if (shell.done)
            return
        shell.done = true
        shell.log("FAIL " + why)
        shell.quit()
    }

    function pass(note) {
        if (shell.done)
            return
        shell.done = true
        shell.log("PASS " + note)
        shell.quit()
    }

    Timer {
        id: settle
        interval: shell.settleMs
        repeat: false
        running: true
        onTriggered: {
            if (shell.loadStep < shell.loadCases.length) {
                var test = shell.loadCases[shell.loadStep]
                var ready = md.contentReady && md.rawText === test.text && md.appliedSeq > shell.loadSeq && Board.emptiesTakeNoHeight(md, test.empties || [])
                shell.log((ready ? "ok " : "FAIL ") + test.name + " contentReady=" + md.contentReady + " status=" + md.status + " parseSeq=" + md.parseSeq + " appliedSeq=" + md.appliedSeq)
                if (!ready) {
                    shell.fail("load completion missed " + test.name)
                    return
                }
                shell.loadStep++
                shell.loadSeq = md.parseSeq
                md.path = shell.fixture + (shell.loadCases[shell.loadStep] || { suffix: "" }).suffix
                settle.restart()
                return
            }
            if (shell.fixture.length === 0)
                shell.fail("no fixture arrived in FLEA_MARKDOWN_FIXTURE")
            else if (!md.contentReady || !pane.contentReady || !longNamePane.contentReady || !quick.markdownShown || !quick.markdownShown.contentReady || !colMd.contentReady)
                shell.fail("the document never loaded")
            else if (md.flickContentHeight > md.height)
                shell.fail("the fixture overflowed its frame")
            else {
                shell.log("blocks=" + md.blockList.map(function (b) { return b.type }).join(",")
                    + " border=" + md.borderHex + " ink=" + md.inkHex + " chrome=" + md.chromeHex
                    + " run0=" + JSON.stringify(String(md.blockList[0].text).slice(0, 120))
                    + " content=" + Math.round(md.flickContentHeight))
                quick.revealNow()
                driver.start()
            }
        }
    }

    // The r, r flip pair: each step settles before the next, and the grab waits
    // one extra step past the flip back to rendered.
    Timer {
        id: driver
        interval: 350
        repeat: true
        running: false
        onTriggered: {
            shell.driveStep++
            shell.log("step=" + shell.driveStep + " content=" + Math.round(md.flickContentHeight))
            if (shell.driveStep === 1) md.view = "source"
            else if (shell.driveStep === 2) md.view = "rendered"
            else {
                driver.stop()
                fallbackProbe.error = "fixture refusal"
                shell.log("grabbing")
                grabRoot.grabToImage(shell.grabbed)
            }
        }
    }

    // The larger layout must settle before its second grab, while the base-size failures remain counted.
    Timer {
        id: largerSettle
        interval: 350
        onTriggered: {
            fallbackProbe.error = "fixture refusal"
            grabRoot.grabToImage(shell.grabbed)
        }
    }

    Timer {
        interval: 30000
        repeat: false
        running: !shell.done
        onTriggered: shell.fail("the watchdog outlived the verdict")
    }

    function grabbed(result) {
        if (!result.saveToFile(shell.shotPath)) {
            shell.fail("the grab could not be saved")
            return
        }
        shell.log("grab " + shell.shotPath)
        shot.source = "file://" + shell.shotPath
    }

    Connections {
        target: shot
        function onStatusChanged() {
            if (shot.status === Image.Ready) {
                shell.armed = true
                probe.requestPaint()
            } else if (shot.status === Image.Error)
                shell.fail("the saved grab would not reload")
        }
    }

    // Pixel facts off the reloaded grab, in image coordinates, which are the
    // component's own: the grab reads root at its size, md sitting at its top.
    function analyze(ctx) {
        var w = 560
        var h = 1080
        ctx.clearRect(0, 0, w, h)
        ctx.drawImage(shot, 0, 0)
        var pixels = ctx.getImageData(0, 0, w, h).data
        function at(x, y) {
            var o = (y * w + x) * 4
            return [pixels[o], pixels[o + 1], pixels[o + 2]]
        }
        function same(a, b) { return a[0] === b[0] && a[1] === b[1] && a[2] === b[2] }
        function parse(s) {
            return [parseInt(s.slice(1, 3), 16), parseInt(s.slice(3, 5), 16), parseInt(s.slice(5, 7), 16)]
        }
        shell.geometry()
        var border = parse(String(md.borderHex).toLowerCase())
        var chrome = parse(String(md.chromeHex).toLowerCase())
        // The CI theme's foreground is its own gray, never the board's #c0caf5, so ink reads
        // off the component beside the border rather than off a hardcoded token.
        var fg = parse(String(md.inkHex).toLowerCase())
        var ground = parse(String(window.color).toLowerCase())
        // The table's rules are the foreground at the bar's wash, and the grab composites over black, so the pixel is that share of the ink, within the renderer's rounding.
        var ruleWash = 0.12
        var rule = [0, 1, 2].map(function (k) { return Math.round(fg[k] * ruleWash) })
        var ruleSlack = 2
        function isRule(c) { return Math.abs(c[0] - rule[0]) <= ruleSlack && Math.abs(c[1] - rule[1]) <= ruleSlack && Math.abs(c[2] - rule[2]) <= ruleSlack }
        function inkAt(x, y) {
            if (x < 0 || x >= w || y < 0 || y >= h)
                return false
            var c = at(x, y)
            var toInk = 0
            var toGround = 0
            for (var i = 0; i < 3; i++) {
                toInk += Math.abs(c[i] - fg[i])
                toGround += Math.abs(c[i] - ground[i])
            }
            return toInk < toGround
        }
        shell.listBaselines(inkAt)
        shell.lineCentres(inkAt)
        shell.probesClear()
        if (shell.larger) {
            shell.check(Flea.Theme.font.body > shell.suiteBody ? "" : "text size did not grow", "larger text size")
            Flea.ViewState.state = shell.savedState
            if (shell.failures > 0)
                return shell.fail(shell.failures + " geometry checks failed")
            return shell.pass(shell.loadStep + " load completions, run, chip, markers at two sizes, rules, fence, box, bar and links all read")
        }

        // The chrome chip behind the inline code, confined to its line, never full-bleed.
        var codeIdx = shell.blockIndex("run", "<code")
        if (codeIdx < 0)
            return shell.fail("no run carried a styled code span")
        var run = shell.rectOf(codeIdx)
        var chip = 0
        var chipCols = {}
        for (var y = run.y; y < run.y + run.h; y++)
            for (var x = run.x; x < run.x + run.w; x++)
                if (same(at(x, y), chrome)) {
                    chip++
                    chipCols[x] = true
                }
        // The paragraph is one line box, so the chip is told from a full-width bar by its columns, not its rows.
        var chipColCount = Object.keys(chipCols).length
        shell.log("chip px=" + chip + " cols=" + chipColCount + " of " + run.w)
        if (chip < 40)
            return shell.fail("the chrome surface is missing behind the inline code")
        if (chipColCount > run.w * 0.4)
            return shell.fail("the chrome paint is no chip")

        // The leftmost ink column over a y band, for marker and paragraph alignment.
        // Any ink counts, not exact foreground: a thin marker antialiases without a pure
        // core pixel, while empty canvas reads as window background or transparent black.
        function painted(c) {
            return !((c[0] === 16 && c[1] === 19 && c[2] === 21) || (c[0] === 0 && c[1] === 0 && c[2] === 0))
        }
        function firstInk(rect, y0, y1) {
            for (var fx = rect.x; fx < rect.x + 40; fx++)
                for (var fy = y0; fy < y1; fy++)
                    if (painted(at(fx, fy)))
                        return fx
            return -1
        }
        var para0 = shell.rectOf(shell.blockIndex("run"))
        var orderedIdx = shell.blockIndex("list", undefined, true)
        if (orderedIdx < 0)
            return shell.fail("no ordered list block arrived")
        var orderedRect = shell.rectOf(orderedIdx)
        var orderedX = firstInk(orderedRect, orderedRect.y, orderedRect.y + 22)
        var orderedMark = shell.markerX(orderedIdx)
        var bulletIdx = shell.blockIndex("list", undefined, false)
        if (bulletIdx < 0)
            return shell.fail("no bullet list block arrived")
        var bulletRect = shell.rectOf(bulletIdx)
        var bulletX = firstInk(bulletRect, bulletRect.y, bulletRect.y + 22)
        var bulletMark = shell.markerX(bulletIdx)
        // The marker boxes share the paragraph's edge exactly; the ink sits a glyph bearing inside its own box.
        shell.log("marker box x ordered=" + orderedMark + " bullet=" + bulletMark + " paragraph=" + para0.x
            + ", ink ordered=" + orderedX + " bullet=" + bulletX)
        if (orderedMark !== para0.x || orderedX < orderedMark || orderedX - orderedMark > 3)
            return shell.fail("the ordered marker left the paragraph edge")
        if (bulletMark !== para0.x || bulletX < bulletMark || bulletX - bulletMark > 3)
            return shell.fail("the bullet marker left the paragraph edge")

        var tableIdx = shell.blockIndex("table")
        if (tableIdx < 0)
            return shell.fail("no table block arrived")
        var table = shell.rectOf(tableIdx)
        // A rule spans the hugged table, far narrower than the full-width delegate.
        var rules = 0
        for (var ry = table.y; ry < table.y + table.h; ry++) {
            var span = 0
            for (var rx = table.x; rx < table.x + table.w; rx++)
                if (isRule(at(rx, ry)))
                    span++
            if (span >= 100)
                rules++
        }
        var verticals = 0
        for (var cx = table.x; cx < table.x + table.w; cx++) {
            var drop = 0
            for (var cy = table.y; cy < table.y + table.h; cy++)
                if (isRule(at(cx, cy)))
                    drop++
            if (drop >= table.h * 0.75)
                verticals++
        }
        shell.log("rules=" + rules + " verticals=" + verticals)
        if (rules < 3)
            return shell.fail("the table rules never painted")
        if (verticals > 0)
            return shell.fail("the table drew vertical rules")
        // The table hugs its content: rules start at the left edge, end well short of the
        // frame, track the ink, and a clean vertical gap separates the columns.
        var ruleStart = table.x + table.w
        var ruleEnd = table.x
        var ruleRows = {}
        for (var qy = table.y; qy < table.y + table.h; qy++) {
            var qfirst = -1
            var qlast = -1
            for (var qx = table.x; qx < table.x + table.w; qx++)
                if (isRule(at(qx, qy))) {
                    if (qfirst < 0)
                        qfirst = qx
                    qlast = qx
                }
            if (qlast - qfirst >= 100) {
                ruleRows[qy] = true
                ruleStart = Math.min(ruleStart, qfirst)
                ruleEnd = Math.max(ruleEnd, qlast)
            }
        }
        var inkRight = table.x
        for (var iy = table.y; iy < table.y + table.h; iy++) {
            if (ruleRows[iy])
                continue
            for (var ix = table.x; ix < table.x + table.w; ix++)
                if (painted(at(ix, iy)) && ix > inkRight)
                    inkRight = ix
        }
        var gapX = -1
        for (var gx = table.x + 20; gx < ruleEnd - 20; gx++) {
            var clean = true
            for (var gy = table.y; gy < table.y + table.h; gy++) {
                if (ruleRows[gy])
                    continue
                if (same(at(gx, gy), fg)) {
                    clean = false
                    break
                }
            }
            if (clean) {
                gapX = gx
                break
            }
        }
        shell.log("table x=" + table.x + " rule " + ruleStart + ".." + ruleEnd + " ink to " + inkRight + " gap at " + gapX)
        if (ruleStart - table.x > 4)
            return shell.fail("the table left its left edge")
        if (ruleEnd - table.x > 400)
            return shell.fail("the table rules ran past their content")
        if (ruleEnd - inkRight > 40)
            return shell.fail("the table rules outran their ink")
        if (gapX < 0)
            return shell.fail("the table columns never separate")

        var quoteIdx = shell.blockIndex("quote")
        if (quoteIdx < 0)
            return shell.fail("no quote block arrived")
        var quote = shell.rectOf(quoteIdx)
        var bar = 0
        for (var by = quote.y; by < quote.y + quote.h; by++)
            for (var bx = quote.x; bx < quote.x + 2; bx++)
                if (same(at(bx, by), border))
                    bar++
        shell.log("bar px=" + bar + " of " + (quote.h * 2))
        if (bar < quote.h * 2 * 0.8)
            return shell.fail("the quote bar lost its muted ink")

        // A fenced block is fill with no border: its perimeter carries surface, never muted.
        var fenceIdx = shell.blockIndex("fence")
        if (fenceIdx < 0)
            return shell.fail("no fence block arrived")
        var fence = shell.rectOf(fenceIdx)
        var edgeMuted = 0
        var edgeFill = 0
        for (var ex = fence.x; ex < fence.x + fence.w; ex++) {
            if (same(at(ex, fence.y), border))
                edgeMuted++
            if (same(at(ex, fence.y), chrome))
                edgeFill++
            if (same(at(ex, fence.y + fence.h - 1), border))
                edgeMuted++
            if (same(at(ex, fence.y + fence.h - 1), chrome))
                edgeFill++
        }
        for (var ey = fence.y; ey < fence.y + fence.h; ey++) {
            if (same(at(fence.x, ey), border))
                edgeMuted++
            if (same(at(fence.x, ey), chrome))
                edgeFill++
            if (same(at(fence.x + fence.w - 1, ey), border))
                edgeMuted++
            if (same(at(fence.x + fence.w - 1, ey), chrome))
                edgeFill++
        }
        shell.log("fence edge muted=" + edgeMuted + " fill=" + edgeFill)
        if (edgeMuted > 0)
            return shell.fail("the fence kept its border")
        if (edgeFill < 100)
            return shell.fail("the fence lost its fill")

        // The remote box closes its four dashed edges, and its line starts left of centre.
        var remoteIdx = shell.blockIndex("remote")
        if (remoteIdx < 0)
            return shell.fail("no remote block arrived")
        var box = shell.rectOf(remoteIdx)
        // One dashed edge, first to last border pixel along it.
        function dashSpan(horizontal, fixed, from, to) {
            var first = -1
            var last = -1
            for (var i = from; i < to; i++) {
                var c = horizontal ? at(i, fixed) : at(fixed, i)
                if (same(c, border)) {
                    if (first < 0)
                        first = i
                    last = i
                }
            }
            return [first, last]
        }
        var top = dashSpan(true, box.y, box.x, box.x + box.w)
        var bottom = dashSpan(true, box.y + box.h - 1, box.x, box.x + box.w)
        var left = dashSpan(false, box.x, box.y, box.y + box.h)
        var right = dashSpan(false, box.x + box.w - 1, box.y, box.y + box.h)
        shell.log("remote top=" + top + " bottom=" + bottom + " left=" + left + " right=" + right)
        // The runs of one dashed edge over a window short of the opposite corner, first dash on, the window's cut last run dropped.
        var dashWindow = 60
        function dashRuns(horizontal, fixed, from, to) {
            var runs = []
            var on = null
            var len = 0
            for (var i = from; i < to; i++) {
                var hit = same(horizontal ? at(i, fixed) : at(fixed, i), border)
                if (on === null && !hit)
                    continue
                if (on === null || hit !== on) {
                    if (on !== null)
                        runs.push(len)
                    on = hit
                    len = 0
                }
                len++
            }
            return runs
        }
        var topRuns = dashRuns(true, box.y, box.x, box.x + Math.min(dashWindow, Math.floor(box.w / 2)))
        var leftRuns = dashRuns(false, box.x, box.y, box.y + Math.floor(box.h / 2))
        shell.log("remote dash runs top=" + topRuns.join(",") + " left=" + leftRuns.join(","))
        var dashFail = Checks.dashError(topRuns, "the top edge") || Checks.dashError(leftRuns, "the left edge")
        if (dashFail !== "")
            return shell.fail(dashFail)
        var deep = 0
        for (var tx = box.x + 4; tx < box.x + box.w - 4; tx++)
            if (same(at(tx, box.y + 1), border))
                deep++
        if (deep > 0)
            return shell.fail("the remote box's dashed edge is thicker than one pixel")
        if (top[0] - box.x > 8 || box.x + box.w - 1 - top[1] > 20
                || bottom[0] - box.x > 8 || box.x + box.w - 1 - bottom[1] > 20
                || left[0] - box.y > 8 || box.y + box.h - 1 - left[1] > 20
                || right[0] - box.y > 8 || box.y + box.h - 1 - right[1] > 20)
            return shell.fail("the remote box never closed its rectangle")
        var lineStart = box.x + box.w
        for (var ly = box.y + 4; ly < box.y + box.h - 4; ly++)
            for (var lx = box.x + 4; lx < box.x + box.w - 4; lx++) {
                var lc = at(lx, ly)
                if (same(lc, fg) || same(lc, border)) {
                    if (lx < lineStart)
                        lineStart = lx
                    break
                }
            }
        shell.log("remote line starts at " + lineStart + " of centre " + (box.x + box.w / 2))
        if (lineStart >= box.x + box.w / 2)
            return shell.fail("the remote line drifted past centre")

        for (var py = 0; py < h; py++)
            for (var px = 0; px < w; px++) {
                var c = at(px, py)
                if (c[2] >= 200 && c[0] <= 110 && c[1] <= 170)
                    return shell.fail("a Qt default link blue survived at " + px + "," + py)
            }
        shell.savedState = Flea.ViewState.state
        shell.suiteBody = Flea.Theme.font.body
        shell.larger = true
        shell.armed = false
        // Native label and r events finish their queued layout before the next specimen replaces the state.
        Qt.callLater(function () {
            Flea.ViewState.state = Object.assign({}, shell.savedState,
                { display: { textSize: { mode: shell.suiteBody < 16 ? 16 : 20 } } })
            largerSettle.start()
        })
    }
}
