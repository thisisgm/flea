import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import "." as Flea
import "js/Eject.js" as Eject
import "js/Mounts.js" as Mounts
import "js/Places.js" as Places
import "js/PlaceMenu.js" as PlaceMenu
import "js/RailMenu.js" as RailMenu
import "js/RailKeys.js" as RailKeys

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
    readonly property var userFavouriteEntries: RailPlaces.favouriteEntries
    readonly property var homeEntries: RailPlaces.homeEntries
    // Recent sits under Home and carries a location token.
    readonly property var homeLead: root.homeEntries.slice(0, 1)
    readonly property var homeRest: root.homeEntries.slice(1)
    readonly property var recentEntries: RailPlaces.recentEntries
    readonly property var placesEntries: root.homeLead.concat(root.recentEntries, root.homeRest, root.trashEntries, root.userFavouriteEntries)
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
    readonly property var trashEntries: RailPlaces.trashEntries.map(function (entry) {
        return Object.assign({}, entry, { count: root.trashCount })
    })
    signal trashRequested()

    // The saved place an Edit is rewriting, "" when none is; ui/js/RailMenu.js editPlace sets it.
    property string editingPlace: ""
    // And the request that Edit's own attempt went out with, so no other mount answers for it.
    property string editingRequest: ""
    // PaneRail injects the window-long host, so hiding the rail preserves mounts, waits and dialog answers.
    property var service: null
    readonly property var networkEntries: root.placesState.showNetwork === false || !root.railGate.showNetwork || !root.service ? [] : root.service.entries
    // The poll rebinds its delegates in place, so a rename left standing would edit a different share.
    onNetworkEntriesChanged: root.cancelRename()
    // Phones ride the DEVICES group behind the block devices: a plugged phone is a device to the person holding it, whatever transport gvfs reaches it over.
    readonly property var deviceEntries: root.placesState.showDevices === false || !root.railGate.showDevices ? [] : devices.entries.concat(phones.entries)
    // The eject chain's guard state, read fresh at ipc time, so a failed eject names its guard.
    function ejectChainState() { return devices.ejectState() }
    readonly property var entries: root.placesEntries.concat(root.networkEntries, root.deviceEntries)

    // The rail lands in one step by gating the entries themselves, so cursor, IPC and menus match only drawn rows.
    readonly property int railSettleMs: 800
    property bool railDeadlineElapsed: false
    property bool bookmarksReady: false
    // Favourite-relative insertion boundary, count past the last row, -1 when idle; a rebuild clears it.
    property int reorderLine: -1
    readonly property var railGate: Mounts.railGroupsReady(root.bookmarksReady && root.service !== null && root.service.listingAnswered && root.service.dropboxAnswered, devices.firstAnswered && phones.firstDone, root.railDeadlineElapsed ? root.railSettleMs : 0, root.railSettleMs)
    Timer { interval: root.railSettleMs; running: true; repeat: false; onTriggered: root.railDeadlineElapsed = true }

    // Reconcile only the aggregate; evaluating entries from a group's change handler re-enters its binding.
    onEntriesChanged: {
        root.reorderLine = -1
        var next = Places.railCursorAfter(root.cursorEntries, root.entries, root.cursorIndex)
        if (root.renamingIndex >= 0
            && Places.railCursorAfter(root.cursorEntries, root.entries, root.renamingIndex) !== root.renamingIndex)
            root.cancelRename()
        root.cursorEntries = root.entries
        root.cursorIndex = next
        availability.restart()
    }

    signal opened(string path)
    // Recent answers bounded newest-first history paths, which the pane opens through listpaths.
    signal recentRequested(var paths, var requester, var visits)
    signal addRequested()
    // The rail's Edit row asks the window to open the dialog over the saved place.
    signal editRequested(string uri, string label, string password, string reason, bool failedConnect, var origin)
    signal message(string text, bool isError)
    signal forgetMessage(string text)

    // The entry index mid-rename, or -1; ui/SidebarRow.qml swaps its Text for a field on it, and ui/Pane.qml holds its keys off while it stands.
    property int renamingIndex: -1
    // Fires once, on both commit and cancel, so ui/Pane.qml has one place to hand focus back.
    signal renameFinished()
    // Any press inside the rail, so an overlay rail can take the keyboard though a row or the Flickable below holds the grab: the handler's item is above every child.
    signal pressed()

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
        id: bookmarksFile
        path: Quickshell.env("HOME") + "/.config/gtk-3.0/bookmarks"
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onLoaded: { root.bookmarksReady = true; root.pushBookmarks() }
        onLoadFailed: { root.bookmarksReady = true; root.pushBookmarks() }
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

    // The desktop's own history, read and never written, kept across opens.
    property var recentPaths: []
    property var recentVisits: ({})
    property bool recentKept: false
    property bool recentReading: false
    property int recentChanges: 0
    property int recentReadAt: -1
    // Every asker waiting on the read in flight, null meaning the rail pane itself.
    property var recentRequesters: []
    // How many times the history has been parsed; the seam reads it the way it reads the jump's.
    property int recentReads: 0
    function readRecent(requester) {
        // One read at a time: every asker waits on it, so each pane opens once it lands.
        if (root.recentReading) {
            root.recentRequesters = recentReader.item.joinRequesters(root.recentRequesters, requester)
            return
        }
        if (root.recentKept && root.recentReadAt === root.recentChanges) {
            root.recentRequested(root.recentPaths, requester || null, root.recentVisits)
            return
        }
        // Build the helper with the reader, only after an uncached Recent action.
        recentReader.active = true
        root.recentRequesters = recentReader.item.joinRequesters([], requester)
        root.recentReading = true
        root.recentReadAt = root.recentChanges
        recentWatcher.path = recentReader.item.file
        recentReader.item.refresh()
    }
    // Watching only: it never loads the file, and it reports a rename over it, a delete and a re-create alike.
    FileView {
        id: recentWatcher
        preload: false
        watchChanges: true
        onFileChanged: root.recentChanges += 1
    }
    // The history reader and its helpers load on a Recent action, then drop after the read.
    Loader {
        id: recentReader
        active: false
        source: "PickerRecent.qml"
    }
    Connections {
        target: recentReader.item
        function onRefreshed() {
            root.recentPaths = recentReader.item.paths
            root.recentVisits = recentReader.item.visits
            root.recentKept = true
            root.recentReading = false
            root.recentReads += 1
            // Drop the bounded parsed model later, after the reader finishes emitting this signal.
            Qt.callLater(function () { if (!root.recentReading) recentReader.active = false })
            var askers = root.recentRequesters
            root.recentRequesters = []
            for (var i = 0; i < askers.length; i++) root.recentRequested(root.recentPaths, askers[i], root.recentVisits)
        }
    }

    // The context menu's gate for the two Dropbox rows, so the pane never reaches into the rail.
    readonly property bool dropboxReady: root.service !== null && root.service.dropboxReady
    readonly property var providerService: root.service

    DeviceMounts {
        id: devices
        showUnmounted: root.placesState.showUnmounted !== false
        onOpened: function (path) { root.opened(path) }
        onMessage: function (text, isError) { root.message(text, isError) }
        onForgetMessage: function (text) { root.forgetMessage(text) }
        // An eject releases the volume, so readers on it stop first and panes on it go Home.
        onQuiesce: function (path) { if (root.navigationPane) root.navigationPane.quiesceVolume(path) }
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
    Connections {
        target: Favourites
        function onFailed(message) { root.message(message, true) }
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

    // The favourites store persists reorders; a refused move keeps its row and reports the failure.
    function moveFavourite(from, to) {
        root.reorderLine = -1
        if (to !== from)
            Favourites.move(from, to)
    }
    // A favourite's path is already real and opens directly; a share or a volume may need its Service.
    function openFavourite(index) {
        Places.openEntry(root.userFavouriteEntries[index], {
            pane: root.navigationPane, opened: root.opened, message: root.message,
            networkHost: root.networkHost, trash: root.trashRequested
        })
    }

    function activate(index) {
        root.cancelRename()
        root.cursorIndex = index
        var entry = root.entries[index]
        if (!entry) return
        if (entry.kind === "favourite") { root.openFavourite(entry.favouriteIndex); return }
        if (entry.kind === "home") { root.opened(entry.path); return }
        if (entry.kind === "recent") { root.readRecent(); return }
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
        color: Glass.surfacePlane
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
        // No bar and no lane: rows fill the rail and still scroll by wheel, touchpad and keys.
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
                id: homeLeadRepeater
                model: root.homeLead
                delegate: SidebarRow {
                    cursor: index === root.cursorIndex
                    focused: root.focused
                    onActivated: function (idx) { root.activate(idx) }
                    onMenuRequested: function(idx, pos) { root.openRailMenu(idx, pos) }
                    onTabRequested: function (idx) { PlaceMenu.openTabAt(root, idx) }
                }
            }
            Repeater {
                id: recentRepeater
                model: root.recentEntries
                delegate: SidebarRow {
                    cursor: index + root.homeLead.length === root.cursorIndex
                    focused: root.focused
                    onActivated: function (idx) { root.activate(idx + root.homeLead.length) }
                    onMenuRequested: function(idx, pos) { root.openRailMenu(idx + root.homeLead.length, pos) }
                }
            }
            Repeater {
                id: homeRestRepeater
                model: root.homeRest
                delegate: SidebarRow {
                    cursor: index + root.homeLead.length + root.recentEntries.length === root.cursorIndex
                    focused: root.focused
                    onActivated: function (idx) { root.activate(idx + root.homeLead.length + root.recentEntries.length) }
                    onMenuRequested: function(idx, pos) { root.openRailMenu(idx + root.homeLead.length + root.recentEntries.length, pos) }
                    onTabRequested: function (idx) { PlaceMenu.openTabAt(root, idx + root.homeLead.length + root.recentEntries.length) }
                }
            }
            Repeater {
                id: trashRepeater
                model: root.trashEntries
                delegate: SidebarRow {
                    // The menu sets cursorIndex on open, so isCursor also means the open menu is on this row.
                    cursor: RailKeys.trashCursor(root.trashActive, index + root.homeLead.length + root.recentEntries.length + root.homeRest.length === root.cursorIndex, root.focused, root.menu && root.menu.opened && root.menu.forRail)
                    focused: root.focused || root.trashActive
                    onActivated: function (idx) { root.activate(idx + root.homeLead.length + root.recentEntries.length + root.homeRest.length) }
                    onMenuRequested: function(idx, pos) { root.openRailMenu(idx + root.homeLead.length + root.recentEntries.length + root.homeRest.length, pos) }
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
                    cursor: index + root.homeLead.length + root.recentEntries.length + root.homeRest.length + root.trashEntries.length === root.cursorIndex
                    focused: root.focused
                    // The rail's own reorder drag, persisted through the favourites store.
                    dragFrom: modelData.favouriteIndex
                    lineCount: root.userFavouriteEntries.length
                    onMoved: function (to) { root.moveFavourite(dragFrom, to) }
                    onReorderAt: function (line) { root.reorderLine = line }
                    onActivated: function (idx) { root.activate(idx + root.homeLead.length + root.recentEntries.length + root.homeRest.length + root.trashEntries.length) }
                    onMenuRequested: function (idx, pos) { root.openRailMenu(idx + root.homeLead.length + root.recentEntries.length + root.homeRest.length + root.trashEntries.length, pos) }
                    onTabRequested: function (idx) { PlaceMenu.openTabAt(root, idx + root.homeLead.length + root.recentEntries.length + root.homeRest.length + root.trashEntries.length) }
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
        // Sidebar040 specimen 1: the bar lies over the boundary, one hairline in the row above and the rest under it.
        Rectangle {
            readonly property var row: favRepeater.itemAt(Math.min(root.reorderLine, favRepeater.count - 1))
            readonly property real boundary: row ? row.y + (root.reorderLine === favRepeater.count ? row.height : 0) : 0
            visible: root.reorderLine >= 0 && root.reorderLine <= favRepeater.count && row !== null
            width: rail.width
            height: Theme.accentEdge * Theme.spacing.hairline
            y: boundary - Theme.spacing.hairline + rail.y
            color: Theme.color.accent
            z: 1
        }
    }

    // The "+" ink, its hit target and the rail's own indicator dot: the three boxes that share one centre.
    function networkMarkItems() { var netRow = netRepeater.itemAt(0); return [addGlyph, addMark, netRow ? netRow.indicatorSlot : null] }
    function headingItems() { return [placesHeading, favouritesHeading, netHeading, devHeading] }
    // The rail has no ListView virtualization, so every row already exists; the same itemFor idiom ui/Pane.qml uses for the list, so a test can find a rail row's on-screen box.
    function railItemFor(index) {
        if (index < root.homeLead.length) return homeLeadRepeater.itemAt(index)
        var rest = index - root.homeLead.length
        if (rest < root.recentEntries.length) return recentRepeater.itemAt(rest)
        rest -= root.recentEntries.length
        if (rest < root.homeRest.length) return homeRestRepeater.itemAt(rest)
        rest -= root.homeRest.length
        if (rest < root.trashEntries.length) return trashRepeater.itemAt(rest)
        rest -= root.trashEntries.length
        if (rest < root.userFavouriteEntries.length) return favRepeater.itemAt(rest)
        rest -= root.userFavouriteEntries.length
        if (rest < root.networkEntries.length)
            return netRepeater.itemAt(rest)
        rest -= root.networkEntries.length
        return devRepeater.itemAt(rest)
    }
    // Above the Flickable and every row, because a PointHandler under their exclusive press grab is never given the press; a passive handler leaves the press to the row.
    Item {
        anchors.fill: parent
        z: 1
        PointHandler {
            acceptedButtons: Qt.AllButtons
            onActiveChanged: if (active) root.pressed()
        }
    }
    // The rail edge, one Divider shared with the column edges.
    Flea.Divider {
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: parent.bottom
    }
}
