import QtQuick
import Quickshell
import Quickshell.Io
import "js/Mounts.js" as Mounts
import "js/Places.js" as Places

// The saved places file, lifted out of ui/NetworkMounts.qml: the only writer of
// ~/.config/gtk-3.0/bookmarks in the Network Service. "forget" derives its body from
// "bookmarksText", the same text the rail was built from, so no removal can be older than the rail,
// and a body it cannot find the line in writes nothing at all. "rename" reads the file itself,
// because a body that grows needs to know the read behind it worked and the rail's text cannot say.
// Staleness of that text against this component's own last write is ui/Sidebar.qml
// "reloadBookmarks"'s job: it blocks, so "bookmarksText" already carries a write by the time the
// edit that caused it returns and a second "forget" cannot derive from the text before.
Item {
    id: root

    property var entries: []
    property string bookmarksText: ""

    signal message(string text, bool isError)
    // Fired once the write below has actually landed, so a caller's reload reads it, not stale
    // content. A caller whose reload does not block leaves the next edit deriving from the text
    // this one replaced, so ui/Sidebar.qml's handler waits for its own job before it returns.
    signal wrote()

    // A second FileView on the same path as ui/Sidebar.qml's own read-only watch, the identical
    // split ui/NetworkDialog.qml already uses to write this file without fighting that watch.
    FileView {
        id: bookmarksWrite
        path: Quickshell.env("HOME") + "/.config/gtk-3.0/bookmarks"
        printErrors: false
        // The only report a failed read makes: "loaded" stays true through one and waitForJob()
        // answers true for every job, both measured on quickshell 0.3.1.
        property int readError: FileViewError.Success
        onLoadFailed: function (error) { bookmarksWrite.readError = error }
    }

    // Rewrites uri's own label, or appends a bookmark for it if it was only ever a live mount;
    // either way this is what makes the rename survive a reboot.
    function rename(uri, name) {
        // The file rather than "bookmarksText", which is ui/Sidebar.qml's own FileView.text(): a read
        // that failed empties that, the rail still carries every live mount, and relabelling "" then
        // appends this one line to nothing and destroys every saved place in the file. Blocking, so
        // the body is the file and not what the view last held. AGENTS.md "A failed FileView read".
        bookmarksWrite.readError = FileViewError.Success
        bookmarksWrite.reload()
        bookmarksWrite.waitForJob()
        // An absent file is the one read that is legitimately empty, and the first place ever saved
        // on this box is written into one.
        if (bookmarksWrite.readError !== FileViewError.Success
                && bookmarksWrite.readError !== FileViewError.FileNotFound) {
            root.message("Saved places could not be read, so the new name was not saved.", true)
            return
        }
        root.write(Places.relabel(bookmarksWrite.text(), uri, name))
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

    // waitForJob() blocks until the write lands, the same fix AGENTS.md "A FileView write can race a
    // reload" applies to ui/NetworkDialog.qml's own write. A view that has never read drops an empty
    // setText() (AGENTS.md "An unread FileView drops an empty write"), so it is made to read first.
    function write(body) {
        if (!bookmarksWrite.loaded) {
            bookmarksWrite.reload()
            bookmarksWrite.waitForJob()
        }
        bookmarksWrite.setText(body)
        bookmarksWrite.waitForJob()
        root.wrote()
    }
}
