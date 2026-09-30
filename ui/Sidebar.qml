import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import "." as Flea
import "js/Icons.js" as Icons
import "js/Eject.js" as Eject
import "js/Mounts.js" as Mounts
import "js/Places.js" as Places
import "js/PlaceMenu.js" as PlaceMenu
import "js/RailMenu.js" as RailMenu

// Places, Favorites, Network and Devices share one flat cursor in visual order.
Item {
    id: root

    property bool focused: false
    property var backend: null
    property Item navigationPane: null
    property bool trashActive: false
    // Set by Enter on a row that mounts first, so the open that lands afterwards takes focus into the folder.
    property bool focusOnOpen: false
    // ui/Pane.qml's one ui/ContextMenu.qml, handed in rather than built here: a second instance in
    // this tree took the keyboard away from the list, see ui/SidebarRow.qml's own note.
    property var menu: null
    property int cursorIndex: 0
    property var cursorEntries: []
    readonly property var placesState: ViewState.state.places || ({})
    readonly property var userFavouriteEntries: Places.storedEntries(Favourites.records, Quickshell.env("HOME")).map(function (entry) {
        entry.error = entry.error || Favourites.statuses[entry.favouriteIndex] || ""
        return entry
    })
    property var homeEntries: []
    readonly property var placesEntries: root.homeEntries.concat(root.trashEntries, root.userFavouriteEntries)
    readonly property int trashCount: trashMonitor.count
    signal trashChanged()
    function refreshTrash() { trashMonitor.refresh() }
    TrashMonitor {
        id: trashMonitor
        watching: root.trashActive || root.placesState.showTrash !== false
        onChanged: root.trashChanged()
        // A background poll the operator never asked for must not take the status bar: the count is
        // read for the rail Trash menu even with the badge off, so only a shown count reports it.
        onFailed: function(text) { if (root.trashActive || root.placesState.trashCount === true) root.message(text, true) }
    }
    readonly property var trashEntries: root.placesState.showTrash === false ? []
        : [{ label: "Trash", path: "trash:///", group: "trash", kind: "trash", glyph: "trash", count: root.trashCount }]
    signal trashRequested()

    // The saved place an Edit is rewriting, "" when none is; ui/js/RailMenu.js editPlace sets it.
    property string editingPlace: ""
    // And the request that Edit's own attempt went out with, so no other mount answers for it.
    property string editingRequest: ""
    // The window-long network host this rail renders and routes through, injected by
    // ui/PaneRail.qml: the service outlives the rail Loader below, so hiding the rail mid-mount
    // kills no mount, no bridge wait and no dialog answer. Null until the first open builds it.
    property var service: null
    readonly property var networkEntries: root.placesState.showNetwork === false || !root.railGate.showNetwork || !root.service ? [] : root.service.entries
    // The poll rebinds its delegates in place, so a rename left standing would edit a different share.
    onNetworkEntriesChanged: root.cancelRename()
    // Phones ride the DEVICES group behind the block devices: a plugged phone is a device to the person holding it, whatever transport gvfs reaches it over.
    readonly property var deviceEntries: root.placesState.showDevices === false || !root.railGate.showDevices ? [] : devices.entries.concat(phones.entries)
    readonly property var entries: root.placesEntries.concat(root.networkEntries, root.deviceEntries)

    // The rail lands in one step by gating the entries themselves, so cursor, IPC and menus match only drawn rows.
    readonly property int railSettleMs: 800
    property bool railDeadlineElapsed: false
    property bool bookmarksReady: false
    readonly property var railGate: Mounts.railGroupsReady(root.bookmarksReady && root.service !== null && root.service.listingAnswered && root.service.dropboxAnswered, devices.firstAnswered && phones.firstDone, root.railDeadlineElapsed ? root.railSettleMs : 0, root.railSettleMs)
    Timer { interval: root.railSettleMs; running: true; repeat: false; onTriggered: root.railDeadlineElapsed = true }

    // Reconcile only the aggregate; evaluating entries from a group's change handler re-enters its binding.
    onEntriesChanged: {
        var next = Places.railCursorAfter(root.cursorEntries, root.entries, root.cursorIndex)
        if (root.renamingIndex >= 0
            && Places.railCursorAfter(root.cursorEntries, root.entries, root.renamingIndex) !== root.renamingIndex)
            root.cancelRename()
        root.cursorEntries = root.entries
        root.cursorIndex = next
        availability.restart()
    }

    signal opened(string path)
    signal addRequested()
    // The rail's Edit row asks the window to open the dialog over the saved place.
    signal editRequested(string uri, string label, string password, string reason, bool failedConnect, var origin)
    signal message(string text, bool isError)
    signal forgetMessage(string text)

    // The entry index mid-rename, or -1; ui/SidebarRow.qml swaps its Text for a field on it, and ui/Pane.qml holds its keys off while it stands.
    property int renamingIndex: -1
    // Fires once, on both commit and cancel, so ui/Pane.qml has one place to hand focus back.
    signal renameFinished()

    // Sized in characters, because a monospace makes that exact where a pixel constant would be an accident.
    readonly property int widthChars: 18
    // The mark and its gap count too, because ui/SidebarRow.qml draws them before the label: without them an 18-character entry elided at 15.
    implicitWidth: Places.sidebarWidth(root.placesState.sidebarWidth)

    TextMetrics {
        id: metrics
        font.family: Theme.font.family
        font.pixelSize: Theme.font.body
        text: "0"
    }

    FileView {
        id: userDirsFile
        path: Quickshell.env("HOME") + "/.config/user-dirs.dirs"
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onLoaded: root.rebuild()
        onLoadFailed: root.rebuild()
    }

    FileView {
        id: bookmarksFile
        path: Quickshell.env("HOME") + "/.config/gtk-3.0/bookmarks"
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onLoaded: { root.bookmarksReady = true; root.rebuild(); root.pushBookmarks() }
        onLoadFailed: { root.bookmarksReady = true; root.rebuild(); root.pushBookmarks() }
    }

    // The host keeps its own copy of this text: it outlives this rail, so a rail unload
    // mid-mount leaves the mount's labels and dedup exactly where they were. Pushed only
    // after the FileView loaded, so a reveal never blanks the host with an unread "".
    function pushBookmarks() {
        if (!root.bookmarksReady || !root.service) return
        root.service.bookmarksText = bookmarksFile.text()
    }

    // The window-long poll runs while a rail is loaded; arrival waits until the pane's overlay parent is bound.
    Component.onCompleted: Qt.callLater(root.meetHost)
    function meetHost() { root.arrive() } // never builds the host: ui/WindowBody.qml orders it after the first rows
    // Arrival counts whenever the service appears, so a host built after this rail still starts the poll.
    property bool arrived: false
    function arrive() { if (root.arrived || !root.service) return; root.arrived = true; root.service.railArrived() }
    onServiceChanged: { root.arrive(); root.pushBookmarks() }
    // Departure hands the timer back to a flight, if any, and the last listing stands while it is off.
    Component.onDestruction: { if (root.service && root.arrived) root.service.railLeft() }

    // The context menu's gate for the two Dropbox rows, so the pane never reaches into the rail.
    readonly property bool dropboxReady: root.service !== null && root.service.dropboxReady
    readonly property var providerService: root.service

    DeviceMounts {
        id: devices
        showUnmounted: root.placesState.showUnmounted !== false
        onOpened: function (path) { root.opened(path) }
        onMessage: function (text, isError) { root.message(text, isError) }
        onForgetMessage: function (text) { root.forgetMessage(text) }
    }

    // Lists and unmounts only: activate() below routes a phone's mount-and-open through the same openShare leg a share rides.
    PhoneMounts {
        id: phones
        listingText: root.service !== null ? root.service.mountListing : ""
        listingAnswered: root.service !== null && root.service.listingAnswered
        onMessage: function (text, isError) { root.message(text, isError) }
        onReleased: if (root.service) root.service.pollMounts()
    }

    // The mount service lives window-long in ui/WindowBody.qml's network host, injected through
    // ui/PaneRail.qml: every call below reaches the same instance whether this rail is shown or
    // has been unloaded mid-mount, and the answers land through ui/PaneRail.qml's own
    // connections rather than through this rail.

    // Home is in neither file, so it is prepended; the merge and its first-position-wins rule are Places.favorites'.
    onPlacesStateChanged: root.rebuild()
    Connections {
        target: Favourites
        function onFailed(message) { root.message(message, true) }
    }

    function rebuild() {
        var home = Quickshell.env("HOME")
        root.homeEntries = root.placesState.showHome === false ? [] : Places.homeEntries(home, userDirsFile.text(), Icons.sidebarGlyphFor)
    }

    // NetworkDialog's saved() drives this reload because a watch set up before its parent directory existed never fires, and it blocks because "forget" derives its body from this text: measured here, an asynchronous reload put a removed line back.
    function reloadBookmarks() {
        bookmarksFile.reload()
        bookmarksFile.waitForJob()
    }

    function networkResult() {
        return root.service !== null ? root.service.result : ""
    }

    // The eject mark's one click: Eject for a drive, Unmount for a share. The release
    // row itself is named rather than rows[0], which an unmounted-switch volume's Open
    // would win; Eject.releaseAction and Mounts.railKey are what Ctrl+E reads too.
    function ejectRow(index) {
        var entry = root.entries[index]
        if (!entry)
            return
        var action = Eject.releaseAction(entry)
        if (action.length === 0)
            return
        root.releaseChosen(action, Mounts.railKey(entry))
    }

    // Right click is the whole affordance, and which rows offer what is ui/js/Mounts.js "rowMenu"'s: a row with nothing to offer opens no menu at all.
    function openRailMenu(index, scenePosition) {
        root.cancelRename()
        var entry = root.entries[index]
        if (!entry || !root.menu) {
            return
        }
        root.cursorIndex = index
        RailMenu.railMenuFor(root, entry, scenePosition, ViewState.menuHidden)
    }

    // The keyboard's entrance to the same menu: ui/js/Mounts.js "raiseMenu" has already asked whether the row releases anything, so this only turns the cursor into a point.
    function openCursorMenu() {
        var row = root.railItemFor(root.cursorIndex)
        if (!row)
            return
        root.openRailMenu(root.cursorIndex, row.mapToItem(null, Style.spacing.rowPaddingX, row.height))
    }

    // A chosen row arrives with its key rather than its position, and RailMenu.release names the row.
    function releaseChosen(action, key) {
        if (key === "trash") return
        // A place and a favourite are both a path, and ui/js/PlaceMenu.js owns what their rows do.
        if (key.indexOf("place:") === 0 || key.indexOf("favourite:") === 0) {
            PlaceMenu.perform(action, key, root, Favourites)
            return
        }
        RailMenu.release(action, key, devices, root.service, root)
    }

    Connections {
        target: root.menu
        function onRailChosen(action, key) { root.releaseChosen(action, key) }
    }

    // The host exists from creation, so a row pressed before the rail arrives still answers.
    function networkHost() {
        if (root.service) return root.service
        if (root.navigationPane) return root.navigationPane.ensureNetworkService()
        return null
    }

    // A favourite's path is already real and opens directly; a share or a volume may need its Service.
    function openFavourite(index) {
        var entry = root.userFavouriteEntries[index]
        if (!entry) return
        var error = Places.recordError(entry.original)
        if (error) { root.message("Could not open " + entry.label + " · " + error, true); return }
        if (entry.path.indexOf("://") >= 0 && entry.path.indexOf("file://") !== 0) {
            var host = root.networkHost()
            if (host) host.openChildShare(entry.path, entry.label, root.navigationPane)
        } else {
            root.opened(entry.path.indexOf("file://") === 0 ? Mounts.decodePath(entry.path.substring(7)) : entry.path)
        }
    }

    function activate(index) {
        root.cancelRename()
        root.cursorIndex = index
        var entry = root.entries[index]
        if (!entry) return
        if (entry.kind === "favourite") { root.openFavourite(entry.favouriteIndex); return }
        if (entry.kind === "home") { root.opened(entry.path); return }
        if (entry.kind === "trash") { root.trashRequested(); return }
        var rest = index - root.placesEntries.length
        if (rest < root.networkEntries.length) root.service.activate(rest, root.navigationPane)
        // A phone mounts, resolves and opens the way a share does.
        else if (entry.kind === "phone") { var phoneHost = root.networkHost(); if (phoneHost) phoneHost.openShare(entry.uri, entry.mounted, entry.label, false, { origin: root.navigationPane }) }
        else devices.activate(rest - root.networkEntries.length)
    }

    // RailMenu.release hands the phone action back here, because the phone Service is this rail's own child.
    function releasePhone(key) { phones.release(key) }
    // Its Mount and Open rows are the row's own activation, resolved by key because the poll renumbers.
    function openPhone(key) {
        var e = phones.entries[Mounts.rowByKey(phones.entries, key)]
        var host = e ? root.networkHost() : null
        if (e && host) host.openShare(e.uri, e.mounted, e.label, false, { origin: root.navigationPane })
    }

    // Network only: neither a favourite nor a device has a bookmark line of its own shape for
    // Places.relabel to find, and a volume's label lives on the filesystem, not in a rail file.
    function startRename(index) {
        if (index < root.placesEntries.length)
            return
        if (index >= root.placesEntries.length + root.networkEntries.length)
            return
        root.renamingIndex = index
    }

    // The rail's own answer to ui/Pane.qml's renameEditor: the live editor row, or null.
    function renameEditor() {
        if (root.renamingIndex < 0)
            return null
        var item = netRepeater.itemAt(root.renamingIndex - root.placesEntries.length)
        return item && item.renaming ? item : null
    }

    function cancelRename() {
        if (root.renamingIndex < 0)
            return
        root.renamingIndex = -1
        root.renameFinished()
    }

    // An empty submitted name reverts rather than writing an empty label.
    function commitRename(index, name) {
        if (root.renamingIndex !== index) return
        root.renamingIndex = -1
        root.renameFinished()
        var trimmed = String(name || "").trim()
        if (trimmed.length === 0)
            return
        var entry = root.networkEntries[index - root.placesEntries.length]
        if (!entry)
            return
        root.service.rename(entry.uri, trimmed)
    }

    Rectangle {
        anchors.fill: parent
        color: Theme.color.surface
    }

    onCursorIndexChanged: root.revealCursor()

    // Anchored inside the Flickable's contentItem, so a row's y is already the scroll coordinate.
    function revealCursor() {
        var row = root.railItemFor(root.cursorIndex)
        if (!row)
            return
        var p = row.mapToItem(scroller.contentItem, 0, 0)
        if (p.y < scroller.contentY)
            scroller.contentY = p.y
        else if (p.y + row.height > scroller.contentY + scroller.height)
            scroller.contentY = p.y + row.height - scroller.height
    }

    // The rows live in a viewport: a rail taller than its own height could show no bottom row at all.
    Timer {
        id: availability
        interval: 120
        onTriggered: {
            var indices = []
            for (var i = 0; i < favRepeater.count; i++) {
                var item = favRepeater.itemAt(i)
                var point = item ? item.mapToItem(scroller.contentItem, 0, 0) : null
                if (point && point.y + item.height > scroller.contentY && point.y < scroller.contentY + scroller.height)
                    indices.push(i)
            }
            Favourites.inspect(indices)
        }
    }
    Flickable {
        id: scroller
        onContentYChanged: availability.restart()
        anchors.fill: parent
        clip: true
        contentWidth: width
        contentHeight: rail.height + 2 * Style.spacing.rowPaddingX
        boundsBehavior: Flickable.StopAtBounds

        FastScrollHandler {
            parent: scroller
            flickable: scroller
        }
        Flea.ViewportScrollBar { parent: scroller; anchors.top: parent.top; anchors.right: parent.right; flickable: scroller }
        Column {
            id: rail
            anchors.top: parent.top
            anchors.topMargin: Style.spacing.rowPaddingX
            anchors.left: parent.left
            anchors.right: parent.right

            Text {
                id: placesHeading
                x: Style.spacing.rowPaddingX
                topPadding: Math.ceil(font.pixelSize * 0.15)
                bottomPadding: Style.spacing.rowGap
                visible: root.homeEntries.length + root.trashEntries.length > 0
                text: "PLACES"
                color: Theme.color.muted
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                font.letterSpacing: 1
                textFormat: Text.PlainText
            }
            Repeater {
                id: homeRepeater
                model: root.homeEntries
                delegate: SidebarRow {
                    cursor: index === root.cursorIndex
                    focused: root.focused
                    onActivated: function (idx) { root.activate(idx) }
                    onMenuRequested: function(idx, pos) { root.openRailMenu(idx, pos) }
                    onTabRequested: function (idx) { PlaceMenu.openTabAt(root, idx) }
                }
            }
            Repeater {
                id: trashRepeater
                model: root.trashEntries
                delegate: SidebarRow {
                    // The menu sets cursorIndex on open, so the Trash row takes the cursor rung; trashActive keeps it lit.
                    cursor: root.trashActive || (index + root.homeEntries.length === root.cursorIndex)
                    focused: root.focused || root.trashActive
                    onActivated: function (idx) { root.activate(idx + root.homeEntries.length) }
                    onMenuRequested: function(idx, pos) { root.openRailMenu(idx + root.homeEntries.length, pos) }
                }
            }

            Text {
                id: favouritesHeading
                visible: root.userFavouriteEntries.length > 0
                x: Style.spacing.rowPaddingX
                topPadding: (placesHeading.visible ? Style.spacing.panelGap : 0) + Math.ceil(font.pixelSize * 0.15)
                bottomPadding: Style.spacing.rowGap
                text: "FAVORITES"
                color: Theme.color.muted
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                font.letterSpacing: 1
                textFormat: Text.PlainText
            }
            Repeater {
                id: favRepeater
                model: root.userFavouriteEntries
                delegate: SidebarRow {
                    cursor: index + root.homeEntries.length + root.trashEntries.length === root.cursorIndex
                    focused: root.focused
                    onActivated: function (idx) { root.activate(idx + root.homeEntries.length + root.trashEntries.length) }
                    onMenuRequested: function (idx, pos) { root.openRailMenu(idx + root.homeEntries.length + root.trashEntries.length, pos) }
                    onTabRequested: function (idx) { PlaceMenu.openTabAt(root, idx + root.homeEntries.length + root.trashEntries.length) }
                }
            }

            // The OEM panel idiom's own group gap, not the tighter row-to-row rhythm rows keep inside a group.
            Item {
                visible: root.networkEntries.length > 0
                width: rail.width
                height: Style.spacing.panelGap
            }

            // Self-hides with its list below when gio, the bookmarks file and Dropbox all have nothing to say.
            Item {
                visible: root.networkEntries.length > 0
                width: rail.width
                height: netHeading.implicitHeight + Style.spacing.rowGap

                Text {
                    id: netHeading
                    x: Style.spacing.rowPaddingX
                    topPadding: Math.ceil(font.pixelSize * 0.15)
                    text: "NETWORK"
                    color: Theme.color.muted
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    font.letterSpacing: 1
                    textFormat: Text.PlainText
                }

                // A hand-drawn plus, not a Text "+": at caption size the font glyph read as a Christian cross, not a plus. Sized off the heading's own font token.
                Glyph {
                    id: addGlyph
                    // The rail's trailing indicator slot: caption wide, inset by rowPaddingX, anchored exactly as ui/SidebarRow.qml's dot is.
                    anchors.right: parent.right
                    anchors.rightMargin: Style.spacing.rowPaddingX
                    anchors.verticalCenter: netHeading.verticalCenter
                    name: "plus"
                    color: Theme.color.muted
                    width: Theme.font.caption
                    height: width
                }

                // Bigger than the ink it covers, so it centres on the ink's own box, in real pixels: a centre anchor quantises an odd size difference and leaves the two centres half a pixel apart.
                Item {
                    id: addMark
                    width: Math.max(Theme.hitMin, Theme.font.caption)
                    height: width
                    x: addGlyph.x + (addGlyph.width - width) / 2
                    y: addGlyph.y + (addGlyph.height - height) / 2
                    Accessible.role: Accessible.Button
                    Accessible.name: "Add network location"
                    Accessible.onPressAction: root.addRequested()
                    TapHandler { onTapped: root.addRequested() }
                }
            }

            Repeater {
                id: netRepeater
                model: root.networkEntries
                delegate: SidebarRow {
                    cursor: (index + root.placesEntries.length) === root.cursorIndex
                    focused: root.focused
                    renaming: (index + root.placesEntries.length) === root.renamingIndex
                    onActivated: function (idx) { root.activate(idx + root.placesEntries.length) }
                    onEjectRequested: function (idx) { root.ejectRow(idx + root.placesEntries.length) }
                    onMenuRequested: function (idx, pos) { root.openRailMenu(idx + root.placesEntries.length, pos) }
                    onRenameCommitted: function (idx, text) { root.commitRename(idx + root.placesEntries.length, text) }
                    onRenameCancelled: root.cancelRename()
                }
            }

            Item {
                visible: root.deviceEntries.length > 0
                width: rail.width
                height: Style.spacing.panelGap
            }

            // Self-hides with its list where lsblk reports no disk, and carries no add mark: nothing here is bookmarked.
            Text {
                id: devHeading
                visible: root.deviceEntries.length > 0
                x: Style.spacing.rowPaddingX
                topPadding: Math.ceil(font.pixelSize * 0.15)
                bottomPadding: Style.spacing.rowGap
                text: "DEVICES"
                color: Theme.color.muted
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                font.letterSpacing: 1
                textFormat: Text.PlainText
            }

            Repeater {
                id: devRepeater
                model: root.deviceEntries
                delegate: SidebarRow {
                    cursor: (index + root.placesEntries.length + root.networkEntries.length) === root.cursorIndex
                    focused: root.focused
                    onActivated: function (idx) { root.activate(idx + root.placesEntries.length + root.networkEntries.length) }
                    onEjectRequested: function (idx) { root.ejectRow(idx + root.placesEntries.length + root.networkEntries.length) }
                    onMenuRequested: function (idx, pos) { root.openRailMenu(idx + root.placesEntries.length + root.networkEntries.length, pos) }
                }
            }
        }
    }

    // The "+" ink, its hit target and the rail's own indicator dot: the three boxes that share one centre.
    function networkMarkItems() { var netRow = netRepeater.itemAt(0); return [addGlyph, addMark, netRow ? netRow.indicatorSlot : null] }
    function headingItems() { return [placesHeading, favouritesHeading, netHeading, devHeading] }
    // The rail has no ListView virtualization, so every row already exists; the same itemFor idiom ui/Pane.qml uses for the list, so a test can find a rail row's on-screen box.
    function railItemFor(index) {
        if (index < root.homeEntries.length) return homeRepeater.itemAt(index)
        var rest = index - root.homeEntries.length
        if (rest < root.trashEntries.length) return trashRepeater.itemAt(rest)
        rest -= root.trashEntries.length
        if (rest < root.userFavouriteEntries.length) return favRepeater.itemAt(rest)
        rest -= root.userFavouriteEntries.length
        if (rest < root.networkEntries.length)
            return netRepeater.itemAt(rest)
        rest -= root.networkEntries.length
        return devRepeater.itemAt(rest)
    }
    // The one divider in the whole design.
    Rectangle {
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: Style.spacing.hairline
        color: Theme.color.foreground
        opacity: 0.12
    }
}
