import QtQuick
import Quickshell

// Called only after the portal reply has been saved (or the save has failed). Both processes are
// owned by this chooser; neither can write selected files, so a stalled read needs no write drain.
Item {
    id: root
    required property var checks
    required property var listing
    signal stopped()
    property bool checksStopped: false
    property bool listingStopped: false
    property bool finished: false
    function quit() { listing.quit(); checks.quit() }
    function finish() {
        if (finished || !checksStopped || !listingStopped) return
        finished = true
        stopped()
    }
    Connections {
        target: root.checks
        function onQuitReady() { root.checksStopped = true; root.finish() }
    }
    Connections {
        target: root.listing
        function onQuitReady() { root.listingStopped = true; root.finish() }
    }
}
