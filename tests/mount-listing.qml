//@ pragma ShellId flea-mount-listing-test

import QtQuick
import Quickshell
import Quickshell.Io

// Two listings against one stub gio: the first reports a share, the second reports none. The empty
// one has to arrive empty, and nothing the first one collected may still be standing as the exit
// handler's fallback while the second is under way. Measured on Quickshell 0.3.1, StdioCollector
// raises streamFinished before Process raises exited, even with a child still holding the pipe
// open, so the second half of that is the order-independent check and not a staged race.
ShellRoot {
    id: root

    property int seen: 0
    property string first: ""
    property string carried: "unread"
    property bool finished: false

    function finish(message) {
        if (root.finished)
            return
        root.finished = true
        console.log(message)
        Quickshell.execDetached(["kill", String(Quickshell.processId)])
    }

    MountListing {
        id: listing

        onListed: {
            root.seen += 1
            if (root.seen === 1) {
                root.first = listing.text
                // Out of the exited handler, the poll timer's own way in rather than the
                // mid-listing one, so this is the ordinary populated-to-empty sequence.
                repoll.start()
                return
            }
            var ok = root.first.indexOf("smb://fixture/share/") >= 0
                && listing.text === ""
                && root.carried === ""
            root.finish("MOUNT_LISTING " + (ok ? "populated=share empty=none carried=none" : "FAIL")
                        + " first=" + JSON.stringify(root.first)
                        + " second=" + JSON.stringify(listing.text)
                        + " carried=" + JSON.stringify(root.carried))
        }
    }

    Timer {
        id: repoll
        interval: 50
        repeat: false
        onTriggered: {
            listing.poll()
            root.carried = listing._output
        }
    }

    Timer {
        interval: 5000
        running: true
        repeat: false
        onTriggered: root.finish("MOUNT_LISTING FAIL timeout seen=" + root.seen)
    }
}
