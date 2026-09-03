import QtQuick
import Quickshell
import Quickshell.Io
import "js/Discovery.js" as Discovery
import "js/Tailnet.js" as Tailnet

// Optional, bounded discovery. Neither process owns mounting: both produce ordinary network rows
// and ui/NetworkMounts.qml remains the one service that hands a URI to gio.
Item {
    id: root

    property var tailnetPeers: []
    property var lanEntries: []
    property string tailnetState: "unknown"
    property string tailnetMessage: ""
    property string lanState: "unknown"
    property string _tailOut: ""
    property string _tailErr: ""
    property string _lanOut: ""
    property bool _tailTimedOut: false
    property bool _lanTimedOut: false
    readonly property bool disabled: Quickshell.env("FLEA_DISABLE_NETWORK_DISCOVERY") === "1"
    readonly property var entries: Tailnet.entries(root.tailnetPeers, Quickshell.env("USER"))
        .concat(root.lanEntries).concat(root.statusEntries())

    signal message(string text, bool isError)

    function statusEntries() {
        if (root.tailnetState === "unknown" || root.tailnetState === "ready") return []
        return [{ path:"", label:root.tailnetMessage, group:"network", kind:"status",
                  uri:"flea-status://tailscale/", mounted:false, glyph:"alert", origin:"tailnet",
                  health:root.tailnetState === "no-peers" ? "unknown" : "failed",
                  address:"", peerId:"", taildrop:false, mac:"" }]
    }

    function refresh() {
        if (!tailProcess.running) {
            root._tailTimedOut = false
            root._tailOut = ""
            root._tailErr = ""
            tailProcess.running = true
            tailTimeout.restart()
        }
        if (!lanProcess.running) {
            root._lanTimedOut = false
            root._lanOut = ""
            lanProcess.running = true
            lanTimeout.restart()
        }
    }

    Component.onCompleted: if (!root.disabled) root.refresh()

    Timer {
        interval: 30000
        running: !root.disabled
        repeat: true
        onTriggered: root.refresh()
    }

    Timer {
        id: tailTimeout
        interval: 5000
        onTriggered: {
            if (!tailProcess.running) return
            root._tailTimedOut = true
            tailProcess.running = false
            root.tailnetState = "failed"
            root.tailnetMessage = "Tailscale status timed out."
        }
    }

    Process {
        id: tailProcess
        command: ["tailscale", "status", "--json"]
        stdout: StdioCollector { id: tailOut; waitForEnd: true; onStreamFinished: root._tailOut = text }
        stderr: StdioCollector { id: tailErr; waitForEnd: true; onStreamFinished: root._tailErr = text }
        onExited: function (exitCode) {
            tailTimeout.stop()
            if (root._tailTimedOut) return
            var parsed = Tailnet.parseResult(exitCode, tailOut.text || root._tailOut, tailErr.text || root._tailErr)
            root.tailnetState = parsed.state
            root.tailnetMessage = parsed.message
            root.tailnetPeers = parsed.peers
        }
    }

    Timer {
        id: lanTimeout
        interval: 5000
        onTriggered: {
            if (!lanProcess.running) return
            root._lanTimedOut = true
            lanProcess.running = false
            root.lanState = "failed"
        }
    }

    Process {
        id: lanProcess
        command: ["avahi-browse", "-artp"]
        stdout: StdioCollector { id: lanOut; waitForEnd: true; onStreamFinished: root._lanOut = text }
        onExited: function (exitCode) {
            lanTimeout.stop()
            if (root._lanTimedOut) return
            if (exitCode !== 0) {
                root.lanState = "unavailable"
                root.lanEntries = []
                return
            }
            root.lanEntries = Discovery.parse(lanOut.text || root._lanOut)
            root.lanState = root.lanEntries.length > 0 ? "ready" : "empty"
        }
    }
}
