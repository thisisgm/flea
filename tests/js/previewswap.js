.import "../../ui/js/PreviewSwap.js" as PreviewSwap
.import "../../ui/js/Facts.js" as Facts
.import "../../ui/js/Swap.js" as Swap
.import "../../ui/js/Columns.js" as Columns
.import "../../ui/js/PreviewSettle.js" as PreviewSettle
.import "sourcefixture.js" as Source

// The preview swap, AGENTS.md "The preview swap": its moves, its cap, its frame kinds and what each surface waits for.

// Qt's Image.Status values, which the column hands over as plain numbers.
var NULL_STATUS = 0
var READY = 1
var LOADING = 2
var ERROR = 3

function column(state, extra) {
    var p = { state: state, thumb: false, frame: NULL_STATUS, noThumbComing: false, pdfDrawn: false,
              pdfFailed: false, linesLoading: false, meta: false }
    for (var k in extra)
        p[k] = extra[k]
    return PreviewSwap.columnReady(p)
}

function run(check) {
    check("a preview gets the listing swap's cap once its own work starts", PreviewSwap.capMs(false), Swap.CAP_MS)
    check("a PDF gets the same cap, its document settle running under it", PreviewSwap.capMs(true), Swap.CAP_MS)
    check("a capture gives up before the preview column's own 120 ms settle would load under it",
          PreviewSwap.CAPTURE_MS < 120, true)

    var idle = { capturing: false, holding: false, key: undefined }
    var held = { capturing: false, holding: true, key: "/d\n3" }
    check("a move with nothing held starts a hold", PreviewSwap.onMove(idle, "/d\n3", true), PreviewSwap.HOLD)
    check("the second caller for the same row joins it",
          PreviewSwap.onMove(held, "/d\n3", true), PreviewSwap.JOIN)
    check("a move to another row during a column hold is held j, which gives the stale picture up",
          PreviewSwap.onMove(held, "/d\n4", true), PreviewSwap.BURST)
    check("Quick Look keeps its picture through held j, as it kept the live preview before",
          PreviewSwap.onMove(held, "/d/b.jpg", false), PreviewSwap.JOIN)
    check("a capture still in flight counts as a hold",
          PreviewSwap.onMove({ capturing: true, holding: false, key: "/d\n3" }, "/d\n4", true), PreviewSwap.BURST)

    check("a frame under the picture is held whatever builds beneath it", PreviewSwap.frameKind(true, false, false), "held")
    check("a whole preview drawn live is no kind of frame at all", PreviewSwap.frameKind(false, true, false), "")
    check("a half-built preview drawn live is the defect", PreviewSwap.frameKind(false, false, false), "mid")
    check("after a fallback the same frame is the loading state, which is allowed",
          PreviewSwap.frameKind(false, false, true), "loading")

    check("the column's loading state is never ready", column(Facts.LOADING, {}), false)
    check("an image waits for its cache file to decode", column(Facts.IMAGE, { thumb: true, frame: LOADING }), false)
    check("and lands when it has", column(Facts.IMAGE, { thumb: true, frame: READY }), true)
    check("or when it failed, since the mark stands in then", column(Facts.IMAGE, { thumb: true, frame: ERROR }), true)
    check("an image with no cache file yet but one coming waits for it", column(Facts.IMAGE, {}), false)
    check("an image with none coming waits for the original the frame decodes itself",
          column(Facts.IMAGE, { noThumbComing: true, frame: LOADING }) + "|"
          + column(Facts.IMAGE, { noThumbComing: true, frame: READY }), "false|true")
    check("a video with no poster coming lands on its mark", column(Facts.VIDEO, { noThumbComing: true }), true)
    check("a video with a poster waits for it", column(Facts.VIDEO, { thumb: true, frame: LOADING }), false)
    check("a PDF waits for its first page", column(Facts.PDF, {}) + "|" + column(Facts.PDF, { pdfDrawn: true }), "false|true")
    check("an unreadable PDF lands on its error", column(Facts.PDF, { pdfFailed: true }), true)
    check("text and code wait for their lines",
          column(Facts.TEXT, { linesLoading: true }) + "|" + column(Facts.CODE, { linesLoading: false }), "false|true")
    check("an archive waits for its index", column(Facts.ARCHIVE, {}) + "|" + column(Facts.ARCHIVE, { meta: true }), "false|true")
    check("a kind drawn only as its mark lands with its facts",
          column(Facts.UNSUPPORTED, {}) + "|" + column(Facts.AUDIO, {}) + "|" + column(Facts.SYMLINK, {}) + "|"
          + column(Facts.MULTI, {}), "true|true|true|true")

    check("Quick Look waits while any pane says loading", PreviewSwap.lookReady("loading", false, false, false), false)
    check("interim shown is whole", PreviewSwap.lookReady("loading", false, false, false, true), true)
    check("interim over a PDF is whole too", PreviewSwap.lookReady("loading", true, false, false, true), true)
    check("a Markdown head drawn while the parse runs is whole", PreviewSwap.lookReady("loading", false, false, false, false, true), true)
    check("a Markdown pane with no head drawn still waits", PreviewSwap.lookReady("loading", false, false, false, false, false), false)
    check("without one a loading image still waits", PreviewSwap.lookReady("loading", false, false, false, false), false)
    check("an interim not yet visible at its rect still waits",
        PreviewSwap.lookReady("loading", false, false, false, undefined) + "|"
        + PreviewSwap.lookReady("loading", true, false, false, false), "false|false")
    check("a PDF viewer is ready once a page is on screen",
          PreviewSwap.lookReady("pdf", true, false, false) + "|" + PreviewSwap.lookReady("pdf", true, true, false), "false|true")
    check("or once the document is refused", PreviewSwap.lookReady("This file could not be read.", true, false, true), true)
    check("anything else not loading is whole", PreviewSwap.lookReady("image", false, false, false), true)
    runInterimRect(check)
    runInterimShown(check)
    runFirstScreen(check)
    runFolderDataHold(check)
    runPictureHoldLeak(check)
    runPreviewSettle(check)
    runE81(check)
    runPdfIdle(check)
}

