//@ pragma ShellId flea-network-alias-test
import QtQuick
import Quickshell
import Quickshell.Io

// The first real bridge check waits at a file barrier while a second open completes. Its local
// path needs no bridge, so it can replace _pending* without releasing the first waiter. Then a
// real gio listing reports conflicting live names, followed by favourite and GTK bookmark renames.
ShellRoot {
    id: root
    readonly property string fixture: Quickshell.env("FLEA_TEST_ALIAS_ROOT")
    readonly property string firstPath: Quickshell.env("XDG_RUNTIME_DIR") + "/gvfs/dav:host=first.example/space"
    readonly property string secondPath: root.fixture + "/second"
    readonly property string firstUri: "davs://first.example/space"
    readonly property string secondUri: "davs://second.example/vault"
    property bool started: false
    property bool finished: false
    property int phase: 0

    // The same binding shape as Pane.pathAlias, checked after each asynchronous delivery.
    Item { id: firstPane; readonly property var pathAlias: network.pathAlias(root.firstPath + "/Documents") }
    Item { id: secondPane; readonly property var pathAlias: network.pathAlias(root.secondPath) }

    function finish(text) {
        if (root.finished) return
        root.finished = true
        console.log("NETWORK_ALIAS " + text)
        Quickshell.execDetached(["kill", String(Quickshell.processId)])
    }
    function expect(ok, why) {
        if (!ok) root.finish("FAIL " + why)
        return ok
    }
    function aliasIs(path, label, key) {
        var alias = network.pathAlias(path)
        return alias && alias.path === path && alias.label === label && alias.key === key
    }
    function polled() {
        if (root.phase !== 3 || root.finished) return
        var live = network.entries.filter(function (entry) { return entry.uri === root.firstUri + "/" })
        if (live.length === 0) return
        if (!expect(live[0].label === "Generated first" && live[0].mounted,
                    "the conflicting live row never reached rebuild")) return
        if (!expect(aliasIs(root.firstPath, "Cloud — Personal", root.firstUri)
                    && firstPane.pathAlias.label === "Cloud — Personal", "live poll replaced the favourite name")) return
        root.phase = 4
        network.savedFavourites = [{ path: root.firstUri + "/", label: "Private files" }]
        network.bookmarksText = root.secondUri + "/ Legacy renamed\n"
        Qt.callLater(root.renamed)
    }
    function renamed() {
        if (root.finished) return
        if (!expect(aliasIs(root.firstPath, "Private files", root.firstUri)
                    && firstPane.pathAlias.label === "Private files", "favourite rename did not update the pane")) return
        if (!expect(aliasIs(root.secondPath, "Legacy renamed", root.secondUri)
                    && secondPane.pathAlias.label === "Legacy renamed", "GTK rename did not update the other pane")) return
        var held = network.pathAliases
        network.rebuild()
        if (!expect(network.pathAliases === held, "unchanged rebuild replaced the alias array")) return
        // Reopening through the live rail must not spend the saved name either.
        root.phase = 5
        network.openShare(root.secondUri, true, "Generated second", false, { origin: secondPane })
    }

    NetworkMounts {
        id: network
        savedFavourites: [{ path: root.firstUri, label: "Cloud — Personal" }]
        bookmarksText: root.secondUri + " Legacy vault\n"
        onEntriesChanged: Qt.callLater(root.polled)
        onRetryRequested: function(uri, label, password, reason, failedConnect) {
            root.finish("FAIL retry=" + uri + " reason=" + reason)
        }
        onOpened: function(path, origin) {
            if (root.finished) return
            if (root.phase === 1 && path === root.secondPath) {
                if (!root.expect(origin === secondPane && root.aliasIs(path, "Legacy vault", root.secondUri),
                                 "second open lost its identity")) return
                root.phase = 2
                releaseCheck.running = true
            } else if (root.phase === 2 && path === root.firstPath) {
                if (!root.expect(origin === firstPane && root.aliasIs(path, "Cloud — Personal", root.firstUri),
                                 "delayed first reply took the second request's identity")) return
                if (!root.expect(root.aliasIs(root.secondPath, "Legacy vault", root.secondUri),
                                 "first reply evicted the second alias")) return
                root.phase = 3
                writeListing.running = true
            } else if (root.phase === 5 && path === root.secondPath) {
                if (root.expect(root.aliasIs(path, "Legacy renamed", root.secondUri), "live reopen replaced the saved name"))
                    root.finish("PASS delayed=isolated poll=saved renames=reactive reopen=saved")
            } else {
                root.finish("FAIL unexpected open phase=" + root.phase + " path=" + path)
            }
        }
    }

    Process {
        id: waitForCheck
        running: root.started
        command: ["timeout", "7", "sh", "-c", "while [ ! -e \"$1\" ]; do sleep 0.01; done", "sh", root.fixture + "/check-started"]
        onExited: function(code) {
            if (!root.expect(code === 0 && network.bridge && network.bridge.flow.waiter
                             && network.bridge.flow.waiter.path === root.firstPath && root.phase === 0,
                             "first bridge check did not wait at the barrier: exit=" + code + " phase=" + root.phase)) return
            root.phase = 1
            network.openShare(root.secondUri, true, "Legacy vault", false, { origin: secondPane })
        }
    }
    Process {
        id: releaseCheck
        command: ["touch", root.fixture + "/release-check"]
        onExited: function(code) { root.expect(code === 0, "could not release bridge check") }
    }
    Process {
        id: writeListing
        command: ["sh", "-c", "printf '%s' \"$2\" > \"$1\"", "sh", root.fixture + "/mounts",
            "Mount(0): Generated first -> " + root.firstUri + "/\n  Type: GDaemonMount\n"
            + "Mount(1): Generated second -> " + root.secondUri + "/\n  Type: GDaemonMount\n"]
        onExited: function(code) {
            if (root.expect(code === 0, "could not publish live mount listing")) network.pollMounts()
        }
    }
    Timer {
        interval: 100
        running: true
        onTriggered: {
            network.railArrived()
            network.openShare(root.firstUri, true, "Cloud — Personal", false, { origin: firstPane })
            root.started = true
        }
    }
    Timer {
        interval: 8000
        running: true
        onTriggered: root.finish("FAIL timeout phase=" + root.phase)
    }
}
