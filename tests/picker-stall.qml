import QtQuick
import Quickshell
import "flea" as Flea

ShellRoot {
    id: root
    property string scenario: Quickshell.env("FLEA_PICKER_CASE")
    property bool stalled: false
    property bool localReady: false
    property bool paged: false
    property bool validationBlocked: false
    property bool stopping: false
    property int failures: 0
    property double began: Date.now()
    function stop() { stopping = true; lifecycle.quit() }
    Flea.PickerListing {
        id: listing
        onFailed: function(reason) {
            if (root.scenario.indexOf("missing") >= 0) root.stop()
            else { console.log(reason); root.failures++; root.stop() }
        }
        onMessage: function(message) {
            if (message.t === "blocked") {
                root.stalled = true
                next.start()
            } else if (message.t === "listed") {
                if (message.path !== "/local") root.failures++
                root.localReady = true
                listing.window(1, 1)
            } else if (message.t === "rows" && message.start === 1) {
                root.paged = message.rows[0].n === "next.txt"
                root.stop()
            }
        }
    }
    Flea.Backend {
        id: checks
        pickerOnly: true
        onPickerResult: function(message) { if (message.op === "blocked") root.validationBlocked = true }
        onFailed: function(where, input, message, mode) {
            if (root.scenario.indexOf("missing") < 0) { console.log(message); root.failures++ }
        }
    }
    Flea.PickerLifecycle {
        id: lifecycle
        checks: checks
        listing: listing
        onStopped: {
            var good = root.stopping && !root.failures && Date.now() - root.began < 2000
            if (root.scenario.indexOf("missing") < 0 && root.scenario.indexOf("early") < 0)
                good = good && root.stalled && root.validationBlocked
            if (root.scenario === "navigate") good = good && root.localReady && root.paged
            console.log("picker-stall", root.scenario, good ? "PASS" : "FAIL", Date.now() - root.began)
            Qt.exit(good ? 0 : 1)
        }
    }
    Timer {
        id: next
        interval: 100
        onTriggered: {
            if (root.scenario === "navigate") {
                listing.request({c: "list", path: "/superseded", first: 1})
                listing.request({c: "list", path: "/local", first: 1})
            } else root.stop()
        }
    }
    Timer {
        id: failedStartOrder
        interval: 100
        onTriggered: { root.stopping = true; listing.quit(); checks.testFailedStartDuringQuit() }
    }
    Timer { interval: 3000; running: true; onTriggered: Qt.exit(1) }
    Component.onCompleted: {
        if (scenario === "missing-order") { failedStartOrder.start(); return }
        listing.request({c: "list", path: "/stalled", first: 1})
        checks.send({c: "transfer", rows: [0], dest: "/not-allowed"})
        checks.send({c: "picker", op: "validate", id: 1})
        if (scenario.indexOf("early") === 0) stop()
    }
}
