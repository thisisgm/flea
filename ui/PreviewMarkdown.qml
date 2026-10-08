import QtQuick
import Quickshell.Io
import "." as Flea
import "js/Markdown.js" as Markdown
import "js/MarkdownPrepared.js" as Prepared

// Rendered and Source previews share document insets, and only images beside the document can load.
Item {
    id: root

    property bool active: false
    property string path: ""
    property int size: 0
    // "rendered" or "source"; anything else reads as rendered. Only the Quick Look's r asks for source.
    property string view: Markdown.RENDERED

    // FileView reads whole files, so refuse above maxBytes, with remote storage keeping its smaller 256 KiB gate.
    property int maxBytes: 1048576
    // Retained for its callers; over-limit rows are refused, never truncated, so it reads nothing.
    property bool truncate: false
    readonly property bool tooLarge: root.size > root.maxBytes
    property bool readFailed: false
    // Quick Look only: the one file whose first read may block the key (a page-cache read beats a lost frame), named by the open.
    property string blockPath: ""
    // Reads that blocked the key for the file blockPath named, so a suite tells them from an async read.
    property int blockedReads: 0
    // Quick Look only: a small document's blocks are taken from, and kept in, the one parsed entry a resting cursor prepares.
    property bool shareParse: false
    // Parses a request took from the shared entry instead of running, so a suite can tell a reuse from a parse.
    property int reusedParses: 0

    // Set by each load, never a binding on file.text(): the blocking first read loaded inside such a binding and re-entered it.
    property string loadedText: ""
    readonly property string rawText: root.loadedText
    function hexOf(c) {
        return Prepared.hexOf(c)
    }
    // The host supplies the code surface colour for its page.
    property color codeSurface: Theme.color.surface
    readonly property string borderHex: hexOf(Theme.color.muted)
    readonly property string chromeHex: hexOf(root.codeSurface)
    // The render suite reads the ink it asserts beside the border, same assembly, no coercion.
    readonly property string inkHex: hexOf(Theme.color.foreground)
    readonly property string accentHex: hexOf(Theme.color.accent)
    readonly property string mutedHex: hexOf(Theme.color.muted)
    readonly property string surfaceHex: hexOf(Theme.color.surface)
    readonly property int insetX: Theme.spacing.rowPaddingX + Theme.spacing.rowPaddingY - Theme.spacing.hairline
    readonly property int insetY: Theme.spacing.gap + Theme.spacing.rowPaddingY
    // RenderedPreviews resolves its pixels at body 14, and each one follows the body from there: no spacing token equals them at any size.
    readonly property int boardBody: 14
    readonly property int boardBlockGap: 6
    readonly property int boardFencePadX: 12
    readonly property int boardFencePadY: 8
    // Headings are 20 and 15 px at body 14 in both surfaces, over the body the surface sets; deeper levels are body bold.
    readonly property var boardHeadings: [20, 15]
    function boardPx(px) {
        return Math.round(px * Theme.font.body / root.boardBody)
    }
    readonly property int blockGap: root.boardPx(root.boardBlockGap)
    readonly property int fencePadX: root.boardPx(root.boardFencePadX)
    readonly property int fencePadY: root.boardPx(root.boardFencePadY)
    // The preview column sets the document one token under Quick Look's body (RenderedPreviews 13 against 14).
    property bool compact: false
    readonly property int bodyPx: root.compact ? Theme.font.bodySmall : Theme.font.body
    // The faint rule wash the Quick Look bar's bottom hairline draws, shared by the table's header and row rules.
    readonly property real ruleOpacity: 0.12
    function headingPx(level) {
        return level >= 1 && level <= root.boardHeadings.length ? root.boardPx(root.boardHeadings[level - 1]) : root.bodyPx
    }
    // A document past the nesting limit parses to one sentinel block, and the pane then shows its source behind a notice.
    readonly property bool tooDeep: root.blockList.length === 1 && root.blockList[0].type === "deep"
    readonly property string shownView: root.tooDeep ? Markdown.SOURCE : root.view
    readonly property Item noticeItem: sourceList.noticeItem
    readonly property Item sourceItem: sourceList
    // Only the active file in Rendered view may request figures.
    readonly property bool figuresArmed: root.active && root.shownView !== Markdown.SOURCE
    // A parsed document that may ask for figures starts the helper at once, so it is warm when the first one is asked for.
    readonly property var warmBlocks: root.figuresArmed && root.blocksReady && file.loaded && !root.tooLarge ? Markdown.figuresIn(root.blockList, []) : []
    readonly property bool hasFigures: root.warmBlocks.length > 0
    // An unplaced figure, made only for a document with figures, whose theme is the one every figure here is asked under.
    Loader {
        id: themeProbe
        active: root.hasFigures
        sourceComponent: Flea.MarkdownFigure {
            visible: false
            askArmed: false
            display: true
            bgHex: root.hexOf(Theme.color.background)
            fgHex: root.inkHex
            accentHex: root.accentHex
            mutedHex: root.mutedHex
            surfaceHex: root.surfaceHex
            fontFamily: Theme.font.family
            bodyPx: root.bodyPx
        }
    }
    // The blocks and the themes of both kinds, so the warm query names the cache keys the asks will use; null until the probe exists.
    readonly property var warmRequest: root.hasFigures && themeProbe.item ? { blocks: root.warmBlocks,
        themes: { math: themeProbe.item.themeOfKind("math"), mermaid: themeProbe.item.themeOfKind("mermaid") } } : null
    onWarmRequestChanged: FigureService.warm(root.warmRequest ? root.warmRequest.blocks : [], root.warmRequest ? root.warmRequest.themes : ({}))
    // Parse sequence numbers reject replies for an older file.
    property var blockList: []
    property int parseSeq: 0
    property int appliedSeq: 0
    property bool parsing: false
    // Parses that really ran (worker message or synchronous), so a suite counts one per event.
    property int parseRuns: 0
    // Loads of the shown file that landed, so a suite can tell a save the watcher read from one it never saw.
    property int loadRuns: 0
    // A worker reply landed for this file; the lazy suite asserts the parse left the UI thread.
    property bool parsedOffThread: false
    property string parseError: ""
    // Worker liveness for the quicklook-firstframe watchdog line: last seq of each message kind (-1 none), beats, fallback fires.
    property int ackSeq: -1
    property int beatSeq: -1
    property int beatCount: 0
    property int headSeq: -1
    property int replySeq: -1
    property int fallbackFires: 0
    readonly property bool fallbackRunning: parseFallback.running
    // Give the worker ten seconds to answer before the synchronous recovery parse.
    readonly property int parseFallbackMs: 10000
    // Keep this many pixels of blocks warm beyond the visible ListView window.
    readonly property int blockCachePixels: 600
    readonly property bool blocksReady: root.appliedSeq === root.parseSeq && !root.parsing
    readonly property bool loading: root.active && !root.readFailed && !root.tooLarge && root.parseError === ""
        && (!file.loaded || !root.blocksReady)
    readonly property string status: {
        if (root.tooLarge) return "This file is too large to preview."
        if (root.readFailed || root.parseError !== "") return "This file could not be read."
        return root.blocksReady && file.loaded ? "ready" : "loading"
    }
    readonly property string lineLabel: Markdown.countLine(Markdown.lineCount(root.rawText))
    // The bar's line count reads only once there is a file behind it, never "0 lines" first.
    readonly property bool contentReady: file.loaded && root.blocksReady
        && !root.tooLarge && !root.readFailed && root.parseError === ""
    // A first screen is drawn: blocks are in the list, the head of a parse still running or a whole list, and the swap counts either as shown.
    readonly property bool firstScreen: file.loaded && root.parseError === "" && root.blockList.length > 0
    // True when the reader has settled with nothing to put in the frame, the way PreviewLines.blank reads.
    readonly property bool blank: root.tooLarge
        || (root.active && !root.readFailed && file.loaded && root.rawText.length === 0)

    // What the Source text holds, so a suite sees that Rendered lays none out.
    readonly property int sourceChars: sourceList.laidChars
    readonly property Item bodyItem: body
    // The route a sideways stroke takes to a table wider than the document, which each such table joins.
    readonly property Item tableWheel: tableRoute
    // The first live table drawn wider than the document, which Quick Look's IPC reads for the sideways capture.
    function tableScroller() { return tableRoute.scrollers.filter(function (s) { return s })[0] || null }
    // The offset the wheel moved, in whichever view shows, for Quick Look's IPC.
    readonly property real scrollY: root.shownView === Markdown.SOURCE ? sourceList.contentY : body.contentY
    function scrollBy(pixelDelta) {
        var view = root.shownView === Markdown.SOURCE ? sourceList : body
        var low = view.originY - view.topMargin
        var high = Math.max(low, view.originY + view.contentHeight - view.height + view.bottomMargin)
        view.cancelFlick()
        view.contentY = Math.max(low, Math.min(high, view.contentY + pixelDelta))
        if (view === body) { root.releaseHeldPlace(); root.noteEnd() }
    }

    // The render suite reads live delegate geometry; only visible blocks plus the cache exist, so offscreen blocks answer null.
    function blockItem(i) {
        var kids = body.contentItem.children
        for (var k = 0; k < kids.length; k++) {
            if (kids[k].blockIndex === i)
                return kids[k]
        }
        return null
    }
    // Instantiated delegates only: the lazy suite asserts this stays bounded.
    function delegateCount() { return body.contentItem.children.length }
    // An absent figure delegate answers null to the render probe.
    function figureInfo(i) {
        var d = blockItem(i)
        if (!d)
            return null
        var box = null
        var kids = d.children
        for (var k = 0; k < kids.length; k++)
            if (kids[k].objectName === "figureBox" && kids[k].visible)
                box = kids[k]
        if (!box)
            return null
        var fig = null
        var inner = box.children
        for (var m = 0; m < inner.length; m++)
            if (inner[m].objectName === "figureItem")
                fig = inner[m]
        if (!fig)
            return null
        return { ready: fig.ready, failed: fig.failed, working: fig.working, boxW: box.width,
            imgW: fig.fitWidth, imgH: fig.fitHeight }
    }
    readonly property real flickContentHeight: sourceList.visible
        ? sourceList.contentHeight : body.contentHeight

    visible: root.active

    FileView {
        id: file
        printErrors: false
        // The one watcher is on the shown file; a path change re-points it, so an old file never reloads here.
        watchChanges: true
        // The first event opens the window and the rest land inside it: a restart would starve a file written without pause.
        onFileChanged: if (!reloadCoalesce.running) reloadCoalesce.start()
        onLoaded: {
            root.loadedText = file.text()
            // Only the first read of a path blocks; a reload after a save never does, whatever the file grew to.
            file.blockLoading = false
            root.loadRuns++
            // A save that unlinks and recreates the file can fail one reload; the next good load clears it.
            root.readFailed = false
            root.askParse()
        }
        onLoadFailed: {
            root.loadedText = ""
            file.blockLoading = false
            root.keepScroll = false
            root.readFailed = true
        }
        onPathChanged: root.readFailed = false
    }

    // One step moves the path and the blocking flag together, since a read inside the path change takes the flag as it stands then.
    function pointFile() {
        var want = (root.active && !root.tooLarge) ? root.path : ""
        // A pane built before the path moved sees the last file's path, which an open that named its file must not read first.
        if ((want === file.path) || (want !== "" && root.blockPath !== "" && want !== root.blockPath))
            return
        var blocking = want !== "" && want === root.blockPath
        file.blockLoading = blocking
        if (blocking) {
            root.blockedReads++
            // A view that holds another file takes a new path asynchronously, so the blocking read starts from none.
            file.path = ""
        }
        file.path = want
        // blockLoading gates only text(), so the blocking first read is taken here, in the first frame, not inside a binding.
        if (blocking) root.loadedText = file.text()
        else if (want === "") root.loadedText = ""
    }
    Component.onCompleted: root.pointFile()
    // The list builds its cache asynchronously, so a teardown mid-build drops the model first and no incubation outlives its context.
    Component.onDestruction: body.model = null
    onTooLargeChanged: root.pointFile()

    // One reload per save: an editor's truncate, write and rename raise events within a few ms, and 50 ms is under what a reader notices.
    readonly property int reloadCoalesceMs: 50
    Timer {
        id: reloadCoalesce
        interval: root.reloadCoalesceMs
        repeat: false
        onTriggered: root.reloadFromDisk()
    }

    // A reparse of the shown file (disk edit, theme change) puts the reader back where they were, by block or else by pixels.
    property real savedY: 0
    // The place by block: the block at the view's top and the pixels past its top edge, -1 when none is built there.
    property int savedIndex: -1
    property real savedOffset: 0
    property bool keepScroll: false
    // Where a restore left the view when the content was too short for the saved place, NaN when none waits.
    property real heldY: NaN
    // Two positions this close are the same place: the list stores contentY as a float.
    readonly property real samePlacePx: 1
    // True while the model is replaced: the list moves itself to its top then, and restoreScroll puts the view back.
    property bool settingBlocks: false
    // A place waiting for taller content ends the moment the reader moves the view away from it.
    function releaseHeldPlace() {
        if (root.settingBlocks || isNaN(root.heldY) || Math.abs(body.contentY - root.heldY) < root.samePlacePx)
            return
        root.keepScroll = false
        root.heldY = NaN
    }
    // A place waiting for taller content is kept; otherwise the reader's present place is saved (each landing asks first).
    function rememberScroll() {
        if (root.keepScroll)
            return
        root.savedY = body.contentY
        root.keepScroll = true
        // A path change or a failed load ends a hold without clearing its place, so a new place starts with none.
        root.heldY = NaN
        var item = root.topBlockItem()
        root.savedIndex = item === null ? -1 : item.blockIndex
        root.savedOffset = item === null ? 0 : body.contentY - item.y
    }
    // The first built block that ends below the view's top, so a view in a gap or above the first block still has one.
    function topBlockItem() {
        var kids = body.contentItem.children
        var found = null
        for (var k = 0; k < kids.length; k++) {
            var kid = kids[k]
            if (kid.blockIndex === undefined || kid.y + kid.height <= body.contentY)
                continue
            if (found === null || kid.blockIndex < found.blockIndex)
                found = kid
        }
        return found
    }
    // A place by block is restored by block: the list estimates an unbuilt document, and only a built end may clamp it.
    function restoreScroll() {
        if (!root.keepScroll)
            return
        var byBlock = root.savedIndex >= 0 && root.savedIndex < root.blockList.length
        if (byBlock)
            body.positionViewAtIndex(root.savedIndex, ListView.Beginning)
        var item = byBlock ? root.blockItem(root.savedIndex) : null
        var top = body.originY - body.topMargin
        var end = Math.max(top, body.originY + body.contentHeight - body.height + body.bottomMargin)
        var place = item === null ? root.savedY : item.y + root.savedOffset
        var endExact = item === null || root.blockItem(root.blockList.length - 1) !== null
        var at = Math.max(top, endExact ? Math.min(place, end) : place)
        var settled = !endExact || place <= end
        // The hold is recorded before the move, so the move itself reads as the same place.
        root.heldY = settled ? NaN : at
        body.contentY = at
        if (settled)
            root.keepScroll = false
    }
    // The content height last seen, which is where the end was before a block grew.
    property real seenHeight: 0
    // Whether the last block had a delegate at the last height change or move, so the end then was drawn, not estimated.
    property bool endBuilt: false
    // The snap below sets contentY itself, and the height can settle again under it.
    property bool snappingToEnd: false
    function noteEnd() {
        root.endBuilt = root.blockItem(root.blockList.length - 1) !== null
    }
    // A picture decodes after its block was built at no height; only a reader at the drawn end follows it, never one in unbuilt blocks.
    function holdEnd() {
        var was = root.seenHeight
        var builtBefore = root.endBuilt
        root.seenHeight = body.contentHeight
        root.noteEnd()
        if (root.snappingToEnd || !builtBefore || !(body.contentHeight > was))
            return
        var wasEnd = body.originY + was - body.height + body.bottomMargin
        var tallerThanView = wasEnd > body.originY - body.topMargin
        if (!tallerThanView || body.contentY < wasEnd - root.samePlacePx)
            return
        root.snappingToEnd = true
        body.contentY = body.originY + body.contentHeight - body.height + body.bottomMargin
        root.snappingToEnd = false
    }
    // How far the last block's bottom and the inset under it lie past the viewport, 0 when whole, -1 when the last block is not built.
    function endGap() {
        var last = root.blockItem(root.blockList.length - 1)
        return last === null ? -1 : Math.max(0, Math.round(last.mapToItem(body, 0, last.height).y + body.bottomMargin - body.height))
    }
    function reloadFromDisk() {
        if (!root.active || root.tooLarge)
            return
        file.reload()
    }
    // Link ink and code chrome are parsed into the runs, so a theme change reparses the loaded file.
    function reparseForTheme() {
        if (!root.active || !file.loaded)
            return
        root.askParse()
    }
    // A theme switch moves ink and chrome in one turn, so both ask through one callLater and the parse sees both.
    onInkHexChanged: Qt.callLater(root.reparseForTheme)
    onChromeHexChanged: Qt.callLater(root.reparseForTheme)

    // Large files require synchronous worker activation before their parse request.
    readonly property int workerThreshold: 65536
    Loader {
        id: parserLoader
        active: false
        sourceComponent: parserComponent
    }
    Component {
        id: parserComponent
        CountedWorker {
            source: "MarkdownWorker.js"
            onMessage: function (messageObject) { root.landed(messageObject) }
        }
    }
    function landed(messageObject) {
        if (messageObject.seq !== root.parseSeq)
            return
        // A live worker proves it with messages, so the fallback below fires only after 10 s of none, never for slowness.
        if (messageObject.yielded === true) {
            // One slice a message: only the live request asks for the next, so a newer request is read first.
            parseFallback.restart()
            parserLoader.item.sendMessage({ seq: messageObject.seq, cont: true })
            return
        }
        if (messageObject.ack === true || messageObject.progress === true) {
            if (messageObject.ack === true) root.ackSeq = messageObject.seq
            else { root.beatSeq = messageObject.seq; root.beatCount++ }
            parseFallback.restart()
            return
        }
        // The head of a first parse draws the first screen; the parse goes on, and the whole list replaces it when it lands.
        if (messageObject.partial === true) {
            root.headSeq = messageObject.seq
            parseFallback.restart()
            root.settingBlocks = true
            root.blockList = messageObject.blocks
            root.settingBlocks = false
            return
        }
        root.parsing = false
        root.appliedSeq = messageObject.seq
        root.replySeq = messageObject.seq
        if (messageObject.error !== "") {
            root.parseError = messageObject.error
            root.askedAny = false
            return
        }
        root.parseError = ""
        // The reader's present place is taken before the model reset moves the list to its top; a place that waits is kept.
        root.rememberScroll()
        root.settingBlocks = true
        root.blockList = messageObject.blocks
        root.settingBlocks = false
        // A reply that changed nothing the list sees raises no model change, so the saved place is released here.
        root.restoreScroll()
        root.parsedOffThread = true
    }

    // The worker parses only above workerThreshold; this timer recovers a worker request whose reply never settles.
    Timer {
        id: parseFallback
        interval: root.parseFallbackMs
        repeat: false
        onTriggered: {
            if (!root.parsing)
                return
            root.fallbackFires++
            // A late worker reply for the request being recovered is voided by the new number.
            root.parseSeq++
            root.releaseWorker()
            root.parseNow(root.askedText, root.askedDir, root.askedChrome, root.askedInk)
        }
    }

    // The last parsed request's text, folder, chrome and ink; a reload's second trigger (onRawTextChanged after onLoaded) is skipped.
    property string askedText: ""
    property string askedDir: ""
    property string askedChrome: ""
    property string askedInk: ""
    property bool askedAny: false
    function releaseWorker() {
        // The worker holds a live request's parse between slices; a cancel frees it.
        if (parserLoader.item !== null)
            parserLoader.item.sendMessage({ seq: root.parseSeq, cancel: true })
    }
    // Forget the last request and void any worker reply in flight, so the next ask always parses.
    function dropParse() {
        root.parseSeq++
        root.releaseWorker()
        root.parsing = false
        root.askedAny = false
        root.askedText = ""
    }

    // The synchronous parse of one request, landed like a worker reply: the small-file path and the worker's recovery both end here.
    function parseNow(text, dir, chrome, ink, deep) {
        var blocks = root.shareParse && !deep ? Prepared.take(root.path, text, dir, chrome, ink) : null
        if (blocks !== null)
            root.reusedParses++
        try {
            if (blocks === null)
                blocks = deep ? Markdown.deepBlocks() : Markdown.blocks(text, dir, chrome, ink)
        } catch (e) {
            root.parseError = String(e.message || e)
            root.appliedSeq = root.parseSeq
            root.parsing = false
            root.askedAny = false
            return
        }
        if (root.shareParse && !deep)
            Prepared.store(root.path, text, dir, chrome, ink, blocks)
        // Taken once the parse is good and before the model reset, like the worker landing: a parse that throws takes no place.
        root.rememberScroll()
        root.settingBlocks = true
        root.blockList = blocks
        root.settingBlocks = false
        root.parseError = ""
        root.appliedSeq = root.parseSeq
        root.parsing = false
        root.restoreScroll()
    }

    // Resolve images before Qt sees text: remote images become placeholders; only files beside the document load.
    function askParse() {
        if (!root.active || root.tooLarge || !file.loaded) {
            root.parseError = ""
            root.dropParse()
            return
        }
        // Read the file's own text: onLoaded can run before the rawText binding has caught up.
        var text = file.text()
        var dir = Markdown.dirOf(root.path)
        if (root.askedAny && text === root.askedText && dir === root.askedDir
                && root.chromeHex === root.askedChrome && root.inkHex === root.askedInk) {
            return
        }
        // Only a request that goes on to parse clears an error; a skipped one has nothing to replace it with.
        root.parseError = ""
        root.askedAny = true
        root.askedText = text
        root.askedDir = dir
        root.askedChrome = root.chromeHex
        root.askedInk = root.inkHex
        root.parseSeq++
        root.parseRuns++
        root.parsing = true
        // The live text length decides worker activation before bindings update; a big text with a deep head lands the sentinel unparsed.
        var big = text.length > root.workerThreshold
        var deep = big && Markdown.deepHead(text).deep
        var wantWorker = big && !deep
        parserLoader.active = wantWorker
        var w = parserLoader.item
        if (wantWorker && w) {
            parseFallback.restart()
            // Only a file with nothing drawn yet sends a head: a reparse keeps the list and the reader's place until the whole one lands.
            w.sendMessage({ seq: root.parseSeq, source: text, dir: dir, chrome: root.chromeHex, ink: root.inkHex, head: root.blockList.length === 0 ? Markdown.HEAD_BLOCKS : 0 })
            return
        }
        parserLoader.active = false
        parseFallback.stop()
        root.parseNow(text, dir, root.chromeHex, root.inkHex, deep)
    }

    onRawTextChanged: root.askParse()
    onActiveChanged: {
        root.pointFile()
        root.askParse()
    }
    onPathChanged: {
        // A save's pending reload belongs to the old file.
        reloadCoalesce.stop()
        root.keepScroll = false
        root.parseError = ""
        root.blockList = []
        root.parsedOffThread = false
        // The new file's load asks; asking now would parse the old file's text under the new path.
        root.dropParse()
        root.pointFile()
    }

    // Warming both view heights prevents a contentHeight binding loop on Rendered/Source changes.
    onViewChanged: {
        body.contentHeight
        sourceList.contentHeight
    }

    // Only the shown Source (a too-deep one too) lays out text, a few chunks at a time: an unseen 1 MiB Source held the UI thread 100 ms.
    Flea.MarkdownSource {
        id: sourceList
        anchors.fill: parent
        visible: (!root.tooLarge && !root.readFailed && root.parseError === "")
            && root.shownView === Markdown.SOURCE
        text: root.shownView === Markdown.SOURCE ? root.rawText : ""
        notice: root.tooDeep ? Markdown.deepNotice() : ""
        insetX: root.insetX
        insetY: root.insetY
        blockGap: root.blockGap
        bodyPx: root.bodyPx
        cachePixels: root.blockCachePixels
    }

    // Render only visible blocks and a bounded cache, even for a 1 MiB document.
    ListView {
        id: body
        anchors.fill: parent
        anchors.leftMargin: root.insetX
        anchors.rightMargin: root.insetX
        clip: true
        visible: (!root.tooLarge && !root.readFailed && root.parseError === "")
            && root.shownView !== Markdown.SOURCE
        model: root.blockList
        // A new model resets the view to its origin, so the saved place is restored once that reset is done.
        onModelChanged: root.restoreScroll()
        onContentYChanged: { root.releaseHeldPlace(); root.noteEnd() }
        onContentHeightChanged: root.holdEnd()
        spacing: root.blockGap
        topMargin: root.insetY
        bottomMargin: root.insetY
        cacheBuffer: root.blockCachePixels
        focus: false

        // The wheel and the bar belong to the frame, not to the inset list, so both sit on the root.
        FastScrollHandler {
            parent: root
            flickable: body
            visible: body.visible
        }

        Flea.ViewportScrollBar {
            parent: root
            anchors { top: parent.top; right: parent.right }
            flickable: body
        }

        // A sideways stroke over a table wider than the document goes to the table, above the handler that scrolls the document.
        Flea.MarkdownTableWheel {
            id: tableRoute
            parent: root
            visible: body.visible
        }

        delegate: Flea.MarkdownBlockView {
            block: modelData
            blockIndex: index
            blockCount: root.blockList.length
            siblings: root.blockList
            preview: root
            width: ListView.view.width
            // Only a figure intersecting the viewport may send a render request.
            inView: {
                var v = ListView.view
                if (!v)
                    return true
                return (y + height >= v.contentY) && (y <= v.contentY + v.height)
            }
        }
    }

    Text {
        anchors.centerIn: parent
        visible: root.tooLarge || root.readFailed || root.parseError !== ""
        text: root.status
        color: Theme.color.muted
        font.family: Theme.font.family
        font.pixelSize: Theme.font.body
        textFormat: Text.PlainText
    }
}