// The body of one QML function by brace count, so each check reads the arm it names and not a copy elsewhere.
// Sample input: bodyOf("f() { return 1 }", "f") is "{ return 1 }".
function bodyOf(src, name) {
    var text = String(src)
    var at = text.indexOf(name + "(")
    if (at < 0)
        return ""
    var open = text.indexOf("{", at)
    if (open < 0)
        return ""
    var depth = 0
    for (var i = open; i < text.length; i++) {
        var c = text.charAt(i)
        if (c === "{")
            depth += 1
        else if (c === "}") {
            depth -= 1
            if (depth === 0)
                return text.substring(open, i + 1)
        }
    }
    return ""
}

// Sample input: squashed("a  b\n c") is "a b c".
function squashed(s) {
    return String(s).replace(/\s+/g, " ")
}

// Sample input: stripped("a // root.interimShown\nb") is "a \nb".
function stripped(s) {
    return String(s).replace(/\/\/[^\n]*/g, "")
}

function runShowCursorRow(check, area) {
    var show = squashed(bodyOf(area, "showCursorRow"))
    check("source: showCursorRow guards a stale show with the strict force-and-data-hold early return",
        show.indexOf("if (force !== true && Columns.folderDataHold(root.cursorIsDir, root.answered(root.childPath))) return") >= 0, true)
    var retAt = show.indexOf("return")
    check("source: that early return stands before the shown assignments",
        retAt >= 0 && show.indexOf("shownHasRow") > retAt && show.indexOf("shownIsDir") > retAt && show.indexOf("shownChildPath") > retAt, true)
}

// e81f: the interim rect is the original's upright aspect-fit, never enlarged, never the cache file's size.
function rect(surfaceW, surfaceH, imageW, imageH, orient) {
    var r = PreviewSwap.interimRect(surfaceW, surfaceH, imageW, imageH, orient)
    return r === null ? "none" : r.x + "," + r.y + "," + r.w + "x" + r.h
}

function runInterimRect(check) {
    check("orient 1 sizes from the original, not its cache file",
        rect(754, 471, 6000, 4000, 1), "24,0,706.5x471")
    check("orient 3 keeps the stored sides", rect(754, 471, 6000, 4000, 3), "24,0,706.5x471")
    check("orient 6 swaps to the upright sides", rect(2080, 1137, 4032, 3024, 6), "614,0,852.75x1137")
    check("orient 8 swaps the other way", rect(2080, 1137, 3024, 4032, 8), "282,0,1516x1137")
    check("a swap turns a 60x40 box portrait", rect(100, 100, 60, 40, 6), "30,20,40x60")
    check("while orient 1 keeps it landscape", rect(100, 100, 60, 40, 1), "20,30,60x40")
    check("a small original is never enlarged", rect(754, 471, 120, 68, 1), "317,202,120x68")
    check("unknown pixels are no interim",
        rect(754, 471, 0, 0, 1) + "|" + rect(754, 471, 640, 0, 1) + "|" + rect(754, 471, 0, 480, 6),
        "none|none|none")
}

