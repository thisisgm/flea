import QtQuick
import Quickshell
import Quickshell.Io

// One owned process at a time. A retry cannot reuse it until the old child has exited.
Item {
    id: root
    property var snapshot: ({state: "ready", message: "Originals are kept. Existing different files are refused.", bytes: 0, total: 0, speed: 0})
    property bool busy: false
    property bool started: false
    property bool cancelling: false
    property var worker: null
    property int generation: 0
    property var targets: []
    property bool loading: false
    property string targetError: ""
    property string targetResult: ""
    property bool targetStream: false
    property bool targetExit: false
    property int targetCode: -1
    property bool listTimedOut: false
    function loadTargets() {
        if (loading || busy) return
        loading = true; targets = []; targetError = ""; targetResult = ""; targetStream = false; targetExit = false; targetCode = -1; listTimedOut = false
        listing.command = [Quickshell.env("FLEA_BIN") || "flea", "--cloud-targets"]
        listing.running = true
        listDeadline.restart()
    }
    function finishTargets() {
        if (!loading || !targetStream || !targetExit) return
        loading = false
        listDeadline.stop()
        try {
            var value = JSON.parse(targetResult)
            if (targetCode === 0 && value.targets instanceof Array) targets = value.targets
            else targetError = value.message || "Cloud targets could not be loaded."
        } catch (_) { targetError = "Cloud targets could not be loaded." }
    }
    function start(target, folder, source) {
        if (busy || loading || !target) return false
        cancelDeadline.stop()
        generation += 1
        snapshot = {state: "preparing", message: "Starting direct upload…", bytes: 0, total: 0, speed: 0}
        busy = true; started = false; cancelling = false
        worker = workerFactory.createObject(root, {token: generation, command: [Quickshell.env("FLEA_BIN") || "flea", "--cloud-copy", target, folder, source]})
        worker.running = true
        startDeadline.restart()
        jobDeadline.restart()
        return true
    }
    function cancel() {
        if (!busy || cancelling || terminal(snapshot.state)) return
        cancelling = true
        snapshot = {state: "cancelling", message: "Cancelling; originals are kept. Uploaded files may remain.", bytes: 0, total: 0, speed: 0}
        if (started) worker.write("cancel\n")
        cancelDeadline.restart()
    }
    function terminal(state) { return ["done", "error", "cancelled"].indexOf(state) >= 0 }
    function receive(line) {
        if (!busy) return
        try {
            var next = JSON.parse(line)
            if (["preparing", "uploading", "verifying", "done", "error", "cancelled"].indexOf(next.state) < 0) return
            if (cancelling && !terminal(next.state)) return
            if (next.message === "") next.message = snapshot.message
            snapshot = next
        } catch (_) {}
    }
    Timer {
        id: listDeadline; interval: 5000
        onTriggered: {
            root.listTimedOut = true
            if (listing.running) listing.signal(9)
            else { root.targetExit = true; root.targetStream = true; root.finishTargets() }
        }
    }
    Timer {
        id: startDeadline; interval: 5000
        onTriggered: if (root.busy && !root.started) {
            if (root.worker) root.worker.abortOnStart = true
            if (root.worker && root.worker.running) root.worker.signal(9)
            else { if (root.worker) root.worker.destroy(); root.worker = null; root.busy = false; root.cancelling = false }
            root.snapshot = {state: "error", message: "The upload helper could not start.", bytes: 0, total: 0, speed: 0}
        }
    }
    Timer {
        id: jobDeadline; interval: 86400000
        onTriggered: if (root.busy && root.worker) {
            root.snapshot = {state: "error", message: "Upload exceeded the 24-hour deadline. Originals kept.", bytes: 0, total: 0, speed: 0}
            root.worker.signal(9)
        }
    }
    Timer { id: cancelDeadline; interval: 10000; onTriggered: if (root.worker && root.worker.running) root.worker.signal(9) }
    Process {
        id: listing
        stdout: StdioCollector { onStreamFinished: { root.targetResult = text; root.targetStream = true; root.finishTargets() } }
        onExited: function(code, status) { root.targetCode = code; root.targetExit = true; if (root.listTimedOut) root.targetStream = true; root.finishTargets() }
    }
    function exited(process, code) {
        if (process.token !== generation) { process.destroy(); return }
        startDeadline.stop(); cancelDeadline.stop(); jobDeadline.stop()
        if (code === 0 && snapshot.state === "done") {} // Verified completion wins a simultaneous cancel.
        else if (snapshot.state === "error") {}
        else if (cancelling || snapshot.state === "cancelled") snapshot = {state: "cancelled", message: "Cancelled; originals kept. Uploaded files may remain.", bytes: 0, total: 0, speed: 0}
        else snapshot = {state: "error", message: "Upload ended without checksum confirmation.", bytes: 0, total: 0, speed: 0}
        worker = null; cancelling = false; busy = false
        process.destroy()
    }
    Component {
        id: workerFactory
        Process {
            id: process
            property int token: -1
            property bool abortOnStart: false
            stdinEnabled: true
            stdout: SplitParser { onRead: function(data) { if (process.token === root.generation) root.receive(data) } }
            onStarted: {
                if (abortOnStart || token !== root.generation) { process.signal(9); return }
                root.started = true; startDeadline.stop()
                if (root.cancelling) process.write("cancel\n")
            }
            onExited: function(code, status) { root.exited(process, code) }
        }
    }
    Component.onDestruction: { if (worker && worker.running) worker.write("cancel\n"); listing.running = false }
}
