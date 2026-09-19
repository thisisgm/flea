import QtQuick
import Quickshell
import "ui" as Flea
ShellRoot {
    id: root
    property string scenario: Quickshell.env("FLEA_COPY_CASE")
    property var states: []
    Flea.CloudUploadDialog { id: dialog; width: 640; height: 600; Component.onCompleted: if (root.scenario === "dialog") open("/synthetic", null) }
    Timer { interval: 200; running: root.scenario === "dialog"; onTriggered: dialog.start() }
    Flea.CloudUploadJob {
        id: job
        onSnapshotChanged: root.states = root.states.concat([snapshot.state])
        Component.onCompleted: if (root.scenario !== "dialog") loadTargets()
        onLoadingChanged: if (!loading) start("test", "", "/synthetic")
    }
    Timer { interval: 200; running: ["cancel", "cancelrace", "terminalcancel"].indexOf(root.scenario) >= 0; onTriggered: job.cancel() }
    Timer {
        interval: root.scenario === "missing" ? 11000 : 1000; running: true
        onTriggered: {
            var expected = root.scenario === "cancel" ? "cancelled" : ["missing", "badexit"].indexOf(root.scenario) >= 0 ? "error" : "done"
            var good = root.scenario === "dialog" ? dialog.opened && dialog.state.state === "done" : !job.busy && !job.cancelling && job.snapshot.state === expected
            if (root.scenario === "success") good = good && ["preparing", "uploading", "verifying", "done"].every(function(s) { return root.states.indexOf(s) >= 0 })
            console.log("cloud-copy-ui", root.scenario, good ? "PASS" : "FAIL", JSON.stringify(root.scenario === "dialog" ? dialog.state : job.snapshot))
            Qt.exit(good ? 0 : 1)
        }
    }
}
