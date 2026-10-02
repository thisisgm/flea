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
    // Recent, then PLACES (Home and the XDG dirs), then FAVORITES: the window rail's own order, so
    // the chooser reads the way the browser beside it does. Favourites sat above Home before, which
    // is why they read as loose rows rather than a labelled group; see the section header below.
    readonly property var placeEntries: (root.offerRecent ? root.recentRow : [])
        .concat(Places.homeEntries(root.home, root.dirsText, Icons.sidebarGlyphFor), Places.storedEntries(Flea.Favourites.records, root.home))
    // Directive 55, GM relaying a user's report: a dialog that cannot reach a disk or a share is a
    // dialog that makes you type the path, so the chooser draws the window's own NETWORK and DEVICES
    // groups from the same listings. Rows only: nothing here trashes, ejects or renames.
    readonly property var entries: root.placeEntries.concat(network.entries, devices.entries, phones.entries)

    implicitWidth: Math.min(parent.width / 3, Theme.space(172))
    readonly property alias focusItem: rail
    property bool awaitingNetwork: false
    property string awaitingDevice: ""

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
        Flea.ViewportScrollBar {
            parent: rail
            anchors { top: parent.top; right: parent.right }
            flickable: rail
        }
        // The window rail's own PLACES / FAVORITES / NETWORK / DEVICES headings, drawn by the view
        // itself so a header lives outside the row index space: the cursor, choose() and controls()
        // all keep counting rows and never a heading. The model is already grouped, so consecutive
        // rows of one group sit under one heading; ui/js/Picker.js names each group.
        section.property: "group"
        section.delegate: Item {
            id: heading
            required property string section
            readonly property string heLabel: Picker.groupHeading(section)
            width: rail.width
            // Zero for a group with no heading, Recent's among them, so its lone row sits at the top
            // with no empty band above it; otherwise the label's own box.
            height: heading.heLabel.length > 0 ? label.implicitHeight : 0
            Text {
                id: label
                x: Theme.spacing.rowPaddingX
                topPadding: Math.ceil(font.pixelSize * 0.15) + Theme.spacing.gap
                bottomPadding: Theme.spacing.gap
                text: heading.heLabel
                color: Theme.color.muted
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                font.letterSpacing: 1
                textFormat: Text.PlainText
            }
        }
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
        // The three groups below the places, routed the way ui/Sidebar.qml routes them.
        if (entry.group === "device" && entry.kind === "phone") {
            root.awaitingNetwork = true
            network.openShare(entry.uri, entry.mounted, entry.label)
            return
        }
        if (entry.group === "device") {
            // The row this dialog is waiting on, by device: any other mount's open is not its answer.
            root.awaitingDevice = entry.device
            devices.activate(index - root.placeEntries.length - network.entries.length)
            return
        }
        if (entry.group === "network") {
            root.awaitingNetwork = true
            network.activate(index - root.placeEntries.length)
            return
        }
        if (entry.path.indexOf("file://") === 0) {
            try {
                var path = decodeURIComponent(entry.path.substring(7))
                if (path.charAt(0) !== "/" || path.indexOf("\0") >= 0) throw new Error("not a local file URI")
                root.chosen(path)
            } catch (error) { root.picker.say("This favorite has an invalid local file URI.", true) }
        } else if (entry.path.indexOf("://") >= 0) {
            root.awaitingNetwork = true
            network.openShare(entry.path, false, entry.label, false)
        } else root.chosen(entry.path)
    }
    // Only the row the dialog asked for answers it, which the device says and a path alone cannot.
    function deviceOpened(path) {
        var awaited = root.awaitingDevice
        if (awaited.length === 0) return
        for (var i = 0; i < devices.entries.length; i++) {
            if (devices.entries[i].device === awaited && devices.entries[i].path === path) {
                root.awaitingDevice = ""
                root.chosen(path)
                return
            }
        }
    }

    function openChild(uri, label) { network.openChildShare(uri, label) }
    function retry(requestId, uri, label, password) { root.awaitingNetwork = true; network.saveLocation(uri, label, password, requestId) }
    function cancelNetwork(requestId) {
        root.awaitingNetwork = false
        network.cancelLocation(requestId)
    }
    Connections {
        target: root.picker
        function onPathChanged() { root.awaitingNetwork = false; root.awaitingDevice = "" }
        function onBackendUnavailableChanged() {
            if (root.picker.backendUnavailable) { root.awaitingNetwork = false; root.awaitingDevice = "" }
        }
    }
    function controls() {
        var out = []
        for (var i = 0; i < rail.count; i++) {
            var item = rail.itemAtIndex(i)
            if (item) out.push(root.picker.control(root.entries[i].label, item, !root.entries[i].error && !root.picker.submitting && !root.picker.backendUnavailable))
        }
        return out
    }
    // Standing, not loaded on demand: its own listing is what the NETWORK rows are, and the window
    // pays the same five-second rhythm for them.
    Flea.NetworkMounts {
        id: network
        onCompleted: function(requestId, uri, success, reason) { root.networkCompleted(requestId, uri, success, reason) }
        onOpened: function(path) { if (root.awaitingNetwork) root.chosen(path) }
        // A FUSE path that is a file: the chooser selects it, the way a favourite file resolves.
        onOpenFileRequested: function(path) { if (root.awaitingNetwork) root.chosen(path) }
        onMessage: function(text, error) { root.picker.say(text, error) }
        onRetryRequested: function(uri, label, password, reason, failed) {
            if (root.awaitingNetwork) root.picker.retryNetwork(uri, label, password, reason, failed)
        }
        onSharesListed: function(uri, label, names) {
            if (root.awaitingNetwork) root.picker.showShares(uri, label, names)
        }
    }

    Flea.DeviceMounts {
        id: devices
        onOpened: function (path) { root.deviceOpened(path) }
        // A message carries no device, so it cannot end a wait: only the awaited device's own open does.
        onMessage: function (text, error) { root.picker.say(text, error) }
    }

    // The phone rows the window draws, read off the same gio listing; their mount is the share leg.
    Flea.PhoneMounts {
        id: phones
        listingText: network.mountListing
        onMessage: function (text, error) { root.picker.say(text, error) }
    }
}
