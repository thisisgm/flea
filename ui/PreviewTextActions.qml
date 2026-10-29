pragma ComponentBehavior: Bound

import QtQuick
import "." as Flea

// Plain text opens directly as a bounded selectable document; Markdown source
// and code selection views are built on request under the same file-size gate.
Item {
    id: root

    required property Item target
    property int topInset: 0
    property string path: ""
    property bool available: false
    property int maxBytes: 1048576
    // Markdown already owns its source; null means read the text file on demand.
    property var sourceText: null
    property bool directSelection: false
    property bool selecting: false
    readonly property bool selectionShown: root.directSelection || root.selecting
    property bool copyPending: false
    property bool requested: false
    property bool copied: false
    property bool readReady: false
    property int readBytes: 0
    property int readTicket: 0
    property var reader: null
    property string failure: ""
    property string loadedText: ""
    readonly property bool ready: root.available && (root.sourceText !== null || root.readReady) && root.failure === ""
    readonly property string content: root.sourceText !== null ? root.sourceText : root.loadedText
    readonly property var selectionItem: selection.item
    readonly property alias selectControl: selectButton
    readonly property alias copyControl: copyButton
    implicitWidth: (selectButton.visible ? selectButton.width : 0) + copyButton.width
    implicitHeight: Theme.chromeHeight

    function reset() {
        root.readTicket++
        if (root.reader !== null) {
            root.reader.destroy()
            root.reader = null
        }
        root.readBytes = 0
        copiedTimer.stop()
        root.selecting = false
        root.copyPending = false
        root.requested = false
        root.copied = false
        root.failure = ""
        root.loadedText = ""
        root.readReady = false
    }
    onPathChanged: {
        root.reset()
        if (root.available && root.directSelection) root.request()
    }
    onAvailableChanged: {
        if (!root.available) root.reset()
        else if (root.directSelection) root.request()
    }
    onMaxBytesChanged: {
        root.reset()
        if (root.available && root.directSelection) root.request()
    }
    Component.onCompleted: if (root.available && root.directSelection) root.request()

    function request() {
        if (!root.requested && root.sourceText === null)
            root.reader = readerFactory.createObject(root, { path: root.path, maxBytes: root.maxBytes, ticket: root.readTicket })
        root.requested = true
        if (root.sourceText !== null) {
            var text = String(root.sourceText)
            // UTF-8 byte count without creating a second encoded copy of the document.
            var bytes = 0
            for (var i = 0; i < text.length && bytes <= root.maxBytes; i++) {
                var c = text.charCodeAt(i)
                if (c < 128) bytes++
                else if (c < 2048) bytes += 2
                else if (c >= 0xd800 && c <= 0xdbff && i + 1 < text.length && text.charCodeAt(i + 1) >= 0xdc00 && text.charCodeAt(i + 1) <= 0xdfff) { bytes += 4; i++ }
                else bytes += 3
            }
            if (bytes > root.maxBytes) root.failure = "This file is too large to preview."
            else if (text.indexOf("\u0000") >= 0) root.failure = "This is a binary file."
        }
    }
    onSelectingChanged: if (root.selecting) root.request()

    function copyAll() {
        if (!root.available) return
        root.request()
        if (root.failure !== "") return
        root.copyPending = true
        root.finishCopy()
    }
    function finishCopy() {
        if (!root.copyPending || !root.ready) return
        if (root.content.length === 0) {
            root.copyPending = false
            return
        }
        clipboard.text = root.content
        clipboard.selectAll()
        clipboard.copy()
        clipboard.text = ""
        root.copyPending = false
        root.copied = true
        copiedTimer.restart()
    }
    onReadyChanged: if (root.ready) Qt.callLater(root.finishCopy)

    function readLanded(ticket, text, bytes, ok) {
        if (ticket !== root.readTicket || !root.available) return
        root.readBytes = bytes
        if (!ok) root.failure = "This file could not be read."
        else if (bytes > root.maxBytes) root.failure = "This file is too large to preview."
        else if (text.indexOf("\u0000") >= 0) root.failure = "This is a binary file."
        else {
            root.loadedText = text
            root.readReady = true
        }
        if (root.failure !== "") root.copyPending = false
        root.finishCopy()
    }
    Component {
        id: readerFactory
        Flea.BoundedPreviewRead {
            onFinished: function(ticket, text, bytes, ok) { root.readLanded(ticket, text, bytes, ok) }
        }
    }

    // Qt owns the system clipboard. File contents never become a command argument.
    TextEdit { id: clipboard; visible: false; textFormat: TextEdit.PlainText }
    Timer { id: copiedTimer; interval: 1800; onTriggered: root.copied = false }

    Flea.ChromeButton {
        id: selectButton
        anchors.left: parent.left
        gesturePolicy: TapHandler.ReleaseWithinBounds
        glyph: "text-select"
        active: root.selecting
        visible: !root.directSelection
        enabled: root.available
        accessName: root.selecting ? "Return to preview" : "Select text (source)"
        onActivated: {
            root.selecting = !root.selecting
            if (!root.selecting) root.target.forceActiveFocus()
        }
    }
    Flea.ChromeButton {
        id: copyButton
        anchors.left: root.directSelection ? parent.left : selectButton.right
        gesturePolicy: TapHandler.ReleaseWithinBounds
        glyph: root.copied ? "check" : "copy"
        enabled: root.available && root.failure === "" && !root.copyPending && (!root.ready || root.content.length > 0)
        accessName: root.copied ? "Copied" : "Copy all text"
        onActivated: root.copyAll()
    }
    Loader {
        id: selection
        parent: root.target
        z: 1
        anchors.fill: parent
        anchors.topMargin: root.topInset
        active: root.selectionShown && root.available
        sourceComponent: Flea.SelectablePreviewText {
            text: root.ready ? root.content : ""
            notice: root.failure !== "" ? root.failure : !root.ready ? "Loading text…" : ""
        }
    }
}
