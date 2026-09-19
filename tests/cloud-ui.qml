import QtQuick
import Quickshell
import "ui" as Flea

ShellRoot {
    id: root
    property string scenario: Quickshell.env("FLEA_CLOUD_CASE")
    Flea.CloudStatus { id: monitor; path: "/A" }
    Timer { interval: 30; running: root.scenario === "stale"; onTriggered: monitor.path = "/B" }
    Timer { interval: 60; running: root.scenario === "stale"; onTriggered: monitor.path = "/A" }
    Timer {
        interval: root.scenario === "stale" ? 1400 : 6200
        running: true
        onTriggered: {
            var good = !monitor.collecting
            if (root.scenario === "stale") good = good && monitor.snapshot.queued === 2 && monitor.snapshot.path === "/A"
            else if (root.scenario === "recover") good = good && monitor.snapshot.state === "idle"
            else good = good && monitor.snapshot.state === "unavailable"
            console.log("cloud-ui", root.scenario, good ? "PASS" : "FAIL", JSON.stringify(monitor.snapshot))
            Qt.exit(good ? 0 : 1)
        }
    }
}
