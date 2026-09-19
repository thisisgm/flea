import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import "js/Cloud.js" as Cloud

// One mount-scoped, read-only query at a time. A path change invalidates even a late old reply.
Item {
    id: root
    property string path: ""
    property var snapshot: Cloud.empty()
    readonly property string statusText: Cloud.line(snapshot)
    property int generation: 0
    property int requestedGeneration: -1
    property string requestedPath: ""
    property bool collecting: false
    property bool streamFinished: false
    property bool processExited: false
    property int exitCode: -1
    property bool timedOut: false
    property string result: ""
    visible: statusText.length > 0
    height: visible ? Theme.chromeHeight : 0

    function refresh() {
        if (collecting) return
        if (path.charAt(0) !== "/") { snapshot = Cloud.empty(); return }
        requestedPath = path
        requestedGeneration = generation
        collecting = true
        streamFinished = false
        processExited = false
        exitCode = -1
        timedOut = false
        result = ""
        query.command = [Quickshell.env("FLEA_BIN") || "flea", "--cloud-status", requestedPath]
        query.running = true
        timeout.restart()
    }
    function finish() {
        if (!collecting || !streamFinished || !processExited) return
        collecting = false
        timeout.stop()
        if (!Cloud.accepts(generation, requestedGeneration, path, requestedPath)) {
            Qt.callLater(refresh)
            return
        }
        var next = exitCode === 0 && !timedOut ? Cloud.decode(result, requestedPath) : Cloud.empty("unavailable")
        if (next.state === "unavailable" && !next.mount) next.mount = snapshot.mount
        snapshot = next
        poll.interval = Cloud.interval(next)
        poll.restart()
    }
    onPathChanged: {
        generation += 1
        snapshot = Cloud.navigating(snapshot, path)
        poll.stop()
        // Do not reuse the Process until both its output and exit have been collected.
        if (!collecting) Qt.callLater(refresh)
    }
    Component.onCompleted: refresh()
    Component.onDestruction: query.running = false
    Timer { id: poll; onTriggered: root.refresh() }
    Timer {
        id: timeout
        interval: 5000
        onTriggered: {
            root.timedOut = true
            if (query.running) query.signal(9)
            else {
                // QProcess FailedToStart emits neither exited nor a finished stdout stream.
                root.processExited = true
                root.streamFinished = true
                root.finish()
            }
        }
    }
    Process {
        id: query
        stdout: StdioCollector {
            onStreamFinished: { root.result = text; root.streamFinished = true; root.finish() }
        }
        stderr: StdioCollector {}
        onExited: function(code, status) {
            root.exitCode = code
            root.processExited = true
            if (root.timedOut) root.streamFinished = true
            root.finish()
        }
    }
    Rectangle { anchors.fill: parent; color: Theme.color.surface }
    Text {
        anchors.fill: parent
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.rightMargin: Theme.spacing.rowPaddingX
        verticalAlignment: Text.AlignVCenter
        text: root.statusText
        textFormat: Text.PlainText
        elide: Text.ElideRight
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        color: root.snapshot.state === "error" || root.snapshot.state === "retrying" ? Theme.color.error : Theme.color.muted
    }
}
