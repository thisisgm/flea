import Quickshell
import Quickshell.Io
import QtQuick
import "." as Flea
import "js/Icons.js" as Icons
import "js/Picker.js" as Picker
import "js/Places.js" as Places
import "js/Keymap.js" as Keymap

// The chooser shares Flea's favourites and home locations without importing legacy GTK bookmarks.
Item {
    id: root

    property string home: ""
    required property var picker
    property string current: ""
    // The ink the picker draws every rule in, handed down by ui/picker.qml, which says why; the
    // window's own rule colour stands in so an unwired rail still draws a seam and never a black one.
    property color edge: Theme.color.surface

    signal chosen(string path)
    signal networkCompleted(string requestId, string uri, bool success, string reason)

    // SendPicker.html draws Recent above Home, and a save has no history to write into, so the one
    // mode that cannot use the location does not offer it.
    property bool offerRecent: true

    property string dirsText: ""
    // Recent is a location and not a path, so its row carries the token ui/js/Picker.js names; the
    // rail's own cursor and activation then work on it exactly as they do on a favourite.
    readonly property var recentRow: [{
        path: Picker.RECENT, label: Picker.RECENT_LABEL, group: "favorite", kind: "favorite", glyph: "history"
    }]
    readonly property var entries: (root.offerRecent ? root.recentRow : [])
        .concat(Places.storedEntries(Flea.Favourites.records, root.home), Places.homeEntries(root.home, root.dirsText, Icons.sidebarGlyphFor))

    implicitWidth: Math.min(parent.width / 3, Theme.space(172))
    readonly property alias focusItem: rail
    property bool awaitingNetwork: false

    Rectangle {
        anchors.fill: parent
        color: Theme.color.background
    }

    Rectangle {
        anchors.right: parent.right
        width: Theme.spacing.hairline
        height: parent.height
        color: root.edge
    }

    ListView {
        id: rail
        anchors.fill: parent
        anchors.topMargin: Theme.spacing.rowPaddingY
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        model: root.entries
        activeFocusOnTab: true
        currentIndex: 0
        onCurrentIndexChanged: positionViewAtIndex(currentIndex, ListView.Contain)
        Flea.FastScrollHandler { flickable: rail }
        Keys.onTabPressed: function(event) { root.picker.stepFocus(rail, (event.modifiers & Qt.ShiftModifier) !== 0) }
        Keys.onBacktabPressed: root.picker.stepFocus(rail, true)
        Keys.onPressed: function(event) {
            var action = Keymap.lookup(event.key, event.text, event.modifiers, "rail")
            event.accepted = true
            if (action === "cursorUp") rail.currentIndex = Math.max(0, rail.currentIndex - 1)
            else if (action === "cursorDown") rail.currentIndex = Math.min(root.entries.length - 1, rail.currentIndex + 1)
            else if (action === "open" || action === "pageForward" || event.key === Qt.Key_Space) root.choose(rail.currentIndex)
            else event.accepted = false
        }

        // index and modelData are required on ui/SidebarRow.qml itself, so the view fills them;
        // redeclaring them here left the delegate uninitialised and the rail drew nothing.
        delegate: Flea.SidebarRow {
            cursor: rail.activeFocus ? index === rail.currentIndex : modelData.path === root.current
            focused: rail.activeFocus
            onActivated: function (at) { root.choose(at) }
        }
    }

    FileView {
        id: userDirsFile
        path: (Quickshell.env("XDG_CONFIG_HOME") || root.home + "/.config") + "/user-dirs.dirs"
        printErrors: false
        onLoaded: root.dirsText = text()
        watchChanges: true
        onFileChanged: reload()
    }

    function choose(index) {
        if (root.picker.submitting || root.picker.backendUnavailable) return
        var entry = root.entries[index]
        if (!entry) return
        if (entry.error) { root.picker.say(entry.error, true); return }
        root.awaitingNetwork = false
        if (entry.path.indexOf("file://") === 0) {
            try {
                var path = decodeURIComponent(entry.path.substring(7))
                if (path.charAt(0) !== "/" || path.indexOf("\0") >= 0) throw new Error("not a local file URI")
                root.chosen(path)
            } catch (error) { root.picker.say("This favorite has an invalid local file URI.", true) }
        } else if (entry.path.indexOf("://") >= 0) {
            network.active = true
            root.awaitingNetwork = true
            network.item.openShare(entry.path, false, entry.label, false)
        } else root.chosen(entry.path)
    }
    function openChild(uri, label) { network.item.openChildShare(uri, label) }
    function retry(requestId, uri, label, password) { root.awaitingNetwork = true; network.item.saveLocation(uri, label, password, requestId) }
    function cancelNetwork(requestId) {
        root.awaitingNetwork = false
        if (network.item) network.item.cancelLocation(requestId)
    }
    Connections {
        target: root.picker
        function onPathChanged() { root.awaitingNetwork = false }
        function onBackendUnavailableChanged() { if (root.picker.backendUnavailable) root.awaitingNetwork = false }
    }
    function controls() {
        var out = []
        for (var i = 0; i < rail.count; i++) {
            var item = rail.itemAtIndex(i)
            if (item) out.push(root.picker.control(root.entries[i].label, item, !root.entries[i].error && !root.picker.submitting && !root.picker.backendUnavailable))
        }
        return out
    }
    Loader {
        id: network
        active: false
        sourceComponent: Component {
            Flea.NetworkMounts {
                onCompleted: function(requestId, uri, success, reason) { root.networkCompleted(requestId, uri, success, reason) }
                onOpened: function(path) { if (root.awaitingNetwork) root.chosen(path) }
                onMessage: function(text, error) { root.picker.say(text, error) }
                onRetryRequested: function(uri, label, password, reason, failed) {
                    if (root.awaitingNetwork) root.picker.retryNetwork(uri, label, password, reason, failed)
                }
                onSharesListed: function(uri, label, names) {
                    if (root.awaitingNetwork) root.picker.showShares(uri, label, names)
                }
            }
        }
    }
}
