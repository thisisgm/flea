import QtQuick
import Quickshell
import Quickshell.Io
import "js/Mounts.js" as Mounts
import "js/Places.js" as Places

// The saved places file, lifted out of ui/NetworkMounts.qml: the only writer of
// ~/.config/gtk-3.0/bookmarks in the Network Service, and every body it writes is derived from
// "bookmarksText", the same text the rail was built from, so no write can be older than the rail.
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
    // either way this is what makes the rename survive a reboot.
    function rename(uri, name) {
        root.write(Places.relabel(root.bookmarksText, uri, name))
    }

    // Forgets a saved place. A share that is mounted right now stays on the rail as the live mount
    // it is until something unmounts it, so the bar names the state the press landed in.
    function forget(uri) {
        var row = Mounts.rowByKey(root.entries, uri)
        // ui/js/Mounts.js "release" resolved this key to a row before it called, and an empty key
        // matches nothing, so a row that has left the rail since is the only way this misses.
        if (row < 0)
            return
        var entry = root.entries[row]
        var next = Mounts.removeBookmark(root.bookmarksText, uri)
        // Remove is offered on every share row (ui/js/Mounts.js "rowMenu"), and the text the rail was
        // built from is what says whether this one is saved at all; a live mount often is not. Only a
        // mounted row can reach this: an unmounted one is on the rail because that text has its line.
        if (next === root.bookmarksText) {
            root.message(entry.label + " is not a saved place, and stays on the rail until it is unmounted.", false)
            return
        }
        root.write(next)
        root.message(entry.mounted === true
            ? entry.label + " is forgotten, and stays on the rail until it is unmounted."
            : entry.label + " is forgotten.", false)
    }

    // waitForJob() blocks until the write actually lands, the same fix AGENTS.md "A FileView write
    // can race a reload" applies to ui/NetworkDialog.qml's own bookmark write.
    function write(body) {
        bookmarksWrite.setText(body)
        bookmarksWrite.waitForJob()
        root.wrote()
    }
}
