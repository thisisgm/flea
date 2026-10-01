import QtQuick
import Quickshell
import Quickshell.Io
import "js/Stack.js" as Stack

// Reads The Stack. The ranker is /usr/lib/flea/the-stack.py, with the home-directory copy as a
// fallback. This only runs it and watches the two files that can change the order while the place
// is open: the desktop's recent history and the touch file Flea writes when it opens a file.
Item {
    id: root

    property string home: ""
    // The pane sets this while The Stack is the place on screen, which is the only time a change
    // should poke the listing.
    property bool watch: false
    property bool collecting: false
    property bool again: false
    property bool streamFinished: false
    property bool processExited: false
    property int exitCode: -1
    property string result: ""
    property string errText: ""

    signal ready(var paths)
    signal failed(string text)
    signal sourcesChanged()

    readonly property string historyFile: Stack.historyPath(Quickshell.env("XDG_DATA_HOME"), root.home)
    readonly property string touchFile: Stack.touchPath(root.home)

    function refresh() {
        if (root.collecting) {
            root.again = true
            return
        }
        root.again = false
        root.collecting = true
        root.streamFinished = false
        root.processExited = false
        root.exitCode = -1
        root.result = ""
        root.errText = ""
        query.command = Stack.command(root.home, [])
        query.running = true
    }

    function finish() {
        if (!root.collecting || !root.streamFinished || !root.processExited)
            return
        root.collecting = false
        // A newer request already replaced this run. Its stdout is the order from a moment ago.
        if (root.again) {
            Qt.callLater(root.refresh)
            return
        }
        if (root.exitCode !== 0) {
            root.failed(root.errText.trim() || "The Stack could not be read.")
        } else {
            var parsed = null
            try {
                parsed = JSON.parse(root.result)
            } catch (e) {
                parsed = null
            }
            if (!parsed || !Array.isArray(parsed.paths))
                root.failed("The Stack could not be read.")
            else
                root.ready(parsed.paths)
        }
    }

    Process {
        id: query
        stdout: StdioCollector {
            onStreamFinished: {
                root.result = text
                root.streamFinished = true
                root.finish()
            }
        }
        stderr: StdioCollector {
            onStreamFinished: root.errText = text
        }
        onExited: function (code) {
            root.exitCode = code
            root.processExited = true
            root.finish()
        }
    }

    FileView {
        path: root.historyFile
        watchChanges: true
        printErrors: false
        onFileChanged: debounce.restart()
    }

    FileView {
        path: root.touchFile
        watchChanges: true
        printErrors: false
        onFileChanged: debounce.restart()
    }

    Timer {
        id: debounce
        interval: 400
        onTriggered: if (root.watch) root.sourcesChanged()
    }

    // mtime is not in either watched file. While the place is open, look again so a save or a
    // download can take the top without the operator leaving and coming back.
    Timer {
        interval: 8000
        repeat: true
        running: root.watch
        onTriggered: if (root.watch) root.sourcesChanged()
    }
}
