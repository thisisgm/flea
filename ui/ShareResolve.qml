import QtQuick
import Quickshell.Io
import "js/Mounts.js" as Mounts
import "js/ShareUrl.js" as ShareUrl

// One share root to its FUSE path: the mount leg and the info leg the Network rail runs in
// ui/NetworkMounts.qml, with no rail, no password form and no share listing, for the chooser's
// location field. The info leg is the judge, as it is there: gio mount's exit code only colours
// the sentence when info finds no path. Each leg has the rail's deadline, because a share that
// wants a credential hangs gio with nobody to answer it. tests/share-resolve.sh drives it headless
// against a gio stub.
Item {
    id: root

    // The rail's own bound; tests/share-resolve.sh shortens it to prove the deadline.
    property int legTimeoutMs: 15000
    // Pinned to C so gio's credential wording, which ShareUrl.failure reads, cannot be translated.
    readonly property var gioEnvironment: ({ "LC_ALL": "C" })

    property string _pending: ""
    property bool _mountFailed: false
    property string _mountStderr: ""
    property string _infoOutput: ""
    property bool _timedOut: false

    signal resolved(string root, string localPath)
    signal resolveFailed(string root, string message)

    // Read at the call and never bound: a binding on the two legs reads stale inside the exit
    // handler that answers the previous line, and the next line is often typed from there.
    function busy() {
        return mountProcess.running || infoProcess.running
    }

    function resolve(shareRoot) {
        // Single flight: a second line while the legs run would hand this deadline to itself.
        if (root.busy()) {
            root.resolveFailed(shareRoot, ShareUrl.BUSY)
            return
        }
        root._pending = shareRoot
        root._mountFailed = false
        root._mountStderr = ""
        root._timedOut = false
        mountProcess.command = ShareUrl.mountCommand(shareRoot)
        mountProcess.running = true
        legTimeout.restart()
    }

    function runInfo() {
        root._infoOutput = ""
        infoProcess.command = ShareUrl.infoCommand(root._pending)
        infoProcess.running = true
        legTimeout.restart()
    }

    function fail(message) {
        var pending = root._pending
        root._pending = ""
        root.resolveFailed(pending, message)
    }

    Timer {
        id: legTimeout
        interval: root.legTimeoutMs
        repeat: false
        onTriggered: {
            // Whichever leg is still running is the one that missed the deadline. Its own onExited
            // reports the failure, once the kill has landed, so the next line is never refused as
            // busy by a leg that is only still dying.
            root._timedOut = true
            if (mountProcess.running) {
                mountProcess.running = false
            } else if (infoProcess.running) {
                infoProcess.running = false
            }
        }
    }

    Process {
        id: mountProcess
        environment: root.gioEnvironment
        stderr: StdioCollector { id: mountErr; waitForEnd: true; onStreamFinished: root._mountStderr = text }
        onExited: function (exitCode) {
            legTimeout.stop()
            if (root._timedOut) {
                root.fail(ShareUrl.TIMEOUT)
                return
            }
            // A refusal for a share the rail already mounted and a refusal for one that does not
            // exist differ only in a sentence, so neither ends it here: info answers with a FUSE
            // path when the location really is mounted, whatever this code was.
            root._mountFailed = exitCode !== 0
            root.runInfo()
        }
    }

    Process {
        id: infoProcess
        environment: root.gioEnvironment
        stdout: StdioCollector { id: infoOut; waitForEnd: true; onStreamFinished: root._infoOutput = text }
        onExited: function (exitCode) {
            legTimeout.stop()
            if (root._timedOut) {
                root.fail(ShareUrl.TIMEOUT)
                return
            }
            var path = Mounts.localPath(String(infoOut.text || root._infoOutput || ""))
            if (exitCode === 0 && path.length > 0) {
                var pending = root._pending
                root._pending = ""
                root.resolved(pending, path)
                return
            }
            root.fail(ShareUrl.failure(root._mountFailed, String(mountErr.text || root._mountStderr || "")))
        }
    }
}
