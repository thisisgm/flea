import QtQuick
import Quickshell
import Quickshell.Io
import "js/Mounts.js" as Mounts
import "js/Places.js" as Places

// The saved places file, lifted out of ui/NetworkMounts.qml whole so that file has room in the
// 0.1.4 composition: it is the only writer of ~/.config/gtk-3.0/bookmarks in the Network Service.
Item {
    id: root

    property var entries: []
    property string bookmarksText: ""

    signal message(string text, bool isError)
    // Fired once the write below has actually landed, so a caller's reload reads it, not stale content.
    signal wrote()

    // A second FileView on the same path as ui/Sidebar.qml's own read-only watch, the identical
    // split ui/NetworkDialog.qml already uses to write this file without fighting that watch.
    FileView {
        id: bookmarksWrite
        path: Quickshell.env("HOME") + "/.config/gtk-3.0/bookmarks"
        printErrors: false
    }

    // Rewrites uri's own label, or appends a bookmark for it if it was only ever a live mount;
    // either way this is what makes the rename survive a reboot. waitForJob() blocks until the
    // write actually lands, the same fix AGENTS.md "A FileView write can race a reload" applies
    // to NetworkDialog.qml's own bookmark write.
    function rename(uri, name) {
        var body = bookmarksWrite.text()
        bookmarksWrite.setText(Places.relabel(body, uri, name))
        bookmarksWrite.waitForJob()
        root.wrote()
    }

    // Forgets a saved place, the same blocking write rename() makes just above: a share that is
    // mounted right now stays on the rail as the live mount it is until something unmounts it, so
    // the bar names which of the four states the press landed in rather than guessing at one.
    function forget(uri) {
        if (String(uri || "").length === 0)
            return
        var row = Mounts.rowByKey(root.entries, uri)
        var entry = row >= 0 ? root.entries[row] : null
        var name = entry ? entry.label : uri
        var mounted = entry !== null && entry.mounted === true
        // Remove is offered on every share row (ui/js/Mounts.js "rowMenu"), and the text the rail was
        // built from is what says whether this one is saved at all; a live mount often is not.
        if (Mounts.removeBookmark(root.bookmarksText, uri) === root.bookmarksText) {
            root.message(mounted ? name + " is not a saved place, and stays on the rail until it is unmounted."
                                 : name + " is not a saved place.", false)
            return
        }
        var body = bookmarksWrite.text()
        var next = Mounts.removeBookmark(body, uri)
        // This FileView answers "" until it has read, and the rail's own text says the line is there,
        // so a body this removal changes nothing in is behind the file and would be written over it.
        if (next === body) {
            root.message("The saved places have not been read yet; try Remove again in a moment.", true)
            return
        }
        bookmarksWrite.setText(next)
        bookmarksWrite.waitForJob()
        root.wrote()
        root.message(mounted ? name + " is forgotten, and stays on the rail until it is unmounted."
                             : name + " is forgotten.", false)
    }
}
