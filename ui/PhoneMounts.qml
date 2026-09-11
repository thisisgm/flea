import QtQuick
import Quickshell.Io
import "js/Phones.js" as Phones
import "js/Mounts.js" as Mounts

// The DEVICES group's phone rows, the third Service beside ui/DeviceMounts.qml (block devices over
// lsblk) and ui/NetworkMounts.qml (shares over gio). An Android phone or a camera is a gvfs volume
// with no block device behind it, so lsblk can never list one; only the volume monitors can, and
// ui/MountListing.qml's five second listing already walks them, so this Service reads that text
// through ui/NetworkMounts.qml rather than earning a second 500 ms gio walk of its own.
// It only lists and unmounts: mounting and opening ride ui/NetworkMounts.qml's openShare leg,
// because an mtp:// uri mounts, resolves its FUSE folder and opens exactly the way a share does.
Item {
    id: root

    property string listingText: ""
    property var entries: []

    signal message(string text, bool isError)
    // Raised when an unmount ends, so the owner re-polls the listing; the poll is what flips the row.
    signal released()

    property string _unmountLabel: ""

    onListingTextChanged: root.rebuild()

    function rebuild() {
        var out = Phones.parsePhones(root.listingText)
        // Same rule as the other two Services: an unchanged poll assigns nothing, see Mounts.sameEntries.
        if (!Mounts.sameEntries(root.entries, out))
            root.entries = out
    }

    // gio's -f is offered nowhere in this tree: forcing an unmount over an open transfer is how a
    // file manager loses somebody's photos, and a phone is the surface people copy photos from.
    function unmount(index) {
        var e = root.entries[index]
        if (!e || e.kind !== "phone" || !e.mounted || unmountProcess.running)
            return
        root._unmountLabel = e.label
        unmountProcess.command = ["gio", "mount", "-u", e.uri]
        unmountProcess.running = true
    }

    Process {
        id: unmountProcess
        onExited: function (exitCode) {
            if (exitCode !== 0)
                root.message("Could not unmount " + root._unmountLabel + "; a transfer may still be using it.", true)
            root.released()
        }
    }
}
