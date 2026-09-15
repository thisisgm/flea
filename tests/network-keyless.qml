//@ pragma ShellId flea-network-keyless-test

import QtQuick
import Quickshell
import Quickshell.Io

// An sftp place is mounted once with no password at all before any credential is demanded, because
// gvfs's sftp backend is the ssh binary and a key or an agent can answer it. Phase 1 proves the
// passwordless open; phase 2 proves a refused one is what asks. tests/network-keyless.sh drives it.
ShellRoot {
    id: root

    property bool finished: false
    property int phase: 0

    function finish(message) {
        if (root.finished)
            return
        root.finished = true
        console.log(message)
        Quickshell.execDetached(["kill", String(Quickshell.processId)])
    }

    NetworkMounts {
        id: network

        onOpened: function (path) {
            // Phase 1 is a key opening a place with no password at all; phase 4 is a key opening
            // one that has a remembered password, which the helper leg used to claim first.
            if (path !== "/key-should-open"
                    || (root.phase !== 1 && root.phase !== 4)) {
                root.finish("NETWORK_KEYLESS FAIL opened=" + path + " phase=" + root.phase)
                return
            }
            if (root.phase === 1) {
                root.phase = 2
                network.openShare("sftp://ask@slot.test/home", false, "Ask slot")
                return
            }
            root.phase = 5
            network.remember("sftp://pw@slot.test/home", "fixture-secret")
            network.openShare("sftp://pw@slot.test/home", false, "Pw slot")
        }

        onRetryRequested: function (uri, label, password, reason, failedConnect) {
            var asked = reason === "Enter the password to mount this location."
                && failedConnect === false
            // Phase 2 is a refused path-shaped place; phase 3 is a refused server root, which the
            // bare-root listing used to claim before the prompt could reach it; phase 5 is a refused
            // place whose password this session already remembered, and it must come back with it.
            if (root.phase === 2 && asked && password === "" && uri === "sftp://ask@slot.test/home") {
                root.phase = 3
                network.openShare("sftp://ask@slot.test/", false, "Ask root")
                return
            }
            if (root.phase === 3 && asked && password === "" && uri === "sftp://ask@slot.test/") {
                root.phase = 4
                // A key opens this one even though a password is remembered for it, which is the
                // whole point: the remembered secret is a fallback, not a first resort.
                network.remember("sftp://key@slot.test/home", "fixture-secret")
                network.openShare("sftp://key@slot.test/home", false, "Key slot")
                return
            }
            if (root.phase === 5 && asked && password === "fixture-secret"
                    && uri === "sftp://pw@slot.test/home") {
                root.finish("NETWORK_KEYLESS passwordless=open needs-password=asked bare-root=asked"
                            + " remembered=kept")
                return
            }
            root.finish("NETWORK_KEYLESS FAIL retry=" + uri + " reason=" + reason
                        + " password=" + password + " phase=" + root.phase)
        }
    }

    Timer {
        interval: 200
        running: true
        repeat: false
        onTriggered: {
            root.phase = 1
            network.openShare("sftp://key@slot.test/home", false, "Key slot")
        }
    }

    Timer {
        interval: 6000
        running: true
        repeat: false
        onTriggered: root.finish("NETWORK_KEYLESS FAIL timeout phase=" + root.phase)
    }
}
