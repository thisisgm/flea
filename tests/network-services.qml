//@ pragma ShellId flea-network-services-test

import QtQuick
import Quickshell

ShellRoot {
    id: root

    readonly property string mode: Quickshell.env("FLEA_SERVICE_MODE") || "discover"
    property bool started: false

    NetworkDiscovery {
        id: discovery
        visible: false
    }

    NetworkHistory {
        id: history
        visible: false
    }

    NetworkActions {
        id: actions
        visible: false
        onMessage: function (text, isError) {
            console.log("ACTION " + (isError ? "error " : "ok ") + text)
            root.finish()
        }
    }

    Loader {
        id: mountsLoader
        active: root.mode === "list-timeout"
        sourceComponent: Component {
            NetworkMounts {
                onMessage: function (text, isError) {
                    console.log("MOUNTS " + (isError ? "error " : "ok ") + text)
                    root.finish()
                }
            }
        }
    }

    function finish() {
        Quickshell.execDetached(["kill", String(Quickshell.processId)])
    }

    Timer {
        interval: 50
        running: true
        repeat: true
        onTriggered: {
            if (root.mode === "discover") {
                if (discovery.tailnetState === "unknown" || discovery.lanState === "unknown") return
                console.log("DISCOVERY " + discovery.tailnetState + " " + discovery.lanState
                            + " peers=" + discovery.tailnetPeers.length + " lan=" + discovery.lanEntries.length
                            + " message=" + discovery.tailnetMessage)
                root.finish()
                stop()
                return
            }
            if (root.started) return
            root.started = true
            if (root.mode === "history") {
                history.record("sftp://pi:secret@box/home?token=bad", "Box\nInjected", 0)
                history.rememberProfile("", "sftp://lan/", "Sleeping box", "AA-BB-CC-DD-EE-FF")
                console.log("HISTORY recent=" + history.recent.length + " profiles=" + history.profiles.length)
                root.finish()
            } else if (root.mode === "copy") {
                actions.copyAddress({ address: "host; touch should-not-exist" })
            } else if (root.mode === "wake") {
                actions.wake({ label: "box", mac: "aa:bb:cc:dd:ee:ff" })
            } else if (root.mode === "list-timeout") {
                mountsLoader.item.listShares("smb://host/")
            }
            stop()
        }
    }

    Timer {
        interval: 14000
        running: true
        onTriggered: {
            console.log("HARNESS timeout")
            root.finish()
        }
    }
}
