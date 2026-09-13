pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import "js/UiState.js" as UiState

// All callers share one operation writer; the Rust updater locks and re-reads before editing.
QtObject {
    id: root
    readonly property var records: ((ViewState.state.places || {}).favourites || [])
    readonly property bool busy: writer.running
    property bool operationActive: false
    property int recordCount: 0
    property var statuses: ({})
    onRecordsChanged: {
        var previousCount = root.recordCount
        root.recordCount = root.records.length
        root.statuses = ({})
        if (!root.operationActive) root.externalChanged(previousCount)
    }
    property string lastError: ""
    signal wrote()
    signal failed(string message)
    signal completed(string requestId, bool success, string message)
    signal externalChanged(int previousCount)

    function apply(operation, requestId) {
        if (root.busy) { root.failed("A Favorites change is still being saved."); return false }
        root.lastError = ""
        writer.answer = ""
        writer.errorText = ""
        writer.requestId = requestId || ""
        writer.beforeRecords = root.records
        writer.expectedRecords = UiState.favouritesAfter(root.records, operation)
        root.operationActive = true
        writer.command = [Quickshell.env("FLEA_BIN") || "flea", "--favourites", JSON.stringify(operation)]
        writer.pending = true
        writer.running = true
        return true
    }
    function add(path, label, requestId) {
        return root.apply({ op: "add", record: { label: label, path: path } }, requestId)
    }
    function remove(index) {
        return root.apply({ op: "remove", index: index, expected: root.records })
    }
    function move(index, to) {
        return root.apply({ op: "move", index: index, to: to, expected: root.records })
    }
    function rename(index, label) {
        return root.apply({ op: "rename", index: index, label: label, expected: root.records })
    }
    function inspect(indices) {
        var pending = indices.filter(function (index) { return root.statuses[index] === undefined })
        if (pending.length === 0 || inspector.running) return
        inspector.answer = ""
        inspector.command = [Quickshell.env("FLEA_BIN") || "flea", "--favourites", JSON.stringify({ op: "inspect", indices: pending.slice(0, 128) })]
        inspector.running = true
    }
    function finish(expected) {
        var changed = JSON.stringify(root.records) !== JSON.stringify(expected)
        root.operationActive = false
        if (changed) root.externalChanged(writer.beforeRecords.length)
    }
    property var stateChanges: Connections {
        target: ViewState
        function onFavouritesReadFailed(message) { root.failed(message) }
    }
    property var inspection: Process {
        id: inspector
        property string answer: ""
        stdout: StdioCollector { onStreamFinished: inspector.answer = this.text }
        onExited: function (code) {
            if (code !== 0) return
            try {
                var rows = JSON.parse(inspector.answer).statuses
                var next = Object.assign({}, root.statuses)
                for (var i = 0; i < rows.length; i++) {
                    if (JSON.stringify(root.records[rows[i].index]) === JSON.stringify(rows[i].record))
                        next[rows[i].index] = rows[i].error
                }
                root.statuses = next
            } catch (error) { root.failed("Favorite availability could not be read.") }
        }
    }

    property var process: Process {
        id: writer
        property string answer: ""
        property string errorText: ""
        property bool pending: false
        property string requestId: ""
        property var beforeRecords: []
        property var expectedRecords: []
        onStarted: writer.pending = false
        onRunningChanged: {
            if (!running && pending) {
                pending = false
                root.finish(writer.beforeRecords)
                root.lastError = "Favorites updater could not start."
                root.failed(root.lastError)
                root.completed(writer.requestId, false, root.lastError)
            }
        }
        stdout: StdioCollector { onStreamFinished: writer.answer = this.text }
        stderr: StdioCollector { onStreamFinished: writer.errorText = this.text.trim() }
        onExited: function (code) {
            writer.pending = false
            if (code !== 0) {
                ViewState.refreshFavourites()
                root.finish(writer.beforeRecords)
                root.lastError = writer.errorText.replace(/^flea: /, "").split("\n")[0] || "Favorites could not be saved."
                root.failed(root.lastError)
                root.completed(writer.requestId, false, root.lastError)
                return
            }
            try {
                var state = JSON.parse(writer.answer)
                if (!state.places || !Array.isArray(state.places.favourites)) throw new Error("missing favorites")
                if (!ViewState.refreshFavourites()) throw new Error("saved favorites could not be read")
                root.finish(writer.expectedRecords)
                root.wrote()
                root.completed(writer.requestId, true, "")
            } catch (error) {
                root.finish(writer.beforeRecords)
                root.lastError = "Favorites were saved, but their new state could not be read."
                root.failed(root.lastError)
                // CLI exit zero means the write committed; a response error must never replay the add.
                root.completed(writer.requestId, true, root.lastError)
            }
        }
    }
}
