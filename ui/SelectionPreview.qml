import QtQuick
import "." as Flea
import "js/Facts.js" as Facts
import "js/Thumbs.js" as Thumbs
import "js/Keymap.js" as Keymap
import "js/PreviewKeys.js" as PreviewKeys

// Selection loading is independent of column visibility and the separate Quick Look overlay.
Flea.PreviewColumn {
    id: root
    property var pane: null
    property int loadedIndex: -1
    property string loadedDirectory: ""
    property string loadedIdentity: ""
    property int pendingToken: 0
    signal thumbsApplied(var work)
    onExpandRequested: {
        if (!root.row || !root.pane) return
        root.pane.preview.open(root.path, root.row.i, root.row.s)
        root.pane.preview.pdfItem.expandFrom(root.pdfPage(), root.pdfZoom)
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
            else if (root.row && !root.row.d) root.pane.preview.open(root.path, root.row.i, root.row.s)
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
    thumb: root.pane && root.loadedIndex >= 0 ? Thumbs.fileFor(root.pane.thumbState, root.loadedIndex) : ""
    noThumbComing: root.row !== null && (root.row.t !== true || !root.pane
        || Thumbs.refused(root.pane.thumbState, root.loadedIndex))

    function identity(row) {
        return row ? JSON.stringify([row.n, row.s, row.m, row.p, row.i]) : ""
    }

    function clear() {
        settle.stop()
        root.pendingToken = 0
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
        if (root.loadedDirectory === root.pane.path && root.loadedIndex === root.pane.cursorIndex
                && root.loadedIdentity === root.identity(candidate)) return
        root.clear()
        if (ViewState.previewAutomatic) settle.restart()
    }

    // Ctrl+Space calls this directly; automatic selection reaches it only after selection settles.
    function loadSelection() {
        if (!root.canRead) return
        var pane = root.pane
        var current = pane.rowFor(pane.cursorIndex)
        root.clear()
        if (!current) return
        root.loadedIndex = pane.cursorIndex
        root.loadedDirectory = pane.path
        root.loadedIdentity = root.identity(current)
        root.row = Object.assign({}, current)
        root.path = pane.join(pane.path, current.n)
        root.kindName = pane.kindNames[current.k] || ""
        root.selectionCount = pane.selectionCount()
        root.selectedRows = pane.selectedIndices().map(function (index) { return pane.rowFor(index) }).filter(function (row) { return row !== null })
        if (current.d || root.selectionCount > 1) return
        var kind = Facts.state(current, 1, false, "", root.kindName)
        root.pendingToken = pane.backend.askMeta(root.loadedIndex, kind === Facts.TEXT || kind === Facts.CODE,
            kind === Facts.VIDEO || kind === Facts.AUDIO, kind === Facts.ARCHIVE)
        if (current.t && pane.thumbState.file[root.loadedIndex] === undefined) {
            var work = { ask: [root.loadedIndex], drop: [] }
            root.thumbsApplied(work)
            pane.backend.thumb(work.ask)
        }
    }

    Timer {
        id: settle
        interval: root.pane ? root.pane.settleMs : 120
        onTriggered: if (ViewState.previewAutomatic) root.loadSelection()
    }
    onCanReadChanged: {
        if (!root.canRead) root.clear()
        else root.followSelection()
    }
    Connections {
        target: ViewState
        function onPreviewAutomaticChanged() {
            if (ViewState.previewAutomatic) root.followSelection()
            else settle.stop()
        }
    }
    Connections {
        target: root.pane
        function onCursorIndexChanged() { root.followSelection() }
        function onRowsChanged() {
            if (root.loadedIndex >= 0 && root.loadedIdentity !== root.identity(root.pane.rowFor(root.loadedIndex)))
                root.clear()
            root.followSelection()
        }
        function onPathChanged() { root.clear(); root.followSelection() }
        function onSelectionVersionChanged() { root.clear(); root.followSelection() }
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
                owner: message.owner || "" }
        }
    }
    Component.onCompleted: root.followSelection()
}
