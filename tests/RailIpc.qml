import QtQuick
import Quickshell
import Quickshell.Io

// The window's own railState over IPC, asked from the test the way the native checks ask it.
Scope {
    id: root
    property string json: ""
    // The test's check(name, actual, expected).
    property var report: null
    // A step condition: false until an answer lands, then it reports the fields once against the expected, so a later call asks again.
    function is(name, expected) {
        var state = root.take()
        if (state !== null) root.report(name, state, expected)
        return state !== null
    }
    // Parses the railState reply, one JSON object per call (fields from ui/Ipc.qml): {"hidden":false,"width":192,"inset":<int>,"pane":<int>,"revealed":true,"x":0}
    function take() {
        if (root.json === "") { if (!ask.running) ask.running = true; return null }
        var s = JSON.parse(root.json)
        root.json = ""
        return { hidden: s.hidden, width: s.width, revealed: s.revealed, x: s.x }
    }
    Process {
        id: ask
        command: [Quickshell.env("FLEA_IPC_HELPER"), "-p", Quickshell.shellDir, "call", "flea", "railState"]
        stdout: StdioCollector { waitForEnd: true; onStreamFinished: if (this.text.charAt(0) === "{") root.json = this.text }
    }
}
