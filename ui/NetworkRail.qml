import QtQuick

// One façade for the rail's four network concerns: persisted recents, optional discovery, GIO
// mounts, and argv-direct host actions. Sidebar renders entries and owns no process itself.
Item {
    id: root

    property string bookmarksText: ""
    readonly property alias entries: mounts.entries
    readonly property alias dropboxReady: mounts.dropboxReady
    readonly property alias tailnetState: discovery.tailnetState
    readonly property alias tailnetMessage: discovery.tailnetMessage
    readonly property alias lanState: discovery.lanState

    signal opened(string path)
    signal message(string text, bool isError)
    signal sharesListed(string baseUri, string baseLabel, var names)
    signal renamed()
    signal taildropRequested(string peerId)

    function activate(index) { mounts.activate(index) }
    function openShare(uri, mounted, label, mac) { mounts.openShare(uri, mounted, label, mac) }
    function unmount(index) { mounts.unmount(index) }
    function rename(uri, label) { mounts.rename(uri, label) }
    function copyAddress(entry) { actions.copyAddress(entry) }
    function openSsh(entry) { actions.openSsh(entry) }
    function wake(entry) { actions.wake(entry) }
    function rememberProfile(oldUri, uri, label, mac) { history.rememberProfile(oldUri, uri, label, mac) }

    function remove(index) {
        var entry = mounts.entries[index]
        if (entry && entry.kind === "recent") {
            history.remove(entry.uri)
            return
        }
        mounts.remove(index)
    }

    NetworkDiscovery {
        id: discovery
        onMessage: function (text, isError) { root.message(text, isError) }
    }

    NetworkHistory { id: history }

    NetworkActions {
        id: actions
        onMessage: function (text, isError) { root.message(text, isError) }
    }

    NetworkMounts {
        id: mounts
        bookmarksText: root.bookmarksText
        extraEntries: history.entries.concat(history.profileEntries).concat(discovery.entries)
        onOpened: function (path) { root.opened(path) }
        onMessage: function (text, isError) { root.message(text, isError) }
        onSharesListed: function (baseUri, baseLabel, names) { root.sharesListed(baseUri, baseLabel, names) }
        onRenamed: root.renamed()
        onConnectionSucceeded: function (uri, label, mac) { history.record(uri, label, mac) }
    }
}
