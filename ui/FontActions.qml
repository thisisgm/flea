import QtQuick
import Quickshell
import Quickshell.Io

// Only the selected font starts this helper. Its reply is stamped, so moving never applies the previous file's state.
Item {
    id: root
    property string path: ""
    property var info: null
    property string error: ""
    property string notice: ""
    property bool busy: false
    property string askedPath: ""
    property string verb: ""
    property string reply: ""
    signal finished(string verb)
    onPathChanged: { root.info = null; root.error = ""; root.notice = ""; Qt.callLater(root.inspect) }
    Component.onCompleted: Qt.callLater(root.inspect)

    function inspect() { if (root.path && !root.busy) root.run("info") }
    function run(action) {
        if (root.busy || !root.path) return
        root.busy = true
        root.askedPath = root.path
        root.verb = action
        root.reply = ""
        root.error = ""
        root.notice = ""
        child.command = [Quickshell.env("FLEA_BIN") || "flea", "--font", action, root.path]
        child.running = true
        deadline.restart()
    }
    function complete(code) {
        if (!root.busy) return
        deadline.stop()
        root.busy = false
        if (root.path !== root.askedPath) { root.inspect(); return }
        try {
            var result = JSON.parse(root.reply)
            if (code !== 0 || result.error) root.error = result.error || "The font operation failed."
            else {
                root.info = result
                if (!result.cacheOk) root.notice = "Font changed; the font cache could not be refreshed. Reopen applications to check it."
                else if (root.verb !== "info") root.notice = result.installed ? "Installed for your user. Reopen other applications if needed." : "Font uninstalled for your user."
            }
        } catch (e) { root.error = "The font helper could not be started or returned no result." }
        root.finished(root.verb)
    }
    Process {
        id: child
        stdout: StdioCollector { onStreamFinished: root.reply = this.text }
        onExited: function(code) { root.complete(code) }
        onRunningChanged: if (!running && root.busy) Qt.callLater(function() { if (!child.running) root.complete(2) })
    }
    Timer {
        id: deadline
        // Scan, variable-axis inspection, cache refresh, discovery and a rollback each have their own deadline.
        interval: 95000
        onTriggered: {
            child.running = false
            root.reply = JSON.stringify({ error: "The font operation timed out. Reopen the preview to check its state." })
            root.complete(2)
        }
    }
}
