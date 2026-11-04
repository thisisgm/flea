import QtQuick
import "." as Flea
import "js/Columns.js" as Columns
import "js/Facts.js" as Facts
import "js/Thumbs.js" as Thumbs
import "js/ExtThumbs.js" as ExtThumbs
import "js/Keymap.js" as Keymap
import "js/PreviewKeys.js" as PreviewKeys
import "js/Search.js" as Search
import "js/PreviewSettle.js" as PreviewSettle
import "js/PreviewAsk.js" as PreviewAsk
import "js/PreviewSwap.js" as PreviewSwap

// Selection loading is independent of column visibility and the separate Quick Look overlay.
Flea.PreviewColumn {
    id: root
    property var pane: null
    property int loadedIndex: -1
    property string loadedDirectory: ""
    property string loadedIdentity: ""
    property int pendingToken: 0
    // ui/ColumnsArea.qml's third-column swap, which then drives the cursor moves; null leaves every change immediate.
    property var swap: null
    property string settleKey: ""
    // The running settle stands for a move the clock stamped and trailed; ColumnsArea's own arm stands for none.
    property bool settleTrails: false
    // A load queued under the swap's picture, so a duplicate replace queues no clear behind it.
    property string loadQueuedKey: ""
    // The multi-selection the loaded frame was built for; "" for a lone selection, which never keys on the version.
    property string loadedSelection: ""
    // Last distinct cursor move, so a held key cannot decode mid-burst: only idleness loads at once.
    property double lastMoveAt: 0
    property string lastMoveKey: ""
    signal thumbsApplied(var work)
    onExpandRequested: {
        if (!root.row || !root.pane) return
        var look = root.pane.quickLook()
        look.open(root.path, root.row.i, root.row.s, root.kindName)
        look.pdfItem.expandFrom(root.pdfPage(), PreviewKeys.PDF_ZOOM_MIN)
    }

    Keys.onPressed: function(event) {
        var context = root.rowState === Facts.PDF ? "pdf"
            : root.rowState === Facts.VIDEO || root.rowState === Facts.AUDIO ? "media" : "preview"
        var action = Keymap.lookup(event.key, event.text, event.modifiers, context)
        if (action === "escape" || action === "focusPreview") root.pane.listArea.forceActiveFocus()
        else if (action === "loadPreview") root.loadSelection()
        else if (root.rowState === Facts.PDF) PreviewKeys.pdfAction(action, root)
        else if (action === "preview") {
            var strip = root.mediaStripItem()
            if (strip) strip.toggled()
            else if (root.row && !root.row.d) root.pane.quickLook().open(root.path, root.row.i, root.row.s, root.kindName)
        } else if (action === "seekBack" || action === "seekForward") {
            var direction = action === "seekBack" ? -1 : 1
            if (root.rowState === Facts.PDF) root.turnPage(direction)
            else {
                var transport = root.mediaStripItem()
                if (transport) transport.seeked(root.mediaPosition() + direction * PreviewKeys.SEEK_MS)
            }
        } else if (action === "parent") root.turnPage(-1)
        else if (action === "pageForward") root.turnPage(1)
        event.accepted = true
    }

    readonly property bool canRead: root.visible && root.pane !== null && !root.pane.listInFlight
    overlayOpen: root.pane && root.pane.preview ? root.pane.preview.active : false
    thumb: root.pane ? (root.manualHold ? Thumbs.fileFor(root.pane.thumbState, root.pane.cursorIndex)
        : (root.loadedIndex >= 0 ? Thumbs.fileFor(root.pane.thumbState, root.loadedIndex) : "")) : ""
    noThumbComing: root.row !== null && (root.row.t !== true || !root.pane
        || Thumbs.refused(root.pane.thumbState, root.loadedIndex))
    // True from a move's clear until the next plain clear, the load's own; only then may the swap's fallback end, and its held-off mark show.
    property bool moved: false
    loadingHeldOff: root.swap !== null && root.swap.fellBack && !root.moved

    // What the swap waits for: the cursor row's preview whole, or nothing more coming for it.
    // A held frame is whole from the listing alone, cached thumbnail or not.
    readonly property bool ready: {
        if (root.manualHold)
            return !root.pending
        // A move's clear writes pending false and then true, and ready must not read true between the two: that ends the swap's fallback at every step of a held key.
        if (root.pane === null || root.row === null)
            return !root.pending && !root.moved
        if (root.loadedIndex !== root.pane.cursorIndex)
            return false
        return PreviewSwap.columnReady({ state: root.previewState, thumb: root.thumb.length > 0,
            frame: root.frameStatus, noThumbComing: root.noThumbComing, pdfDrawn: root.pdfDrawn,
            pdfFailed: root.pdfFailed, linesLoading: root.textLoading, meta: root.meta !== null })
    }

    // ExtThumbs: how much of a text file this frame may read; the column passes it on.
    textLimit: ExtThumbs.textLimit(root.pane ? root.pane.storageClass : "")
    truncateText: root.pane ? (root.pane.storageClass === "network" || root.pane.storageClass === "phone") : false

    function identity(row) {
        return row ? JSON.stringify([row.n, row.s, row.m, row.p, row.i]) : ""
    }

    function swapKey() { return root.pane.path + "\n" + root.pane.cursorIndex }

    // The settle runs from the key, and a second call for the same row does not push it back.
    function armSettle() {
        if (!ViewState.previewAutomatic || !root.pane)
            return
        var key = root.swapKey()
        if (settle.running && root.settleKey === key)
            return
        root.settleKey = key
        root.settleTrails = false
        settle.restart()
    }

    function clear() {
        settle.stop()
        root.moved = false
        root.clearShown()
    }

    // A move's clear: the loading state stands under the swap's picture until the settle loads the new row.
    function clearForMove() {
        root.moved = true
        root.clearShown()
        root.pending = true
    }

    // A folder or null row keeps the old preview: ColumnsArea holds the old column by data until the peek lands.
    function replace() {
        if (!Columns.isFileRow(root.pane ? root.pane.rowFor(root.pane.cursorIndex) : null)) { settle.stop(); return }
        var key = root.swapKey()
        // A true duplicate preserves the frame and timer; a changed identity lets a same-key refresh proceed.
        // A queued load stands for the same key: clearing behind it would wipe the load when it lands.
        if (key === root.lastMoveKey && (root.isShown() || root.loadQueuedKey === key)) return
        // A burst's move for this key already waits on its settle, so a rows landing under it holds nothing again.
        if (key === root.lastMoveKey && root.moved && settle.running && root.settleKey === key) return
        if (root.swap && ViewState.previewAutomatic && root.canRead) {
            // The picture first, the exact contract a deferred load keeps; the scheduler may load under it at once.
            root.swap.hold(root.clearForMove, key)
            root.settleFor(key)
            return
        }
        if (!root.canRead) { root.clear(); return }
        // No swap takes no picture, so the same scheduler decides after the same duplicate check above.
        root.clear()
        root.settleFor(key)
    }

    // Whether the loaded frame already names the cursor row: the duplicate test the key clock cannot make.
    function isShown() {
        var current = root.pane.rowFor(root.pane.cursorIndex)
        return root.loadedIndex === root.pane.cursorIndex && root.loadedDirectory === root.pane.path
            && root.loadedIdentity === root.identity(current)
            && root.loadedSelection === PreviewSettle.selectionKey(root.pane.selectionCount(), root.pane.selectionVersion)
    }

    // Refresh-or-new scheduler: a pending timer already covers this key, otherwise the move clock decides.
    function settleFor(key) {
        if (!ViewState.previewAutomatic || !root.pane) return
        var now = Date.now()
        var decision = PreviewSettle.plan(now, root.lastMoveAt, settle.interval, key, root.lastMoveKey,
            settle.running && root.settleKey === key, root.unrested(key))
        if (decision === "same") return
        root.lastMoveKey = key
        root.lastMoveAt = now
        if (decision === "now") {
            settle.stop()
            root.settleKey = key
            root.fireSettle()
            // Armed once the load's own work returns, work a starved loop can stretch past the settle.
            rested.restart()
        } else {
            root.settleKey = key
            root.settleTrails = true
            settle.restart()
        }
    }

    function clearShown() {
        root.loadQueuedKey = ""
        root.loadedSelection = ""
        root.pending = false
        root.pendingToken = 0
        root.manualHold = false
        root.loadedIndex = -1
        root.loadedDirectory = ""
        root.loadedIdentity = ""
        root.row = null
        root.meta = null
        root.path = ""
        root.kindName = ""
        root.selectionCount = 0
        root.selectedRows = []
    }

    function followSelection() {
        if (!root.canRead) return
        var candidate = root.pane.rowFor(root.pane.cursorIndex)
        // Unknown is never held: the class has not named its verdict yet, so the frame waits
        // under the swap's cap and onStorageKnownChanged decides when it lands, never a held mark.
        if (!root.pane.storageKnown) {
            if (root.swap) root.swap.start(false)
            return
        }
        var hold = ExtThumbs.manualHold(root.pane.storageClass, ViewState.preview)
        if (root.loadedDirectory === root.pane.path && root.loadedIndex === root.pane.cursorIndex
                && root.loadedIdentity === root.identity(candidate) && root.manualHold === hold) return
        // A folder or null row keeps the old preview until its peek lands; clearing here would blank the held column.
        if (!Columns.isFileRow(candidate)) { settle.stop(); return }
        // An off class holds the frame on the listing's own facts until Ctrl+Space loads it; nothing loads, so no swap.
        if (hold) { root.clear(); root.holdSelection(candidate); return }
        root.replace()
    }

    // The held frame: the row, its path and its facts from the listing alone, no meta and no
    // thumbnail ask. The loaded identity is recorded so rows replies do not clear and re-hold.
    // Space still opens Quick Look, which loads on request.
    function holdSelection(candidate) {
        if (!candidate || candidate.d) return
        root.manualHold = true
        root.loadedIndex = root.pane.cursorIndex
        root.loadedDirectory = root.pane.path
        root.loadedIdentity = root.identity(candidate)
        root.row = Object.assign({}, candidate)
        root.path = root.pane.join(Search.baseOf(root.pane), candidate.n)
        root.kindName = root.pane.kindNames[candidate.k] || ""
        root.selectionCount = root.pane.selectionCount()
        root.loadedSelection = PreviewSettle.selectionKey(root.pane.selectionCount(), root.pane.selectionVersion)
        root.selectedRows = root.pane.selectedIndices().map(function (index) { return root.pane.rowFor(index) }).filter(function (row) { return row !== null })
        if (root.swap) root.swap.start(false)
    }

    // Ctrl+Space calls this directly; automatic selection reaches it only after selection settles.
    function loadSelection() {
        if (!root.canRead) return
        if (!Columns.isFileRow(root.pane.rowFor(root.pane.cursorIndex))) { settle.stop(); return }
        var key = root.swapKey()
        root.loadQueuedKey = key
        if (root.swap) root.swap.hold(root.load, key, true)
        else root.load()
    }

    // The swap is told the new preview's work has begun, so its cap counts from here, a PDF's after its document settle.
    function load() {
        if (!root.canRead) return
        var pane = root.pane
        var current = pane.rowFor(pane.cursorIndex)
        if (!Columns.isFileRow(current)) { settle.stop(); root.startSwap(false); return }
        root.clear()
        root.loadedIndex = pane.cursorIndex
        root.loadedDirectory = pane.path
        root.loadedIdentity = root.identity(current)
        root.row = Object.assign({}, current)
        root.path = pane.join(Search.baseOf(pane), current.n)
        root.kindName = pane.kindNames[current.k] || ""
        root.selectionCount = pane.selectionCount()
        root.loadedSelection = PreviewSettle.selectionKey(pane.selectionCount(), pane.selectionVersion)
        root.selectedRows = pane.selectedIndices().map(function (index) { return pane.rowFor(index) }).filter(function (row) { return row !== null })
        if (root.selectionCount > 1) { root.startSwap(false); return }
        var kind = Facts.state(current, 1, false, "", root.kindName)
        root.startSwap(kind === Facts.PDF)
        root.pendingToken = pane.backend.askMeta(root.loadedIndex, kind === Facts.TEXT || kind === Facts.CODE,
            kind === Facts.VIDEO || kind === Facts.AUDIO, kind === Facts.ARCHIVE)
        root.askThumb()
    }

    // The single-row ask, held until the storage class lands: unknown reads as generating,
    // so asking early would thumbnail a NAS image in full over the network.
    function askThumb() {
        var pane = root.pane
        if (!pane || root.loadedIndex < 0) return
        if (root.manualHold || !root.isShown()) return
        var current = pane.rowFor(root.loadedIndex)
        var work = PreviewAsk.workFor({ canRead: root.canRead, storageKnown: pane.storageKnown,
            selectionCount: root.selectionCount, index: root.loadedIndex,
            rowT: current ? !!current.t : false,
            asked: pane.thumbState.file[root.loadedIndex] !== undefined,
            storageClass: pane.storageClass, preview: ViewState.preview })
        if (!work) return
        root.thumbsApplied(work)
        pane.backend.thumb(work.ask, work.cacheOnly)
    }

    // Idle press asks the cursor thumb before the capture, so load's own ask sends nothing twice.
    function askCursorThumb() {
        if (!ViewState.previewAutomatic || !ViewState.previewColumn) return
        var pane = root.pane
        if (!pane) return
        var current = pane.rowFor(pane.cursorIndex)
        if (!Columns.isFileRow(current)) return
        var key = root.swapKey()
        var work = PreviewAsk.earlyWorkFor({ now: Date.now(), lastMoveAt: root.lastMoveAt,
            interval: settle.interval, key: key, lastMoveKey: root.lastMoveKey,
            armed: settle.running && root.settleKey === key, pending: root.unrested(key), canRead: root.canRead,
            storageKnown: pane.storageKnown, selectionCount: pane.selectionCount(),
            index: pane.cursorIndex, rowT: !!current.t,
            asked: pane.thumbState.file[pane.cursorIndex] !== undefined,
            storageClass: pane.storageClass, preview: ViewState.preview })
        if (!work) return
        root.thumbsApplied(work)
        pane.backend.thumb(work.ask, work.cacheOnly)
    }

    function startSwap(isPdf) { if (root.swap) root.swap.start(isPdf) }

    // The settle's whole decision, shared by the timer and the idle fast path, so both spend the same gates.
    function fireSettle() {
        if (!ViewState.previewAutomatic) return
        var hold = root.pane ? (!root.pane.storageKnown || ExtThumbs.manualHold(root.pane.storageClass, ViewState.preview)) : false
        if (hold) root.followSelection()
        else root.loadSelection()
    }

    // The cursor has not rested since its last move: an earlier row's move still trailing, or a load made at once whose rest has not run out.
    function unrested(key) { return (settle.running && root.settleTrails && root.settleKey !== key) || rested.running }
    // A single-shot Timer stays running until its trigger, a posted event, is handled, so a held key's next repeat that a starved loop
    // handles before then has not rested, however late its own load work made it, and trails instead of loading at once again.
    Timer { id: rested; interval: settle.interval }
    Timer {
        id: settle
        interval: root.pane ? root.pane.settleMs : 120
        // The automatic settle holds an off class; Ctrl+Space's direct loadSelection stays full.
        // Unknown waits through followSelection, so the first settle never spends the class fsinfo has not named yet.
        onTriggered: root.fireSettle()
    }
    onCanReadChanged: {
        // A listing going out leaves the preview to Nav.forget's reset, which clears it when held rows go.
        if (!root.canRead && (!root.visible || root.pane === null)) root.clear()
        else if (root.canRead) root.followSelection()
    }
    Connections {
        target: ViewState
        function onPreviewAutomaticChanged() {
            if (ViewState.previewAutomatic) root.followSelection()
            else settle.stop()
        }
        // A class switch moves the frame between held and loaded without a cursor move.
        function onStateChanged() { root.followSelection() }
    }
    Connections {
        target: root.pane
        // With a swap, ui/ColumnsArea.qml calls followSelection after it has put the hold in place.
        function onCursorIndexChanged() { if (!root.swap) root.followSelection() }
        // The class lands with fsinfo, after the rows; a settle fired in between spent local.
        function onStorageClassChanged() { root.followSelection() }
        // A frame loaded before the class lands never asked; the ask runs here, once the gate names it.
        function onStorageKnownChanged() { root.followSelection(); root.askThumb() }
        function onRowsChanged() {
            if (root.loadedIndex >= 0 && root.loadedIdentity !== root.identity(root.pane.rowFor(root.loadedIndex)))
                root.replace()
            else if (!root.swap)
                root.followSelection()
        }
        function onPathChanged() { root.replace() }
        function onSelectionVersionChanged() { root.replace() }
    }
    Connections {
        target: root.pane ? root.pane.backend : null
        function onMetaResult(message) {
            if (!root.canRead || !root.pendingToken || message.token !== root.pendingToken
                    || root.loadedDirectory !== root.pane.path
                    || root.loadedIdentity !== root.identity(root.pane.rowFor(root.loadedIndex))) return
            root.pendingToken = 0
            root.meta = { w: message.w, h: message.h, durationMs: message.ms, sampleRate: message.rate,
                entries: message.entries, unpacked: message.unpacked, archiveFailed: message.afailed,
                names: message.names, lines: message.lines, partial: message.partial,
                linesFailed: message.lfailed, target: message.target, targetDir: message.targetdir,
                owner: message.owner || "", orient: message.orient || 1, frames: message.frames || 0, photo: message.photo || null }
        }
    }
    Component.onCompleted: root.followSelection()
}
