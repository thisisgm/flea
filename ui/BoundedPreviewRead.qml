import QtQuick
import Quickshell.Io

// One immutable read request. head bounds bytes before the collector allocates them;
// completion waits for both EOF and exit so partial output on failure is refused.
Item {
    id: root
    required property string path
    required property int maxBytes
    required property int ticket
    property string text: ""
    property int bytes: 0
    property bool eof: false
    property bool exited: false
    property bool succeeded: false
    property bool delivered: false
    signal finished(int ticket, string text, int bytes, bool ok)

    function complete() {
        if (root.delivered || !root.eof || !root.exited) return
        root.delivered = true
        root.finished(root.ticket, root.text, root.bytes, root.succeeded)
    }
    Process {
        id: reader
        command: ["head", "-c", String(root.maxBytes + 1), "--", root.path]
        running: true
        stdout: StdioCollector {
            onStreamFinished: {
                root.text = this.text
                root.bytes = this.data.byteLength
                root.eof = true
                root.complete()
            }
        }
        stderr: StdioCollector {}
        onExited: function(code, status) {
            root.exited = true
            root.succeeded = code === 0 && status === 0
            root.complete()
        }
    }
    Component.onDestruction: if (reader.running) reader.running = false
}
