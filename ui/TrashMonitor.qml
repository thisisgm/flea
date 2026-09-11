import QtQuick
import Quickshell
import Quickshell.Io

// GIO provides the shared Trash count, including Trash on other mounted volumes.
Item {
    id: root
    property bool enabled: true
    property int count: 0
    property bool ready: false
    property bool refreshPending: false
    property bool collecting: false
    property bool timedOut: false
    property bool streamFinished: false
    property bool processExited: false
    property int exitCode: -1
    property string result: ""
    signal changed()
    signal failed(string message)

    function ownedCommand(args) {
        // setpriv ties the exec'd helper to this UI; the PPID check covers death before signal registration.
        return ["setpriv", "--pdeathsig", "TERM", "--", "sh", "-c",
            '[ "$PPID" = "$1" ] || exit 1; shift; exec "$@"',
            "flea-trash", String(Quickshell.processId)].concat(args)
    }
    function refresh() {
        if (!enabled) return
        if (collecting) { refreshPending = true; return }
        refreshPending = false
        collecting = true
        timedOut = false
        streamFinished = false
        processExited = false
        result = ""
        query.running = true
        timeout.restart()
    }
    function finish() {
        if (!collecting || !streamFinished || !processExited) return
        collecting = false
        timeout.stop()
        // Sample GIO output: "  trash::item-count: 12".
        var match = result.match(/^\s*trash::item-count:\s*(\d+)\s*$/m)
        if (exitCode === 0 && !timedOut && match) {
            count = Number(match[1])
            ready = true
        } else failed("Could not read the Trash count.")
        if (refreshPending) Qt.callLater(refresh)
    }
    onEnabledChanged: {
        if (enabled) refresh()
        else { timeout.stop(); query.running = false; refreshPending = false }
    }
    Component.onCompleted: refresh()
    Component.onDestruction: { monitor.running = false; query.running = false }

    Timer {
        id: settle
        interval: 120
        onTriggered: { root.refresh(); root.changed() }
    }
    Timer {
        id: timeout
        interval: 10000
        onTriggered: { root.timedOut = true; query.running = false }
    }
    Process {
        id: monitor
        running: root.enabled
        command: root.ownedCommand(["gio", "monitor", "--dir=trash:///"])
        stdout: SplitParser { onRead: settle.restart() }
        stderr: StdioCollector {}
        onExited: function(code, status) {
            if (root.enabled && code !== 0) root.failed("Trash monitoring is unavailable.")
        }
    }
    Process {
        id: query
        command: root.ownedCommand(["gio", "info", "--attributes=trash::item-count", "trash:///"])
        environment: ({LC_ALL: "C"})
        stdout: StdioCollector {
            onStreamFinished: { root.result = text; root.streamFinished = true; root.finish() }
        }
        stderr: StdioCollector {}
        onExited: function(code, status) {
            root.exitCode = code
            root.processExited = true
            root.finish()
        }
    }
}