// Quick Look releases early only on the shown interim: each pin reads its binding's Source.slice with comments stripped.
function runInterimShown(check) {
    var readyScope = stripped(Source.slice(Source.source("ui/Preview.qml"), "readonly property bool lookReady:", "property string interimThumb"))
    var shownScope = stripped(Source.slice(Source.source("ui/Preview.qml"), "readonly property bool interimShown:", "// The original's pixels"))
    var imageScope = stripped(Source.slice(Source.source("ui/PreviewImage.qml"), "readonly property bool interimReady:", "// The same name the media"))
    check("lookReady answers on the shown interim",
        readyScope.indexOf("root.interimShown") >= 0, true)
    check("interimShown releases only on the interim image Ready",
        shownScope.indexOf("imageLoader.item.interimReady === true") >= 0, true)
    check("that term is PreviewImage interimReady, never a local flag",
        squashed(imageScope).trim() === "readonly property bool interimReady: interimPicture.status === Image.Ready", true)
}

// Sample input: firstScreenOf("file.loaded && root.blockList.length > 0", true, "", ["a"], true) is true.
function firstScreenOf(expr, loaded, parseError, blocks, parsing) {
    var file = { loaded: loaded }
    var root = { parseError: parseError, blockList: blocks, parsing: parsing }
    return new Function("root", "file", "return " + expr)(root, file)
}

// A reparse keeps its drawn screen: firstScreen reads the real binding, so a parsing exclusion fails it.
function runFirstScreen(check) {
    var pane = Source.source("ui/PreviewMarkdown.qml")
    var found = pane.match(/readonly property bool firstScreen:([^\n]*)/)
    check("source: firstScreen counts drawn blocks", found !== null && found[1].indexOf("blockList.length") >= 0, true)
    var expr = found !== null ? stripped(found[1]) : "false"
    check("source: firstScreen names no reparse exclusion", expr.indexOf("parsing") < 0, true)
    check("a reparse of an open document keeps its drawn screen", firstScreenOf(expr, true, "", ["a"], true), true)
    check("with no blocks there is no first screen", firstScreenOf(expr, true, "", [], true), false)
    check("control: a parsing exclusion loses the reparse screen", firstScreenOf(expr + " && !root.parsing", true, "", ["a"], true), false)
}

// A folder peek in Columns holds by data: an unanswered folder keeps the old column, and the landed peek shows it with its rows in one pass.
function runFolderDataHold(check) {
    check("an unanswered folder defers by data", Columns.folderDataHold(true, false), true)
    check("an answered folder lands at once", Columns.folderDataHold(true, true), false)
    check("a file never defers by data", Columns.folderDataHold(false, false)
        + "|" + Columns.folderDataHold(false, true), "false|false")
    check("a landed peek shows the folder it waited for", Columns.showFolderOnPeek("/a", "/b", true), true)
    check("an already shown folder shows nothing new", Columns.showFolderOnPeek("/a", "/a", true), false)
    check("a still outstanding peek leaves the old column up", Columns.showFolderOnPeek("/a", "/b", false), false)
    check("no cursor folder shows nothing", Columns.showFolderOnPeek("", "/b", true), false)
    check("a data-held empty folder lands settled", Columns.shouldSettleHero(true, "/a", "/a", false, 0), true)
    check("a non-empty folder never settles a hero", Columns.shouldSettleHero(true, "/a", "/a", false, 3), false)
    check("a locked folder never settles a hero", Columns.shouldSettleHero(true, "/a", "/a", true, 0), false)
    check("a file never settles a hero", Columns.shouldSettleHero(false, "/a", "/a", false, 0), false)
    check("a stale peek never settles a hero", Columns.shouldSettleHero(true, "/a", "/b", false, 0), false)
    var area = Source.source("ui/ColumnsArea.qml")
    var move = squashed(bodyOf(area, "moveThird"))
    check("a move onto an unanswered folder takes the data hold", move.indexOf("Columns.folderDataHold(") >= 0, true)
    var arm = move.substring(move.indexOf("Columns.folderDataHold("), move.indexOf("folderFallback.restart()"))
    check("a live hold is handed to the data hold's wait, never cancelled into it",
        arm.indexOf("thirdSwap.cancel()") < 0, true)
    check("the waiting folder is never shown before its peek lands", arm.indexOf("showCursorRow") < 0, true)
    check("the data hold takes no picture",
        arm.indexOf("thirdSwap.hold(") < 0 && arm.indexOf("thirdSwap.start(") < 0, true)
    check("while every other move stops it", move.indexOf("folderFallback.stop()") >= 0, true)
    var peeked = squashed(bodyOf(area, "onPeeked"))
    check("a landed peek shows the folder it waited for", peeked.indexOf("Columns.showFolderOnPeek(") >= 0
        && peeked.indexOf("root.showCursorRow()") > peeked.indexOf("Columns.showFolderOnPeek("), true)
    check("the fallback is a single shot at the listing swap cap", area.indexOf("interval: Swap.HOLD_MS") >= 0, true)
    check("and shows the pending column when the peek is late", area.indexOf("onTriggered: if (root.cursorIsDir && !root.answered(root.childPath)) { thirdSwap.cancel(); root.showCursorRow(true) }") >= 0, true)
    runShowCursorRow(check, area)
    check("a data-held empty hero skips its entrance", Source.source("ui/EmptyState.qml").indexOf("animateEntrance") >= 0, true)
    check("its mark can settle at once", Source.source("ui/FleaMark.qml").indexOf("function settle()") >= 0, true)
    check("a landed empty peek settles its hero whole", peeked.indexOf("Columns.shouldSettleHero(") >= 0
        && peeked.indexOf("markItem.settle()") > peeked.indexOf("Columns.shouldSettleHero("), true)
}

