import QtQuick
import Quickshell
import Quickshell.Io
import "js/Recents.js" as Recents

// Successful opens only, in a file that carries no credentials. GTK bookmarks remain Favorites;
// this bounded list is recency, not a second source of truth for what the operator pinned.
Item {
    id: root

    property var recent: []
    property var profiles: []
    readonly property var entries: Recents.entries(root.recent)
    readonly property var profileEntries: Recents.profileEntries(root.profiles)

    function write() {
        historyFile.setText(Recents.serializeState(root.recent, root.profiles))
        historyFile.waitForJob()
    }

    function record(uri, label, mac) {
        root.recent = Recents.record(root.recent, uri, label, Date.now(), mac)
        root.write()
    }

    function rememberProfile(oldUri, uri, label, mac) {
        root.profiles = Recents.rememberProfile(root.profiles, oldUri, uri, label, mac)
        root.write()
    }

    function remove(uri) {
        root.recent = Recents.remove(root.recent, uri)
        root.profiles = Recents.remove(root.profiles, uri)
        root.write()
    }

    FileView {
        id: historyFile
        path: Quickshell.env("HOME") + "/.config/flea-network-recents.json"
        atomicWrites: true
        watchChanges: true
        printErrors: false
        onLoaded: {
            var state = Recents.parseState(text())
            root.recent = state.recent
            root.profiles = state.profiles
        }
        onFileChanged: reload()
        onLoadFailed: {
            root.recent = []
            root.profiles = []
        }
    }
}
