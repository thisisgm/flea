//@ pragma ShellId flea-process-output-test

import QtQuick
import Quickshell

// Each service owns a StdioCollector fallback because Process.onExited can race the collector's
// text property. These values deliberately poison every fallback, then start a second run. The
// reset is synchronous, so this catches stale cross-run output without depending on signal order.
ShellRoot {
    id: root

    property bool checked: false

    NetworkMounts { id: network }
    DeviceMounts { id: devices }
    Taildrop { id: taildrop }

    function finish() {
        Quickshell.execDetached(["kill", String(Quickshell.processId)])
    }

    Timer {
        interval: 500
        running: true
        repeat: false
        onTriggered: {
            root.checked = true

            network._mountErrOutput = "old mount error: already mounted"
            network.openShare("smb://new-host/share", false, "New")

            network._infoOutput = "local path: /old/share"
            network.runInfo("smb://new-host/share")

            network._listSharesOutput = "old-share"
            network.listShares("smb://new-host/")

            network._mountListOutput = "Mount(0): old -> smb://old/share/"
            network.pollMounts()

            devices._listOutput = '{"blockdevices":[{"name":"old"}]}'
            devices.poll()

            taildrop._statusOutput = '{"Peer":{"old":{}}}'
            taildrop.refresh()

            var fresh = network._mountErrOutput === ""
                && network._infoOutput === ""
                && network._listSharesOutput === ""
                && network._mountListOutput === ""
                && devices._listOutput === ""
                && taildrop._statusOutput === ""
            console.log("PROCESS_OUTPUT fresh=" + (fresh ? "yes" : "no"))
            root.finish()
        }
    }

    Timer {
        interval: 5000
        running: true
        onTriggered: {
            console.log("PROCESS_OUTPUT timeout checked=" + root.checked)
            root.finish()
        }
    }
}
