import QtQuick
import Quickshell
import Quickshell.Io

Item {
    id: root
    property string executable: ""
    readonly property bool available: executable.length > 0

    function send(paths) {
        if (root.available && paths.length > 0)
            Quickshell.execDetached([root.executable].concat(paths))
    }

    // Native Omarchy and upstream Linux packages use these two executable names.
    // Probe once at startup; file paths never enter a shell command.
    Process {
        command: ["sh", "-c", "command -v localsend || command -v localsend_app"]
        running: true
        stdout: StdioCollector {
            waitForEnd: true
            onStreamFinished: root.executable = text.trim()
        }
    }
}
