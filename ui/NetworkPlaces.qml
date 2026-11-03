import QtQuick
import Quickshell
import Quickshell.Io
import "js/Format.js" as Format
import "js/Mounts.js" as Mounts
import "js/Places.js" as Places

// The saved places file, lifted out of ui/NetworkMounts.qml: the only writer of
// ~/.config/gtk-3.0/bookmarks in the Network Service. Both edits read the file itself before they
// rewrite it, never the rail's own text: another application replaces this file by temp-and-rename,
// so a body derived from a watched read can be a version behind and would drop the lines that
// writer had just added. A read that fails refuses the edit rather than writing a shorter file.
Item {
    id: root

    property var entries: []

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
        // A refused write is the only report this makes; without it a read-only gtk-3.0 answered
        // "NAS is forgotten" while the row came straight back from the unchanged file.
        property int writeError: FileViewError.Success
        onSaveFailed: function (error) { bookmarksWrite.writeError = error }
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

    // Rewrites the saved line's address in place, so a place whose host or path was wrong is the
    // same row afterwards and keeps its position in the file, which is the rail's own order.
    function replace(oldUri, newUri, name) {
        bookmarksWrite.readError = FileViewError.Success
        bookmarksWrite.reload()
        bookmarksWrite.waitForJob()
        if (bookmarksWrite.readError !== FileViewError.Success
                && bookmarksWrite.readError !== FileViewError.FileNotFound) {
            root.message("Saved places could not be read, so the new address was not saved.", true)
            return
        }
        root.write(Places.replace(bookmarksWrite.text(), oldUri, newUri, name))
    }

    // Pins a share the browser opened: appends the saved line, the mirror of forget()'s removal,
    // so a share found by browsing stays on the rail past its unmount. The file, not the cached
    // text, for rename()'s own reason above, and an empty old address appends rather than rewrites.
    // Returns whether the write landed: a caller that toasts success on the return is lying only
    // when the file itself refused, and this arm has already said why on its own message.
    function add(uri, name) {
        bookmarksWrite.readError = FileViewError.Success
        bookmarksWrite.reload()
        bookmarksWrite.waitForJob()
        if (bookmarksWrite.readError !== FileViewError.Success
                && bookmarksWrite.readError !== FileViewError.FileNotFound) {
            root.message("Saved places could not be read, so the share was not saved.", true)
            return false
        }
        return root.write(Places.replace(bookmarksWrite.text(), "", uri, name))
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
        // The file, not the cached text, for rename()'s own reason one function up: an external
        // writer replaces this file by temp-and-rename, and rewriting a stale body would drop the
        // bookmarks that writer had just added.
        bookmarksWrite.readError = FileViewError.Success
        bookmarksWrite.reload()
        bookmarksWrite.waitForJob()
        if (bookmarksWrite.readError !== FileViewError.Success
                && bookmarksWrite.readError !== FileViewError.FileNotFound) {
            root.message("Saved places could not be read, so nothing was forgotten.", true)
            return
        }
        var current = bookmarksWrite.text()
        var next = Mounts.removeBookmark(current, uri)
        // Remove is offered on every share row (ui/js/Mounts.js "rowMenu"), and the text the rail was
        // built from is what says whether this one is saved at all; a live mount often is not. Only a
        // mounted row can reach this: an unmounted one is on the rail because that text has its line.
        if (next === current) {
            root.message(Format.fill(Mounts.WORDS.notSaved, entry.label), false)
            return
        }
        if (!root.write(next)) return
        root.message(entry.mounted === true
            ? Format.fill(Mounts.WORDS.forgottenStays, entry.label)
            : Format.fill(Mounts.WORDS.forgotten, entry.label), false)
    }

    // waitForJob() blocks until the write lands, the same fix AGENTS.md "A FileView write can race a
    // reload" applies to ui/NetworkDialog.qml's own write. A view that has never read drops an empty
    // setText() (AGENTS.md "An unread FileView drops an empty write"), so it is made to read first.
    function write(body) {
        if (!bookmarksWrite.loaded) {
            bookmarksWrite.reload()
            bookmarksWrite.waitForJob()
        }
        bookmarksWrite.writeError = FileViewError.Success
        bookmarksWrite.setText(body)
        bookmarksWrite.waitForJob()
        if (bookmarksWrite.writeError !== FileViewError.Success) {
            root.message("Saved places could not be written.", true)
            return false
        }
        root.wrote()
        return true
    }
}