// Move-clock settle: idle moves load at once, repeats change nothing, bursts trail once.
function runPreviewSettle(check) {
    check("a lone selection never keys on the version", PreviewSettle.selectionKey(0, 9), "")
    check("a single row never keys on the version", PreviewSettle.selectionKey(1, 7), "")
    check("a multi-selection keys on its count and version", PreviewSettle.selectionKey(3, 4), "3:4")
    check("a multi-selection moves with its version", PreviewSettle.selectionKey(3, 5), "3:5")
    check("an idle first move is due at once", PreviewSettle.due(1000, 0, 120), true)
    check("a step exactly one interval on is due", PreviewSettle.due(120, 0, 120), true)
    check("a step inside the interval waits", PreviewSettle.due(119, 0, 120), false)
    check("a backwards clock reads idle, never wedged", PreviewSettle.due(100, 200, 120), true)
    check("a pending repeat stays put", PreviewSettle.plan(50, 0, 120, "b", "b", true), "same")
    check("a settled repeat schedules by the clock", PreviewSettle.plan(1000, 0, 120, "b", "b", false), "now")
    check("a new row after quiet loads at once", PreviewSettle.plan(1000, 0, 120, "c", "b", false), "now")
    check("a new row inside the window trails", PreviewSettle.plan(50, 0, 120, "c", "b", false), "later")
    var lastAt = -1000, lastKey = "a", armed = false, seen = []
    var moves = [[0, "b"], [30, "c"], [60, "d"], [90, "e"], [120, "f"]]
    for (var i = 0; i < moves.length; i++) {
        var decision = PreviewSettle.plan(moves[i][0], lastAt, 120, moves[i][1], lastKey, armed)
        seen.push(decision)
        if (decision !== "same") { lastKey = moves[i][1]; lastAt = moves[i][0]; armed = decision === "later" }
    }
    check("a 30ms burst loads first once, then trails only", seen.join(","), "now,later,later,later,later")
    check("a same-key refresh lands on the pending timer",
        PreviewSettle.plan(100, 90, 120, "e", "e", true), "same")
    var preview = Source.source("ui/SelectionPreview.qml")
    check("the column preview imports the settle helper",
        preview.indexOf('import "js/PreviewSettle.js" as PreviewSettle') >= 0, true)
    check("it stamps moves beside the settle key", preview.indexOf("property string lastMoveKey") >= 0
        && preview.indexOf("property double lastMoveAt") >= 0, true)
    var replaceArm = squashed(bodyOf(preview, "replace"))
    var dupAt = replaceArm.indexOf("if (key === root.lastMoveKey && (root.isShown() || root.loadQueuedKey === key)) return")
    var holdAt = replaceArm.indexOf("root.swap.hold(root.clearForMove, key)")
    var settleAt = replaceArm.indexOf("root.settleFor(key)")
    check("a true duplicate returns before any clear or picture", dupAt >= 0 && holdAt > dupAt, true)
    check("a move takes the swap picture before scheduling", holdAt >= 0 && settleAt > holdAt, true)
    var canReadAt = replaceArm.indexOf('if (!root.canRead) { root.clear(); return }')
    check("the no-swap branch keeps its reset before scheduling", canReadAt >= 0
        && replaceArm.indexOf("root.clear() root.settleFor(key)", canReadAt) > settleAt, true)
    var schedArm = squashed(bodyOf(preview, "function settleFor"))
    check("a pending timer covers its own refresh", schedArm.indexOf("settle.running && root.settleKey === key") >= 0, true)
    var sameAt = schedArm.indexOf('if (decision === "same") return')
    check("settleFor stamps only scheduled moves", sameAt >= 0 && schedArm.indexOf("root.lastMoveKey = key") > sameAt && schedArm.indexOf("root.lastMoveAt = now") > sameAt, true)
    var fireArm = squashed(bodyOf(preview, "function fireSettle"))
    check("the timer and the fast path share one decision",
        fireArm.indexOf("ExtThumbs.manualHold(") >= 0 && preview.indexOf("onTriggered: root.fireSettle()") >= 0, true)
    check("an unknown class still waits instead of loading",
        fireArm.indexOf("!root.pane.storageKnown") >= 0 && fireArm.indexOf("root.followSelection()") >= 0, true)
    var loadArm = squashed(preview.substring(preview.indexOf("function load()"), preview.indexOf("function askThumb()")))
    check("a load stamps no move clock", loadArm.indexOf("lastMoveAt") < 0 && loadArm.indexOf("lastMoveKey") < 0, true)
    check("the column names the queued load's key",
        preview.indexOf("property string loadQueuedKey") >= 0, true)
    var selQueued = squashed(preview.substring(preview.indexOf("function loadSelection()"), preview.indexOf("function load()")))
    check("a queued load names its key before the hold",
        selQueued.indexOf("root.loadQueuedKey = key") >= 0
        && selQueued.indexOf("root.loadQueuedKey = key") < selQueued.indexOf("root.swap.hold(root.load, key, true)"), true)
    check("a clear forgets the queued key",
        squashed(bodyOf(preview, "function clearShown")).indexOf('root.loadQueuedKey = ""') >= 0, true)
    check("a duplicate replace stands down while its load is queued",
        replaceArm.indexOf("root.loadQueuedKey === key") >= 0, true)
    var quick = Source.source("ui/Preview.qml")
    var followArm = squashed(bodyOf(quick, "function follow"))
    check("a pending repeat returns first",
        followArm.indexOf("PreviewSettle.plan(now, root.lastMoveAt, root.followSettleMs, key, root.lastMoveKey, followSettle.running, root.unrested(key))") >= 0
        && followArm.indexOf('if (decision === "same") return') >= 0, true)
    check("a settled revisit of the shown target stays put",
        followArm.indexOf("root.isShown(newPath, newIcon, newSize, newKind)") >= 0, true)
    var qHoldAt = followArm.indexOf("held.hold(null, newPath)")
    var qIdleAt = followArm.indexOf('if (decision === "now") {')
    check("Quick Look takes its picture before deciding", qHoldAt >= 0 && qIdleAt > qHoldAt, true)
    var qShownAt = followArm.indexOf("if (root.revealedOpen) {")
    check("a move during the hold takes no picture", qShownAt >= 0 && qShownAt < qHoldAt, true)
    check("an explicit open stamps the move", squashed(bodyOf(quick, "function open")).indexOf("root.lastMoveAt = Date.now()") >= 0, true)
    check("a close clears the move key", squashed(bodyOf(quick, "function close")).indexOf('root.lastMoveKey = ""') >= 0, true)
    var thumbs = Source.source("ui/ColumnPane.qml")
    check("thumbnail sweeps keep their own debounce, never the cursor helper",
        thumbs.indexOf("PreviewSettle") < 0, true)
}

