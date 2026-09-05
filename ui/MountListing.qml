import QtQuick
import Quickshell.Io

// The five second "gio mount -l" poll, lifted out of ui/NetworkMounts.qml whole so that file has
// room in the 0.1.4 composition. Nothing else drives it: the Service reads "text" on "listed".
Item {
    id: root

    // Nothing filesystem-watchable tells us when a share appears or drops: a mount materialises a
    // directory under /run/user/1000/gvfs that inotify never reports an event for, measured on this box.
    readonly property int pollMs: 5000
    // A listing is bounded the same way a mount is in ui/NetworkMounts.qml. "gio mount -l" against a
    // share whose server has stopped answering never returns, and an unbounded one froze the rail.
    readonly property int timeoutMs: 10000
    // The C locale ui/NetworkMounts.qml pins on its own gio calls, passed in so one property sets it.
    property var environment: ({})
    // The last listing that finished on time; a listing this component ended never replaces it.
    property string text: ""

    // Raised once "text" holds the new listing, so a handler that rebuilds reads it and not the last.
    signal listed()

    // A Process's own onExited can race its StdioCollector's text property, so the listing is also
    // captured via onStreamFinished as a fallback; see the OEM Dropbox panel's Service.qml.
    property string _output: ""
    // A re-read asked for mid-listing used to be dropped, leaving a just-mounted share to wait out
    // the poll; this remembers it instead, and listProcess runs it the moment the listing ends.
    property bool _pollAgain: false
    // Cleared only when the next listing starts, never in onExited, so it still reads true while the
    // ended listing's own stream finishes and cannot overwrite the last good listing with nothing.
    property bool _timedOut: false

    Timer {
        interval: root.pollMs
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: root.poll()
    }

    function poll() {
        if (listProcess.running) {
            root._pollAgain = true
            return
        }
        root._timedOut = false
        listProcess.running = true
        listTimeout.restart()
    }

    Timer {
        id: listTimeout
        interval: root.timeoutMs
        repeat: false
        onTriggered: {
            if (!listProcess.running)
                return
            // Ending it is what lets the next poll run at all; a listing nobody can end froze the rail.
            root._timedOut = true
            listProcess.running = false
        }
    }

    Process {
        id: listProcess
        environment: root.environment
        command: ["gio", "mount", "-l"]
        stdout: StdioCollector { id: listOut; waitForEnd: true; onStreamFinished: if (!root._timedOut) root._output = listOut.text }
        onExited: function () {
            listTimeout.stop()
            // A listing this timer ended collected nothing, and reading that as "no shares" would
            // empty the rail, taking the Unmount action with it exactly when a server is misbehaving.
            if (!root._timedOut) {
                root.text = listOut.text || root._output || ""
                root.listed()
            }
            if (root._pollAgain) {
                root._pollAgain = false
                root.poll()
            }
        }
    }
}
