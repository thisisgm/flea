import QtQuick
import "." as Flea
import "js/Focus.js" as Focus
import "js/Eject.js" as Eject
import "js/RailKeys.js" as RailKeys
import "js/RailMenu.js" as RailMenu

// The pane's rail, and the two ways it comes and goes. With Auto-hide sidebar off it is part of the
// pane and Show sidebar governs it, which is 0.2.1's behaviour. Directive 77, GM's own sentence:
// "hovering over the left side should show it, like every other autohide feature in every other file
// manager ever". So with the switch on the rail is withdrawn at every width, the pointer at the
// window's left edge reveals it over the pane rather than reflowing it, and it withdraws a moment
// after the pointer or the keyboard leaves.
Item {
    id: root

    property var pane: null
    // The window-long network host ui/WindowBody.qml builds lazily on the first open, handed to
    // the rail below: the rail Loader unloads the Sidebar when the rail hides, but this service
    // and these connections outlive it, so a mount, a bridge wait or a dialog answer that lands
    // afterwards still opens, messages and clears its sticky line.
    property var service: null
    readonly property bool overlay: ViewState.railAutoHide
    readonly property bool hidden: root.overlay ? !root.revealed : ViewState.railHidden
    // An overlay owes the pane no width, so the listing never reflows as the rail comes and goes.
    // This item is that width, and the rail itself draws past it.
    readonly property real inset: root.overlay ? 0 : rail.width
    readonly property real railWidth: rail.active ? rail.width : 0
    readonly property var item: rail.item

    // Long enough that crossing the edge on the way somewhere else does not flash the rail, and that
    // leaving it by a pixel on the way to a row does not drop it.
    readonly property int settleMs: 220
    property bool revealed: false
    property bool over: false
    // The rail's own context menu takes the pointer with it, so without this the rail withdraws out
    // from under the menu it just opened.
    readonly property bool menuHere: root.pane !== null && root.pane.contextMenu().opened
                                     && root.pane.contextMenu().forRail
    readonly property bool wanted: edge.hovered || root.over || root.menuHere
                                   || (root.pane !== null && root.pane.focusView === Focus.RAIL)

    onWantedChanged: {
        if (root.wanted) { settle.stop(); root.revealed = true }
        else settle.restart()
    }
    // The switch itself is a fresh start either way: nothing is revealed until the pointer asks.
    onOverlayChanged: root.revealed = false

    anchors { top: parent.top; bottom: parent.bottom }
    width: root.inset

    Timer {
        id: settle
        interval: root.settleMs
        onTriggered: root.revealed = false
    }

    // The strip that answers the pointer, at the window's own left edge and only while the switch is
    // on. It holds no MouseArea, so a click on the listing under it still lands on the listing.
    Item {
        id: strip
        anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
        width: Theme.space(4)
        enabled: root.overlay
        HoverHandler { id: edge; enabled: root.overlay }
    }

    Loader {
        id: rail
        anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
        width: item ? item.implicitWidth : 0
        active: root.pane !== null && !root.pane.listOnly && root.pane.sharedSidebar === null && !root.hidden
        sourceComponent: Flea.Sidebar {
            id: sidebar
            backend: root.pane.backend
            navigationPane: root.pane.railPane
            service: root.service
            focused: root.pane.railPane.focusView === Focus.RAIL
            trashActive: root.pane.railPane.trash.opened
            stackActive: root.pane.railPane.path === "flea:stack"
            onOpened: function(path) { RailKeys.openFrom(root.pane.railPane, path, sidebar) }
            onTrashRequested: root.pane.railPane.trash.open()
            onMessage: function(text, isError) { RailKeys.messaged(sidebar, isError); root.pane.message(text, isError) }
            onForgetMessage: function(text) { root.pane.forgetMessage(text) }
            menu: root.pane.railPane.contextMenu()
            onRenameFinished: root.pane.railPane.listArea.forceActiveFocus()
            // The rail stays up while the pointer is on it, which is the other half of the reveal.
            HoverHandler { onHoveredChanged: root.over = hovered }
        }
    }

    // The rail's Sidebar renders the window-long service below; the service's answers
    // route once by origin in ui/WindowBody.qml, so this file carries no Connections of its
    // own and a dual-view open cannot fire twice.

    // Ctrl+E from a listing with the rail hidden. The rail Loader above unloads the Sidebar with
    // its DeviceMounts poll, eject verdict state and releaseChosen, so the key has nothing to read
    // and nothing to release through. It spins this transient instead: the same DeviceMounts
    // component the rail uses, never a second poller, alive only for this flow. Its first listing
    // resolves the holding volume through the same Mounts.holding and Mounts.railMenu verdict and
    // releases through the same RailMenu.release and devices.eject path with the same messages;
    // the verdict lands, the host unloads, and while nothing is pressed nothing here exists.
    property bool hiddenEjectArmed: false
    property bool hiddenEjectStarted: false
    function unloadHiddenEject() {
        // Past the signal that asked for it: the transient is unloaded from inside its own
        // listing or message, where a synchronous unload would destroy the sender mid-emission.
        Qt.callLater(function () { hiddenEject.active = false })
    }
    function ejectHidden() {
        if (!root.pane)
            return
        if (!hiddenEject.active) {
            hiddenEjectArmed = true
            hiddenEjectStarted = false
            hiddenEject.active = true
            return
        }
        // A flow is already waiting on its first listing, or an eject is running: re-resolve
        // against the freshest entries, where the service refuses a concurrent eject itself.
        hiddenEjectArmed = true
        if (hiddenEject.item && hiddenEject.item.firstAnswered)
            root.hiddenEjectReady()
    }
    function hiddenEjectReady() {
        var device = hiddenEject.item
        if (!device || !hiddenEjectArmed)
            return
        hiddenEjectArmed = false
        // The adapter is the Sidebar's shape and nothing more: entries to resolve against and
        // releaseChosen to release through, so this lands in Eject.release beside the rail's own.
        // Device-only entries resolve everything a listing can hold: shares and phones carry no
        // path, and favourites offer no release, so Mounts.holding skips them either way.
        var adapter = { entries: device.entries, deviceEntries: device.entries, networkEntries: [] }
        adapter.releaseChosen = function (action, key) { RailMenu.release(action, key, device, null, adapter) }
        Eject.release(root.pane, adapter, false)
        if (device._ejectDevice.length > 0) {
            hiddenEjectStarted = true
            return
        }
        root.unloadHiddenEject()
    }
    Loader {
        id: hiddenEject
        active: false
        sourceComponent: Flea.DeviceMounts {
            showUnmounted: ((ViewState.state.places || {}).showUnmounted !== false)
            onOpened: function (path) { root.pane.open(path) }
            onMessage: function (text, isError) {
                root.pane.message(text, isError)
                var device = hiddenEject.item
                if (root.hiddenEjectStarted && device && device._ejectDevice.length === 0) {
                    root.hiddenEjectStarted = false
                    root.unloadHiddenEject()
                }
            }
            onForgetMessage: function (text) { root.pane.forgetMessage(text) }
            onFirstAnsweredChanged: root.hiddenEjectReady()
        }
    }
}