// r35-pdfidle: an isolated press opens the document at once, a walk and a live render wait.
function pdfOpenNow(now, lastRequestAt, intervalMs, renderLoading) {
    if (typeof PreviewSettle.pdfOpenNow !== "function")
        return "missing"
    return PreviewSettle.pdfOpenNow(now, lastRequestAt, intervalMs, renderLoading)
}

function runPdfIdle(check) {
    check("an isolated press opens at once", pdfOpenNow(1000, 0, 120, false), true)
    check("a press one settle after the last opens at once", pdfOpenNow(120, 0, 120), true)
    check("a press inside the settle waits for it", pdfOpenNow(50, 0, 120, false), false)
    check("a live render blocks the switch even when idle", pdfOpenNow(1000, 0, 120, true), false)
    check("a live render blocks the switch inside the settle too", pdfOpenNow(50, 0, 120, true), false)
    check("a backwards clock reads idle, never wedged", pdfOpenNow(100, 200, 120, false), true)
    var pdf = Source.source("ui/PreviewPdf.qml")
    check("the document reaches PreviewSettle's idle rule",
        pdf.indexOf('import "js/PreviewSettle.js" as PreviewSettle') >= 0, true)
    check("a path or active change stamps the last request",
        pdf.indexOf("root.lastRequestAt = now") >= 0, true)
    var openAt = pdf.indexOf("function openPending()")
    check("one opener serves the timer and the fast path",
        openAt >= 0 && pdf.indexOf("function requestOpen()") >= 0
        && pdf.indexOf("function openPending()", openAt + 1) < 0, true)
    var ask = squashed(bodyOf(pdf, "function requestOpen"))
    var busy = squashed(bodyOf(pdf, "function renderBusy"))
    check("the fast path decides on idleness and a live render, of the one page or of any page of the stack",
        ask.indexOf("PreviewSettle.pdfOpenNow(") >= 0 && ask.indexOf("root.renderBusy()") >= 0
        && busy.indexOf("root.pageItem.status === Image.Loading") >= 0
        && busy.indexOf("pages.itemAt(i)") >= 0 && busy.indexOf(".rendering) return true") >= 0, true)
    check("the first frame never waits on a page whose render failed",
        squashed(bodyOf(pdf, "function firstDrawn")).indexOf("!slot.settled") >= 0
        && squashed(pdf).indexOf("readonly property bool settled: slot.landed || slot.errored") >= 0, true)
    check("the fast path waits for the next presented frame, never the end of the turn",
        ask.indexOf("Qt.callLater") < 0 && ask.indexOf("root.fastArmed = PreviewSettle.pdfOpenNow(") >= 0, true)
    check("a newer request cancels the deferred open",
        ask.indexOf("var now = Date.now() root.fastArmed = PreviewSettle.pdfOpenNow(now, root.lastRequestAt, root.settleMs, root.renderBusy()) root.lastRequestAt = now") >= 0, true)
    check("the settle stays as the backstop on the idle path",
        ask.indexOf("pdfSettle.restart()") >= 0, true)
    var fast = squashed(bodyOf(pdf, "function runFast"))
    check("the deferred open runs once and opens through the shared opener",
        fast.indexOf("root.fastArmed") >= 0 && fast.indexOf("root.openPending()") >= 0, true)
    check("the deferred open stops the backstop it beat",
        fast.indexOf("pdfSettle.stop()") >= 0, true)
    var frame = squashed(Source.slice(pdf, "property bool fastArmed", "function requestOpen"))
    check("one frame connection serves the deferred open and outlives nothing",
        frame.indexOf("onFrameSwapped") >= 0 && frame.indexOf("root.Window.window") >= 0
        && frame.indexOf("root.fastArmed ?") >= 0, true)
    var settle = squashed(Source.slice(pdf, "id: pdfSettle", "id: renderCap"))
    check("the settle opens through the shared opener whatever the render state",
        settle.indexOf("root.openPending()") >= 0, true)
    check("the settle disarms the deferred open it beat",
        settle.indexOf("root.fastArmed = false") >= 0, true)
    check("the settle carries no render gate of its own",
        settle.indexOf("Image.Loading") < 0, true)
}

