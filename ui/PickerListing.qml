import QtQuick
import Quickshell
import Quickshell.Io

// Listings can block inside FUSE. Only this read-only worker is replaceable; selection identities
// belong to the picker's separate backend, and the file manager's write backend is never signalled.
Item {
    id: root
    signal message(var value)
    signal failed(string reason)
    signal quitReady()
    property var current: null
    property var pending: null
    property bool quitting: false

    function clear() {
        pending = null
        if (current) {
            current.obsolete = true
            if (current.running) current.signal(9)
        }
    }
    function request(value) {
        if (quitting || (value.c !== "list" && value.c !== "listpaths")) return
        pending = value
        if (current) {
            current.obsolete = true
            if (current.running) current.signal(9)
        } else launch()
    }
    function launch() {
        if (quitting || !pending || current) return
        var value = pending
        pending = null
        current = worker.createObject(root, {request: value})
        current.running = true
    }
    function window(start, count) {
        if (current && current.running && !current.obsolete && !quitting)
            current.write(JSON.stringify({c: "window", start: start, count: count}) + "\n")
    }
    function quit() {
        quitting = true
        pending = null
        if (!current) { quitReady(); return }
        current.obsolete = true
        if (current.running) current.signal(9)
    }
    function ended(process, reason) {
        if (current !== process) return
        current = null
        process.destroy()
        if (quitting) quitReady()
        else if (pending) Qt.callLater(launch)
        else if (!process.obsolete) failed(reason)
    }
    Component.onDestruction: if (current && current.running) current.signal(9)

    Component {
        id: worker
        Process {
            id: process
            property var request
            property bool obsolete: false
            property bool started: false
            command: [Quickshell.env("FLEA_BIN") || "flea", "--backend"]
            stdinEnabled: true
            onStarted: {
                started = true
                if (obsolete || root.quitting) { signal(9); return }
                write(JSON.stringify(request) + "\n")
            }
            stdout: SplitParser {
                onRead: function(line) {
                    if (process.obsolete || root.quitting || root.current !== process || !line) return
                    try { root.message(JSON.parse(line)) }
                    catch (error) { root.failed("The listing backend sent an invalid reply.") }
                }
            }
            onExited: function(code, status) { root.ended(process, "The listing backend exited with code " + code) }
            // A failed QProcess spawn emits runningChanged, but never exited.
            onRunningChanged: if (!running && !started) root.ended(process, "The listing backend could not be started.")
        }
    }
}
