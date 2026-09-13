import QtQuick
import Quickshell
import Quickshell.Io
import "js/Taildrop.js" as TaildropJs

// The context menu's own Service, the same shape as ui/NetworkMounts.qml: this is the only thing
// here that touches tailscale, ui/ContextMenu.qml only reads "peers" and renders it.
Item {
    id: root

    property var peers: []
    property string reason: "checking"
    property string sendCommand: ""
    property bool checking: false
    property bool _awaitingStart: false
    // The OEM Tailscale service's pollWatchdog bounds status queries at fifteen seconds.
    readonly property int statusTimeoutSeconds: 15
    signal refreshed()
    // onExited can race the StdioCollector's own text, see ui/NetworkMounts.qml's header comment.
    property string _statusOutput: ""
    property string _statusError: ""

    function refresh(facts) {
        if (checking) return false
        peers = []
        var provider = facts.taildrop || {}, sender = facts.taildropSend || {}
        sendCommand = sender.command || ""
        reason = provider.reason || sender.reason || "checking"
        if (!provider.command || !sendCommand) return true
        checking = true
        _awaitingStart = true
        _statusOutput = ""
        _statusError = ""
        statusProcess.command = ["timeout", "--signal=KILL", String(statusTimeoutSeconds), provider.command, "status", "--json"]
        statusProcess.running = true
        return true
    }

    // argv-direct and detached: the script owns its own success/failure notification, see
    // docs/superpowers/specs/2026-08-31-flea-operations-design.md "4.1 Taildrop".
    function send(peerId, paths) {
        var peer = TaildropJs.byId(root.peers, peerId)
        if (!peer || !sendCommand || checking) return false
        Quickshell.execDetached([sendCommand, peer.address].concat(paths))
        return true
    }

    // ui/Pane.qml's own dispatch-confirmation message reads a name, not the id chosen() carries.
    function labelFor(peerId) {
        var peer = TaildropJs.byId(root.peers, peerId)
        return peer ? peer.label : "that peer"
    }

    Process {
        id: statusProcess
        stdout: StdioCollector {
            id: statusOut
            waitForEnd: true
            onStreamFinished: root._statusOutput = text
        }
        stderr: StdioCollector { id: statusErr; waitForEnd: true; onStreamFinished: root._statusError = text }
        onStarted: root._awaitingStart = false
        onRunningChanged: {
            if (root._awaitingStart && !running) {
                root._awaitingStart = false
                root.checking = false
                root.reason = "Tailscale status helper could not start"
                root.refreshed()
            }
        }
        onExited: function (exitCode) {
            root._awaitingStart = false
            var state = TaildropJs.status(statusOut.text || root._statusOutput, exitCode,
                exitCode === 137 || exitCode === 9 ? "Tailscale status was interrupted or timed out" : statusErr.text || root._statusError)
            root.peers = state.peers
            root.reason = state.reason
            root.checking = false
            root.refreshed()
        }
    }
}
