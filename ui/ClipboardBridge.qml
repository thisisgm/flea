import QtQuick
import Quickshell.Io
import "js/ClipShare.js" as ClipShare

// The bridge between this window's clipboard and the compositor's, so a second Flea window, and
// Nautilus beside it, see a Copy or Cut made here and this window sees theirs. ui/WindowBody.qml
// publishes every clipboard its panes set and applies every one this reads back; the payload and
// its rules are ui/js/ClipShare.js's, and wl-clipboard is the hard dependency Copy Path already
// uses. The watch is ui/TrashMonitor.qml's shape: one long-lived wl-paste whose every line says
// the selection changed, and a one-shot read that fetches what it became.
Item {
    id: root

    // The state the system clipboard last agreed with, in ClipShare.key form. Publishing it again
    // is the echo of a read, and reading a publish back lands on it too, so neither can loop.
    property string _last: ""
    // A tick that lands mid-read is kept, ui/MountListing.qml's idiom: the selection changed again
    // under the read, so the answer that just landed describes a clipboard that is already gone.
    property bool _readAgain: false
    // The collector fallback every stdout-reading Process here carries: onExited can race the text.
    property string _readOutput: ""
    // What a publish that arrived mid-write still owes, the ui/ViewState.qml writer idiom: the
    // newest payload wins and the ones between were never the selection for longer than a write.
    property string _owed: ""
    property bool _owing: false
    // wl-paste could not be run at all, said once; the clipboard then stays this window's own.
    property bool _gone: false

    // A payload read back from the compositor, ui/js/Ops.js's clipboard shape; an empty selection,
    // or one that is not files, arrives as the empty clipboard so a stale Paste never stays lit.
    signal arrived(var clip)
    signal message(string text, bool isError)

    function publish(clip) {
        if (root._gone)
            return
        var key = ClipShare.key(clip)
        if (key === root._last)
            return
        root._last = key
        if (writer.running) {
            root._owed = key
            root._owing = true
            return
        }
        root.write(key)
    }

    // An empty clipboard releases the selection rather than owning an empty one, so a Cut this
    // window spent stops offering the moved files to every other window; see ui/CollideHost.qml.
    function write(key) {
        writer.command = key.length === 0
            ? ["wl-copy", "--clear"]
            : ["sh", "-c", "printf '%s' \"$1\" | wl-copy -t x-special/gnome-copied-files", "_", key]
        writer.running = true
    }

    Process {
        id: writer
        onExited: function (exitCode) {
            if (exitCode !== 0)
                root.message("The copy could not be shared with other windows.", true)
            if (root._owing) {
                root._owing = false
                root.write(root._owed)
            }
        }
    }

    function read() {
        if (root._gone)
            return
        if (reader.running) {
            root._readAgain = true
            return
        }
        root._readOutput = ""
        reader.running = true
    }

    Process {
        id: reader
        command: ["wl-paste", "-n", "-t", "x-special/gnome-copied-files"]
        stdout: StdioCollector {
            id: readOut
            waitForEnd: true
            onStreamFinished: root._readOutput = text
        }
        onExited: function (exitCode) {
            var again = root._readAgain
            root._readAgain = false
            if (again) {
                root._readOutput = ""
                reader.running = true
                return
            }
            // wl-paste exits non-zero when the selection carries no files, measured on this box,
            // which is every text copy made anywhere: the mirror empties, the way Nautilus's own
            // Paste goes grey the moment something else owns the clipboard.
            root.apply(exitCode === 0 ? (readOut.text || root._readOutput || "") : "")
        }
    }

    function apply(text) {
        var clip = ClipShare.parse(text)
        var key = clip === null ? "" : ClipShare.key(clip)
        if (key === root._last)
            return
        root._last = key
        root.arrived(clip === null ? { paths: [], moving: false } : clip)
    }

    // Every line is one selection change, whoever made it; the read that follows says what to.
    Process {
        id: watcher
        running: true
        command: ["wl-paste", "-w", "echo", "x"]
        stdout: SplitParser { onRead: root.read() }
        stderr: StdioCollector {}
        // A watcher that ran and ended is a compositor hiccup and is re-armed; one that never ran
        // has no wl-paste to run, which ui/ViewState.qml's writer rule reads off running-with-no-exit.
        property bool exited: false
        onExited: {
            watcher.exited = true
            if (!root._gone)
                rearm.restart()
        }
        onRunningChanged: {
            if (!watcher.running && !watcher.exited) {
                root._gone = true
                root.message("Copy and paste with other windows is off: wl-paste could not be run.", true)
            }
        }
    }

    Timer {
        id: rearm
        interval: 2000
        onTriggered: { watcher.exited = false; watcher.running = true }
    }

    // The selection standing when the window opened is already somebody's copy, Nautilus's or an
    // earlier Flea's, so the first read happens unasked rather than waiting for the next change.
    Component.onCompleted: root.read()
}
