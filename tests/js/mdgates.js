.import "../markdown-render.js" as Render
.import "../markdown-figures-render.js" as Figures
.import "../markdown-board.js" as Board
.import "sourcefixture.js" as Source

// Adversarial fixtures execute the render harness methods with broken live-tree states.
function run(check) {
    var preview = Source.source("ui/PreviewMarkdown.qml")
    check("md3z F2 one preview header", !/\/\/[^\n]*\n\/\/[^\n]*\nItem \{/.test(preview), true)
    check("md3z F2 image sentence beside resolution", /\/\/ Resolve images[^\n]*\n    function askParse/.test(preview), true)
    check("md3z F3 accentMarks sample", /\/\/ Sample input:[^\n]*pele-marker[^\n]*\nfunction accentMarks/.test(Source.source("ui/js/FigureWorker.mjs")), true)
    var commentFiles = ["ui/MarkdownFigure.qml", "ui/MarkdownText.qml", "ui/MarkdownPane.qml",
        "ui/PreviewMarkdown.qml", "ui/js/MdBlocks.js", "ui/js/MdItems.js", "ui/js/MdMath.js", "ui/js/MdEmph.js", "ui/js/MdBreak.js", "ui/js/MarkdownLists.js", "ui/js/MdChunks.js"]
    for (var f = 0; f < commentFiles.length; f++) {
        var sourceLines = Source.source(commentFiles[f]).split("\n")
        for (var c = 0; c < sourceLines.length; c++) {
            if (!/^\s*\/\//.test(sourceLines[c]))
                continue
            check("md3u F4 comment length " + commentFiles[f] + ":" + (c + 1),
                sourceLines[c].trim().length <= 140, true)
            if (c > 0 && /^\s*\/\//.test(sourceLines[c - 1]))
                check("md3u F4 single constraint " + commentFiles[f] + ":" + (c + 1),
                    /Sample(?: input)?:/.test(sourceLines[c - 1] + sourceLines[c]) && !(c > 1 && /^\s*\/\//.test(sourceLines[c - 2])), true)
        }
    }
    // A shell comment is one line, or one sample-input line beside one constraint line.
    var blockcost = Source.source("tests/markdown-blockcost.sh").split("\n")
    for (var h = 1; h < blockcost.length; h++) {
        if (/^\s*#(?!!)/.test(blockcost[h]) && /^\s*#(?!!)/.test(blockcost[h - 1]))
            check("md2 T3 blockcost.sh comment " + (h + 1) + " stands alone", /Sample(?: input)?:/.test(blockcost[h - 1] + blockcost[h]) && !(h > 1 && /^\s*#(?!!)/.test(blockcost[h - 2])), true)
    }
    var blocksSource = Source.source("ui/js/MdBlocks.js")
    check("md3u F5 figureKind sample", /\/\/ Sample input:[^\n]*mermaid[^\n]*\nfunction figureKind/.test(blocksSource), true)
    check("md3u F8 display sample", /\/\/ Sample input:[^\n]*\$\$x[^\n]*\\n[^\n]*\n        var math /.test(blocksSource), true)
    var figureRunner = Source.source("tests/markdown-figures-render.sh")
    check("md3t F5 one shell header", !/^#![^\n]*\n#[^\n]*\n#/.test(figureRunner), true)
    var figureModule = Source.source("tests/markdown-figures-render.js")
    check("md3t F6 labelError sample", /\/\/ Sample input:[^\n]*<text[^\n]*\nfunction labelError/.test(figureModule), true)
    check("md3t F6 paletteError sample", /\/\/ Sample input:[^\n]*fill=[^\n]*\nfunction paletteError/.test(figureModule), true)
    check("md3v F2 one module header", !/^\/\/[^\n]*\n\/\//.test(Source.source("tests/markdown-render.js")), true)
    var source = Source.source("tests/markdown-source-render.qml")
    var geometry = new Function("sourcePane", "reference", "Flea", "Text", "check", "sourceParts",
        Source.slice(source, "function readBack(markup)", "    Timer {") + "\ngeometry()")
    function sourceCase(widths, wheel) {
        var flick = { children: [], width: 520, height: 240, contentHeight: 268, count: 1,
            mapToItem: function () { return { x: 20, y: 16 } } }
        flick.parent = { parent: { children: flick.children } }
        for (var w = 0; w < widths.length; w++)
            flick.children.push({ knobItem: {}, flickable: flick, width: widths[w], mapToItem: function () { return { x: 560 } } })
        if (wheel)
            flick.children.push({ flickable: flick, width: 560, mapToItem: function () { return { x: 560 } } })
        var text = { width: 520, implicitHeight: 268, text: "raw", textFormat: 1,
            mapToItem: function () { return { x: 20, y: 16 } } }
        var row = { height: 268, children: [{ text: "1" }, text] }
        var failures = []
        geometry({ width: 560, height: 272, insetX: 20, insetY: 16, rawText: "raw" },
            { item: { blockItem: function () { return text } } },
            { Theme: { font: { body: 14 } } }, { PlainText: 0, StyledText: 1 },
            function (ok, label) { if (!ok) failures.push(label) },
            function () { return { flick: flick, row: row, text: text } })
        return failures
    }
    check("md3z F4 missing Source scrollbar rejected", sourceCase([]).length > 0, true)
    check("md3z F4 zero-width Source scrollbar rejected", sourceCase([0]).length > 0, true)
    check("md3z F4 duplicate Source scrollbars rejected", sourceCase([14, 14]).length > 0, true)
    check("md3z F4 one Source scrollbar accepted", sourceCase([14]).length, 0)
    check("md3z F4 wheel handler cannot replace missing bar", sourceCase([], true).length > 0, true)
    check("md3z F4 wheel handler not counted as scrollbar", sourceCase([14], true).length, 0)
    var renderRunner = Source.source("tests/markdown-render.sh")
    check("md3z F4 runner requires all 15 Source checks", renderRunner.indexOf("expected_source_checks=15") >= 0
        && renderRunner.indexOf("MARKDOWN_SOURCE $expected_source_checks checks, 0 failed") >= 0, true)

    var preview040 = Source.source("tests/preview-040.qml")
    var finish040 = new Function("poll", "console", "Qt", "checks", "failures",
        Source.slice(preview040, "function finish()", "function descendants(") + "\nfinish()")
    var exitEvents = []
    finish040({ stop: function () { exitEvents.push("stop") } }, { log: function () {} },
        { exit: function () { exitEvents.push("exit") } }, 4, 1)
    check("md3z r4 F12 repeating timer stops before exit", exitEvents.join(","), "stop,exit")

    var hunt = Source.source("tests/preview-hunt.qml")
    // Sample input: onTriggered: { ... } is compiled as function triggered() { ... }.
    var huntTick = new Function("root", "quick", "Date", "stage", "scenario", "liveFlick", "liveMarkdown",
        Source.slice(hunt, "onTriggered: {", "\n    }\n}").replace("onTriggered: {", "function triggered() {")
            + "\ntriggered()")
    function scrollCase(file, defect) {
        var scrollChecks = []
        var finished = false
        var injected = false
        var flick = { contentY: 0, contentHeight: 1000, height: 400, topMargin: 0, bottomMargin: 0 }
        var document = { active: true, blockList: [], blockItem: function () {
            return { mapToItem: function () { return { y: 0 } } }
        } }
        var root = { stage: 1, stamp: 0, overlayCase: true, fixture: "fixture", scrollFramePending: false,
            fileAScrollY: 120, fileBScrollY: 240,
            descendants: function () { return [document] },
            flickOf: function () { return flick },
            finish: function () { finished = true },
            check: function (label, actual, expected) {
                if (label.indexOf("file " + file + " scrolls") === 0)
                    scrollChecks.push(actual === expected)
            } }
        var quick = { status: "ready", open: function () { flick.contentY = 0 } }
        var expectedOffset = file === "A" ? root.fileAScrollY : root.fileBScrollY
        var maxTicks = 8
        for (var tick = 0; tick < maxTicks && !finished; tick++) {
            huntTick(root, quick, { now: function () { return (tick + 1) * 1000 } }, root.stage, "scroll", flick, document)
            if (defect === "frame" && scrollChecks.length > 0)
                return [false]
            if (!injected && flick.contentY === expectedOffset) {
                injected = true
                if (defect === "frame") {
                    huntTick(root, quick, { now: function () { return (tick + 1) * 1000 } }, root.stage, "scroll", flick, document)
                    return [scrollChecks.length === 0 && root.scrollFramePending]
                }
                if (defect === "offset")
                    flick.contentY = 0
                if (defect === "range")
                    flick.contentHeight = flick.height + expectedOffset - 1
            }
            root.scrollFramePending = false
        }
        return scrollChecks
    }
    for (var file of ["A", "B"]) {
        check("md3z r4 F16 " + file + " offset lost after frame rejected", scrollCase(file, "offset").indexOf(false) >= 0, true)
        check("md3z r4 F16 " + file + " offset outside scroll range rejected", scrollCase(file, "range").indexOf(false) >= 0, true)
        check("md3z r4 F16 " + file + " scroll check waits for frame", scrollCase(file, "frame").join(","), "true")
        check("md3z r4 F16 " + file + " scroll survives frame accepted", scrollCase(file, "").join(","), "true")
    }

    var figureSource = Source.source("tests/markdown-figures-render.qml")
    check("md3t F5 one QML header", !/\/\/[^\n]*\n\/\/[^\n]*\nShellRoot/.test(figureSource), true)
    var drive = new Function("shell", "md", "Flea", "Checks", "poll", "grabRoot",
        Source.slice(figureSource, "function drive()", "function grabbed(") + "\ndrive()")
    var checkHistory = new Function("shell", "md", "Flea", "Checks",
        Source.slice(figureSource, "function checkHistory()", "function drive()") + "\nreturn checkHistory()")
    function figureCase(mode, imgW, history, step) {
        var failure = ""
        var shell = { step: step, figMode: mode, farIndex: 9, requestHistory: history, t0: Date.now(),
            figuresSettled: function () { return true }, checkFar: function () { return true },
            log: function () {}, check: function () {}, fail: function (why) { failure = why } }
        var farSource = "flowchart TD\n    FAR --> AWAY"
        var md = { width: 560, blockList: [],
            figureInfo: function (i) { return i === 9 ? null : { boxW: 520, imgW: imgW, imgH: 20 } },
            blockItem: function () { return { visible: false } }, hexOf: function () { return "#101315" } }
        md.blockList[9] = { source: farSource }
        var checks = { surfaceError: Figures.surfaceError, farRequestError: Figures.farRequestError,
            figure: function () { return { svg: "" } },
            bodyFont: function () { return {} }, labelError: function () { return "" }, paletteError: function () { return "" } }
        shell.checkHistory = function () {
            return checkHistory(shell, md, { FigureService: { sends: history.length } }, checks)
        }
        drive(shell, md, { FigureService: { sends: history.length }, Theme: { color: {} } }, checks,
            { stop: function () {} }, { grabToImage: function () {} })
        return failure
    }
    var sentFar = [{ id: 1, source: "flowchart TD\n    FAR --> AWAY" }]
    for (var m = 0; m < 2; m++) {
        var mode = m === 0 ? "real" : "stub"
        check("md3t F2 evicted far request rejected in " + mode, figureCase(mode, 200, sentFar, 1) !== "", true)
        check("md3t F2 far request on return rejected in " + mode, figureCase(mode, 200, sentFar, 3) !== "", true)
        check("md3t F3 image wider than box rejected in " + mode, figureCase(mode, 700, [], 1) !== "", true)
        check("md3t F3 fitted image accepted in " + mode, figureCase(mode, 200, [], 1), "")
    }
    // Sample input: if (c[2] >= 200 && c[0] <= 110 && c[1] <= 170) precedes the default-blue failure.
    var blue = new Function("c", "accent", "ground", "Checks", "return "
        + figureSource.match(/if \(([^\n]*)\)\n\s+return shell\.fail\("a Qt default link blue/)[1])
    check("md3t F4 theme accent composite accepted", blue([101, 133, 202], [122, 162, 247], [16, 19, 21], Figures), false)
    check("md3t F4 Qt default blue rejected", blue([0, 0, 255], [122, 162, 247], [16, 19, 21], Figures), true)

    var captureWidth = 560
    var captureHeight = 1080
    var outside = { x: 20, y: 1080, w: 520, h: 17 }
    var error = ""
    try {
        Figures.inkBounds(new Uint8Array(captureWidth * captureHeight * 4), captureWidth, outside, [16, 19, 21], [30, 30, 30])
    } catch (e) {
        error = String(e)
    }
    check("md3t F7 out-of-buffer rect rejected", error.indexOf("1080") >= 0 && error.indexOf("560x1080") >= 0, true)
    var bodyPx = 14
    for (var r = 1; r <= 2; r++) {
        var box = Math.round(r * bodyPx)
        check("md3v F1 " + r + ".0 line box rejected", Render.lineBoxError([
            { name: "run", h: box, text: { box: box, lineHeight: box, font: { pixelSize: bodyPx } } }
        ]) !== "", true)
    }
    var goodBox = Math.round(Render.BOARD_LINE_BOX_RATIO * bodyPx)
    check("md3v F1 font-derived box accepted", Render.lineBoxError([
        { name: "run", h: goodBox, text: { box: goodBox, lineHeight: goodBox, font: { pixelSize: bodyPx } } }
    ]), "")
    var renderer = Source.source("tests/markdown-render.qml")
    check("md3z F6 board ratio is 1.7", Render.BOARD_LINE_BOX_RATIO, 1.7)
    check("md3z F6 ratio never read from the renderer", renderer.indexOf("Checks.lineBoxError(texts)") >= 0 && renderer.indexOf("boxRatio") < 0, true)
    check("md3v F3 rhythm read from the board's margin", renderer.indexOf("Checks.rhythmError(rects, Board.boardPx(Board.BOARD_BLOCK_GAP, Flea.Theme.font.body))") >= 0, true)
    // The board's 6, 8, 12, 20 and 15 are resolved at body 14 and follow the body from there.
    check("mdfid N6 the board's margin and fence padding at body 14",
        [Board.BOARD_BLOCK_GAP, Board.BOARD_FENCE_PAD_Y, Board.BOARD_FENCE_PAD_X].map(function (px) { return Board.boardPx(px, 14) }).join(","), "6,8,12")
    check("mdfid N4 the board's headings at body 14 and 12",
        [Render.BOARD_H1, Render.BOARD_H2].map(function (px) { return Board.boardPx(px, 14) + "/" + Board.boardPx(px, 12) }).join(" "), "20/17 15/13")
    // Sample input: shell.check(Checks.rhythmError(rects, md.blockGap), "block rhythm").
    var rhythmCall = renderer.match(/shell\.check\(Checks\.rhythmError\([^\n]+/)[0]
    var rhythm = new Function("rects", "md", "Flea", "Checks", "Board", "shell", rhythmCall)
    var rhythmError = ""
    rhythm([{ y: 0, h: 24 }, { y: 34, h: 24 }], { blockGap: 10 },
        { Theme: { font: { body: 14 } } }, Render, Board,
        { check: function (error) { rhythmError = error } })
    check("md3v F3 differing renderer gap rejected", rhythmError !== "", true)
}