// A launch or folder move must never leave the third-column picture up: only a real file row takes one.
function fileRowSafe(row) {
    return typeof Columns.isFileRow === "function" ? Columns.isFileRow(row) : "missing"
}

function runPictureHoldLeak(check) {
    check("the file-row rule exists", typeof Columns.isFileRow === "function", true)
    check("a null row never takes a file load", fileRowSafe(null), false)
    check("an undefined row never takes a file load", fileRowSafe(undefined), false)
    check("a folder never takes a file load", fileRowSafe({d: true}), false)
    check("a file takes a file load", fileRowSafe({d: false}), true)
    var area = Source.source("ui/ColumnsArea.qml")
    var move = squashed(bodyOf(area, "moveThird"))
    check("a move takes a file hold only for a real file row",
        move.indexOf("Columns.isFileRow(root.cursorRow)") >= 0, true)
    var peeked = squashed(bodyOf(area, "onPeeked"))
    var landAt = peeked.indexOf("showFolderOnPeek(")
    var shownAt = peeked.indexOf("root.showCursorRow()", landAt)
    var cancelAt = peeked.indexOf("thirdSwap.cancel()", landAt)
    check("a landed peek ends a live picture before showing",
        landAt >= 0 && shownAt > landAt && cancelAt >= 0 && cancelAt < shownAt, true)
    var fallbackAt = area.indexOf("id: folderFallback")
    var triggerAt = area.indexOf("onTriggered:", fallbackAt)
    var trigger = area.substring(triggerAt, area.indexOf("\n", triggerAt))
    check("the fallback ends a live picture before showing",
        trigger.indexOf("thirdSwap.cancel()") >= 0 && trigger.indexOf("root.showCursorRow(true)") >= 0, true)
    var preview = Source.source("ui/SelectionPreview.qml")
    check("the preview imports the file-row rule",
        preview.indexOf('import "js/Columns.js" as Columns') >= 0, true)
    check("the preview never holds a picture for a folder cursor",
        preview.indexOf("Columns.isFileRow") >= 0, true)
    var replaceArm = squashed(bodyOf(preview, "replace"))
    var replaceGuard = replaceArm.indexOf("Columns.isFileRow(")
    var replaceReturn = replaceArm.indexOf("return", replaceGuard)
    var replaceSlice = replaceGuard >= 0 ? replaceArm.substring(replaceGuard, replaceReturn) : ""
    check("a folder move keeps the old preview instead of clearing it",
        replaceGuard >= 0 && replaceSlice.indexOf("settle.stop()") >= 0 && replaceSlice.indexOf("root.clear()") < 0, true)
    var followArm = squashed(bodyOf(preview, "followSelection"))
    var followGuard = followArm.indexOf("Columns.isFileRow(candidate)")
    var followReturn = followArm.indexOf("return", followGuard)
    var followSlice = followGuard >= 0 ? followArm.substring(followGuard, followReturn) : ""
    check("a folder selection keeps the old preview instead of clearing it",
        followGuard >= 0 && followSlice.indexOf("settle.stop()") >= 0 && followSlice.indexOf("root.clear()") < 0, true)
    var selAt = preview.indexOf("function loadSelection()")
    var loadAt = preview.indexOf("function load()", selAt)
    var selBody = squashed(preview.substring(selAt, loadAt))
    var selGuard = selBody.indexOf("Columns.isFileRow(")
    check("a folder cursor takes no load hold and keeps the old preview",
        selGuard >= 0 && selBody.indexOf("settle.stop()") > selGuard
        && selBody.indexOf("settle.stop()") < selBody.indexOf("root.swap.hold("), true)
    var loadBody = squashed(preview.substring(loadAt, preview.indexOf("function startSwap", loadAt)))
    var loadGuard = loadBody.indexOf("Columns.isFileRow(current)")
    check("a folder load keeps the old preview instead of clearing it",
        loadGuard >= 0 && loadBody.indexOf("root.clear()") > loadGuard, true)
}

