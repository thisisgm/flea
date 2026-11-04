import QtQuick
import Quickshell
import Quickshell.Io
import "js/GlassTag.js" as GlassTag

// Omarchy's Transparent mode is Hyprland's to draw: a Flea window Hyprland leaves at 1 takes the transparency hotkey's tag, so no alpha stacks.
// ui/Glass.qml makes this when Hyprland opens the first Flea window, after the first rows, and binds the mode to it.
QtObject {
    id: root

    property string mode: "transparent"
    property bool usesDefault: true
    // The addresses this process tagged, so a tag the user set with the hotkey is never taken away.
    property var tagged: ({})
    // A request that lands mid-run waits this long and asks again, until every process has finished.
    readonly property int rerunGapMs: 100
    readonly property bool busy: root.clients.running || root.props.running || root.tags.running

    property Process clients: Process {
        command: ["hyprctl", "clients", "-j"]
        stdout: StdioCollector {
            waitForEnd: true
            onStreamFinished: root.probe(GlassTag.ownAddresses(text, Quickshell.processId))
        }
    }
    property Process props: Process {
        stdout: StdioCollector {
            waitForEnd: true
            onStreamFinished: root.apply(text)
        }
    }
    property Process tags: Process {}
    property Timer rerun: Timer {
        interval: root.rerunGapMs
        onTriggered: root.retag()
    }

    function retag() {
        if (root.busy) {
            root.rerun.restart()
            return
        }
        root.clients.running = true
    }
    function probe(addresses) {
        if (addresses.length === 0)
            return
        root.props.command = ["sh", "-c", GlassTag.PROBE, "sh"].concat(addresses)
        root.props.running = true
    }
    function apply(text) {
        var plan = GlassTag.plan(text, root.mode, root.usesDefault, root.tagged)
        root.tagged = plan.tagged
        if (plan.pairs.length === 0)
            return
        root.tags.command = ["sh", "-c", GlassTag.DISPATCH, "sh"].concat(plan.pairs)
        root.tags.running = true
    }
}
