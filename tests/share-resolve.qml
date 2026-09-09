//@ pragma ShellId flea-share-resolve-test

import QtQuick
import Quickshell

// Drives ui/ShareResolve.qml through every answer it can give, one root after another against the
// gio stub tests/share-resolve.sh puts on PATH, and prints one SHARE_RESOLVE line per outcome.
ShellRoot {
    id: root

    property bool finished: false
    property int at: 0
    // Each step: the root asked for, the signal expected and the value it should carry.
    readonly property var steps: [
        { root: "smb://nas/isos", want: "resolved", value: "/gvfs/isos" },
        { root: "smb://nas/locked", want: "failed", value: "Needs a password; add it under Network first" },
        { root: "smb://nas/live", want: "resolved", value: "/gvfs/live" },
        { root: "sftp://box/", want: "resolved", value: "/gvfs/box" },
        { root: "smb://nas/gone", want: "failed", value: "Connect failed: network location was refused" },
        { root: "smb://nas/nopath", want: "failed", value: "Connect failed: location has no browsable folder" },
        { root: "smb://nas/slow", want: "failed", value: "Connect failed: host did not respond" },
        { root: "smb://nas/isos", want: "resolved", value: "/gvfs/isos" }
    ]

    function finish(message) {
        if (root.finished)
            return
        root.finished = true
        console.log(message)
        Quickshell.execDetached(["kill", String(Quickshell.processId)])
    }

    function next() {
        if (root.at >= root.steps.length) {
            root.finish("SHARE_RESOLVE PASS steps=" + root.at)
            return
        }
        var shareRoot = root.steps[root.at].root
        resolver.resolve(shareRoot)
        // The slow root's info leg sleeps, so a second line lands while it runs.
        if (shareRoot === "smb://nas/slow")
            overlap.restart()
    }

    function got(kind, shareRoot, value) {
        var step = root.steps[root.at]
        if (shareRoot !== step.root || kind !== step.want || value !== step.value) {
            root.finish("SHARE_RESOLVE FAIL step=" + root.at + " got " + kind + " " + shareRoot + " " + value)
            return
        }
        console.log("SHARE_RESOLVE step=" + root.at + " " + kind + " " + shareRoot)
        root.at += 1
        root.next()
    }

    ShareResolve {
        id: resolver
        legTimeoutMs: 400
        onResolved: function (shareRoot, localPath) { root.got("resolved", shareRoot, localPath) }
        onResolveFailed: function (shareRoot, message) {
            // The slow root is asked for twice while its legs run: the second is refused as busy
            // and the first still answers with the deadline, so the queue is not advanced for it.
            if (message === "Another location is still connecting") {
                console.log("SHARE_RESOLVE busy " + shareRoot)
                return
            }
            root.got("failed", shareRoot, message)
        }
    }

    Timer {
        interval: 100
        running: true
        repeat: false
        onTriggered: root.next()
    }

    Timer {
        id: overlap
        interval: 150
        repeat: false
        onTriggered: resolver.resolve("smb://nas/overlap")
    }

    Timer {
        interval: 6000
        running: true
        repeat: false
        onTriggered: root.finish("SHARE_RESOLVE FAIL timeout at step " + root.at)
    }
}