// e81f: Quick Look shows the held cache file at once, then the full decode replaces it.
function runE81(check) {
    var quick = Source.source("ui/Preview.qml")
    var sel = Source.source("ui/SelectionPreview.qml")
    var image = Source.source("ui/PreviewImage.qml")
    var openArm = squashed(bodyOf(quick, "function open"))
    check("Space threads the held thumb in", openArm.indexOf("root.load(newPath, newIcon, newSize, newKind, newThumb") >= 0, true)
    check("open captures the row for the meta ask",
        openArm.indexOf("root.pane ? root.pane.cursorIndex : -1") >= 0, true)
    var followArm = squashed(bodyOf(quick, "function follow"))
    check("follow carries the captured row to the settle",
        followArm.indexOf("root.pendingImageRow = root.pane ? root.pane.cursorIndex : -1") >= 0
        && quick.indexOf("root.pendingThumb, root.pendingImageRow") >= 0, true)
    var flat = squashed(quick)
    check("the interim is stamped and replaced, never final",
        quick.indexOf("root.interimStamp = newPath") >= 0 && flat.indexOf("root.path === root.interimStamp && root.interimBox !== null") >= 0
        && image.indexOf("visible: root.interimVisible") >= 0, true)
    var groundAt = image.indexOf("color: Theme.color.background")
    var interimAt = image.indexOf("id: interimPicture")
    var pictureAt = image.indexOf("id: picture")
    check("the interim draws above the ground and below the final picture",
        groundAt >= 0 && interimAt > groundAt && pictureAt > interimAt, true)
    check("the interim takes no turn: a cache file is already upright",
        quick.indexOf("interimTurned") < 0 && image.indexOf("interimTurned") < 0, true)
    var pre = squashed(bodyOf(quick, "function maybePrefetch"))
    check("prefetch warms one cache entry on rest only",
        pre.indexOf("ViewState.previewAutomatic") >= 0 && pre.indexOf("followSettle.running") >= 0 && pre.indexOf("root.thumbClear(row)") >= 0
        && pre.indexOf("cacheOnly: true") >= 0 && pre.indexOf("backend.thumb(work.ask, true)") >= 0 && pre.indexOf("askMeta") < 0, true)
    var clear = squashed(bodyOf(quick, "function thumbClear"))
    check("the prefetch and the owed thumbnail share one gate: listing settled, class known, no manual hold, row allowed",
        clear.indexOf("root.pane.listInFlight") >= 0 && clear.indexOf("!root.pane.storageKnown") >= 0 && clear.indexOf("ExtThumbs.manualHold(") >= 0
        && clear.indexOf("Thumbs.allowed(row, ViewState.thumbnailMode)") >= 0
        && squashed(bodyOf(quick, "function thumbOwed")).indexOf("root.thumbClear(") >= 0, true)
    check("the interim takes the upright original's rect, never the cache pixels",
        flat.indexOf("PreviewSwap.interimRect(panes.width, panes.height - (root.isRawFile ? Theme.chromeHeight : 0),") >= 0
        && flat.indexOf("root.interimBox !== null ? root.interimBox.x : 0") >= 0
        && quick.indexOf("thumbLimit(interimPicture") < 0, true)
    var askArm = squashed(bodyOf(quick, "function askImage"))
    check("the meta ask runs once per show and never mid-burst",
        askArm.indexOf("root.imageAsked") >= 0 && askArm.indexOf("followSettle.running") >= 0
        && askArm.indexOf("root.pane.backend.askMeta(root.imageRow, false, false, false)") >= 0, true)
    check("a reply for another row is dropped",
        flat.indexOf("if (root.isImage && !root.isRawFile && row === root.imageRow && root.imageRowShown())") >= 0, true)
    check("the ask re-resolves a drifted index to the shown file",
        askArm.indexOf("root.resolveImageRow()") >= 0
        && squashed(bodyOf(quick, "function resolveImageRow")).indexOf("root.namedRow(at, root.path) !== null ? at : -1") >= 0, true)
    var wire = Source.source("ui/PaneWire.qml")
    var ext = Source.source("ui/js/ExtThumbs.js")
    check("a prefetch miss on a generating class returns to unasked",
        wire.indexOf("ExtThumbs.thumbReply(pane.thumbState, row, file, pane.thumbCap") >= 0
        && ext.indexOf("Thumbs.CACHE_ASKED") >= 0 && ext.indexOf("Thumbs.miss(state, row, true, cap)") >= 0, true)
    check("the forgotten row is re-planned through the settled view",
        wire.indexOf("if (next.missed && pane.listArea) pane.listArea.restartSettle()") >= 0, true)
    var loadArm = squashed(sel.substring(sel.indexOf("function load()"), sel.indexOf("function askThumb")))
    check("the column single-row ask runs through its held gate",
        loadArm.indexOf("root.askThumb()") >= 0 && loadArm.indexOf("pane.backend.thumb(work.ask") < 0, true)
    var askThumbArm = squashed(bodyOf(sel, "function askThumb"))
    check("that ask waits for the storage class and carries it",
        askThumbArm.indexOf("PreviewAsk.workFor(") >= 0
        && askThumbArm.indexOf("storageKnown: pane.storageKnown") >= 0
        && askThumbArm.indexOf("storageClass: pane.storageClass") >= 0, true)
    check("the class landing runs the held ask",
        sel.indexOf("function onStorageKnownChanged() { root.followSelection(); root.askThumb() }") >= 0, true)
}
