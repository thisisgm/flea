//@ pragma ShellId flea-network-open-share-test

import QtQuick
import Quickshell
import Quickshell.Io

ShellRoot {
    id: root

    property bool finished: false
    property bool started: false
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
            if (root.phase === 1 && path === "/child-should-open") {
                root.finish("NETWORK_OPEN_SHARE overlap=blocked sequential=open")
                return
            }
            root.finish("NETWORK_OPEN_SHARE FAIL opened=" + path + " phase=" + root.phase)
        }

        onRetryRequested: function (uri, label, password, reason, failedConnect) {
            root.finish("NETWORK_OPEN_SHARE FAIL retry=" + uri + " reason=" + reason)
        }

        onSharesListed: function (baseUri, baseLabel, names) {
            var correct = baseUri === "smb://first/"
                && baseLabel === "First"
                && names.length === 1
                && names[0] === "first-share"
            if (!correct) {
                root.finish("NETWORK_OPEN_SHARE FAIL shares=" + baseUri + "|" + baseLabel + "|" + names.join(","))
                return
            }
            root.phase = 1
            network.openShare("smb://first/child/", false, "child")
        }
    }

    Process {
        id: waitForList
        command: ["sh", "-c", "while [ ! -e \"$FLEA_TEST_LIST_STARTED\" ]; do sleep 0.01; done"]
        running: root.started
        onExited: function (exitCode) {
            if (exitCode !== 0) {
                root.finish("NETWORK_OPEN_SHARE FAIL marker-wait=" + exitCode)
                return
            }
            network.openShare("smb://second/", true, "Second")
        }
    }

    Timer {
        interval: 100
        running: true
        repeat: false
        onTriggered: {
            network.openShare("smb://first/", true, "First")
            root.started = true
        }
    }

    Timer {
        interval: 5000
        running: true
        repeat: false
        onTriggered: root.finish("NETWORK_OPEN_SHARE FAIL timeout")
    }
}
