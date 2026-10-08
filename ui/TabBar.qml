import QtQuick
import Quickshell
import Quickshell.Io
import "." as Flea
import "js/DragOut.js" as DragOut
import "js/Tabs.js" as Tabs
import "js/TabMove.js" as TabMove

// The window's tab strip. Hidden with no height until a second tab exists, so the default window
// keeps the chrome-to-list layout every existing click test and the first-paint path already have.
Item {
    id: root

    property var pane: null
    enabled: root.pane !== null && root.pane.enabled

    // pane.tabs is a replaced JS object, so these bindings have to read it directly; a helper
    // call alone would not re-run when t opens a second tab.
    readonly property var tabs: pane ? pane.tabs : null
    readonly property string path: pane ? pane.path : ""
    readonly property int tabCount: root.tabs && root.tabs.items && root.tabs.items.length > 0 ? root.tabs.items.length : 1
    readonly property int currentIndex: root.tabs ? root.tabs.index : 0
    readonly property bool open: root.tabCount > 1
    readonly property int tabWidth: {
        var n = Math.max(1, root.tabCount)
        var avail = Math.max(0, root.width - Theme.hitMin)
        var maxW = Math.round(Theme.font.caption * 12) + Theme.hitMin + 2 * Theme.spacing.rowPaddingX
        var minW = Theme.hitMin * 3
        return Math.round(Math.max(minW, Math.min(maxW, avail / n)))
    }

    // How long a drag rests on a tab before the tab is selected: long enough to cross it on the way elsewhere.
    readonly property int hoverSwitchMs: 400

    // Tabs040 callout 1: a tab drag reorders the strip. dragFrom is the tab
    // held, dropAt the insertion point (0..tabCount) its pointer names.
    property int dragFrom: -1
    property int dropAt: -1
    readonly property int dragTo: root.dropAt > root.dragFrom ? root.dropAt - 1 : root.dropAt
    readonly property var layoutOrder: {
        var order = []
        for (var i = 0; i < root.tabCount; i++) order.push(i)
        if (root.dragFrom >= 0 && root.dropAt >= 0)
            TabMove.reorder(order, root.dragFrom, root.dragTo, root.currentIndex)
        return order
    }

    // Gesture state is separate from outstanding lifts awaiting their own ack.
    property var outMime: ({})
    property bool outActive: false
    property bool outOutside: false
    property bool outRefused: false
    property int outIndex: -1
    property string outPath: ""
    property string outPid: ""
    property string outToken: ""
    property bool ownAccepted: false
    property var pendingTab: null
    readonly property int tabDropPeekFirst: 2
    // A receiver cannot wait longer than the source keeps its unacknowledged lift.
    readonly property int tabDropWaitMs: Tabs.ACK_WAIT_MS
    onPaneChanged: root.refuseTabDrop()
    // The lift outlives the drag's own end, until the taken ack or this wait ends.
    property int ackWaitMs: Tabs.ACK_WAIT_MS
    property double ackLiftedAt: 0
    property var takenQueue: []
    property var outstandingLifts: []
    readonly property int ackPollMs: 100 // Settle acknowledged lifts when their captured pane finishes loading.
    // Stage trace, on only with FLEA_TRACE_TABDRAG=1; read once, silent otherwise.
    readonly property bool tabTrace: Quickshell.env("FLEA_TRACE_TABDRAG") === "1"
    function traceTab(stage, detail) { if (root.tabTrace) console.log("TABDRAG " + stage + " pid=" + Quickshell.processId + " " + detail) }

    Timer {
        id: ackTimer
        interval: root.ackPollMs
        repeat: true
        running: root.outstandingLifts.length > 0
        onTriggered: root.drainLifts()
    }

    Timer {
        id: tabDropDeadline
        interval: root.tabDropWaitMs
        onTriggered: root.refuseTabDrop()
    }

    Process {
        id: takenAck
        onExited: function (exitCode, exitStatus) {
            root.traceTab("taken-exit", "code=" + exitCode)
            root.pumpTaken()
        }
    }

    // QTBUG-64128: a DropArea rejects a drag sourced from its ancestor.
    Item { id: dragOrigin; width: 0; height: 0; visible: false }
    Drag.source: dragOrigin
    Flea.TabDragGeometry { id: sourceGeometry }

    Drag.dragType: Drag.Automatic
    // Private tab MIME must never offer the folder as a file drag.
    Drag.supportedActions: Qt.MoveAction
    Drag.mimeData: root.outMime
    Drag.onDragFinished: function (dropAction) { root.outFinished(dropAction) }

    function dragStarted(index) {
        root.dragFrom = index
        root.dropAt = index < 0 ? -1 : index + 1
    }
    function dragMoved(x) {
        if (root.dragFrom < 0)
            return
        root.dropAt = TabMove.insertionAt(x, root.tabWidth, root.tabCount)
    }
    function dragFinished() {
        if (root.dragFrom >= 0 && root.dropAt >= 0 && root.pane) {
            var at = root.dropAt
            Tabs.move(root.pane, root.dragFrom, at > root.dragFrom ? at - 1 : at)
        }
        root.dragFrom = -1
        root.dropAt = -1
        root.drainLifts()
    }

    // A lift captures the tab payload before the pane can change.
    function tabLiftBegan(index) {
        if (!Tabs.canLift(root.pane)) {
            root.dragFrom = -1
            root.dropAt = -1
            return
        }
        root.dragStarted(index)
        root.outIndex = index
        var info = Tabs.tabInfo(root.pane, index)
        root.outPath = info ? info.path : ""
        root.outPid = String(Quickshell.processId)
        root.outToken = Tabs.newToken()
        var band = strip.mapToItem(null, 0, 0)
        sourceGeometry.begin(root.outToken, { x: band.x, y: band.y,
            width: root.width - strip.x, height: root.height })
        root.ackLiftedAt = 0
        var lift = Tabs.captureLift(root.pane, index, root.outToken, root.ackLiftedAt)
        if (lift) root.outstandingLifts = root.outstandingLifts.concat([lift])
        root.outMime = Tabs.tabDragMime(root.pane, index, root.outPid, root.outToken)
        root.outOutside = false
        root.outRefused = false
        root.ownAccepted = false
    }

    // A platform drag freezes the strip reorder until the drop answers.
    function tabLiftMoved(stripX, winX, winY) {
        root.dragMoved(stripX)
        if (root.parent)
            root.outOutside = winX < 0 || winY < 0 || winX > root.parent.width || winY > root.parent.height
        if (root.outOutside && !root.outActive && root.outMime[Tabs.TAB_MIME]) {
            var refusal = Tabs.tearRefusal(root.pane)
            if (refusal.length > 0) {
                if (!root.outRefused && root.pane)
                    root.pane.message(refusal, false)
                root.outRefused = true
                return
            }
            root.outActive = true
            root.Drag.active = true
            root.traceTab("drag-start", "index=" + root.outIndex + " path=" + root.outPath + " mime=" + Object.keys(root.outMime).join(",") + " dragType=" + root.Drag.dragType)
        }
    }

    // A platform gesture owns its release, so the handler must not reorder again.
    function tabLiftEnded() {
        if (root.outActive || root.ownAccepted) {
            root.dragFrom = -1
            root.dropAt = -1
            return
        }
        root.dragFinished()
        if (root.ackLiftedAt === 0) root.clearAck()
    }

    // The lift must survive the drag ending until its acknowledgment arrives.
    function holdAck() {
        var lift = Tabs.liftFor(root.outstandingLifts, root.outToken)
        root.ackLiftedAt = Date.now()
        if (lift) lift.liftedAt = root.ackLiftedAt
    }
    function clearAck() {
        root.outstandingLifts = root.outstandingLifts.filter(function (lift) { return lift.token !== root.outToken })
        root.outToken = ""
        root.outIndex = -1
        root.outPath = ""
        root.ackLiftedAt = 0
    }

    function drainLifts() {
        if (root.dragFrom >= 0) return
        var now = Date.now()
        var kept = []
        for (var i = 0; i < root.outstandingLifts.length; i++) {
            var lift = root.outstandingLifts[i]
            if (!lift.taken && !Tabs.ackCloses(lift.token, lift.token, lift.liftedAt, now)) {
                if (root.outToken === lift.token) root.outToken = ""
                continue
            }
            var index = Tabs.resolveMovedTab(lift.pane, lift.identity)
            if (index < 0) continue
            if (lift.taken && !lift.pane.listInFlight) {
                Tabs.closeTabAfterMove(lift.pane, index)
                if (root.outToken === lift.token) root.outToken = ""
            } else kept.push(lift)
        }
        root.outstandingLifts = kept
    }

    // A cross-process drop closes the source tab only after a matching acknowledgment.
    function outFinished(dropAction) {
        root.traceTab("drag-finished", "action=" + dropAction)
        var consumed = root.ownAccepted
        root.dragFrom = -1
        root.dropAt = -1
        root.outActive = false
        root.ownAccepted = false
        if (consumed)
            root.clearAck()
        else if (root.outToken.length > 0)
            root.holdAck()
    }

    // Qt's platform Escape filter and explicit cancellation share the same ending.
    function cancelOut() {
        if (!root.outActive)
            return
        root.Drag.cancel()
        if (root.outActive)
            root.outFinished(Qt.IgnoreAction)
    }

    function returnAt(at) {
        var lift = Tabs.liftFor(root.outstandingLifts, root.outToken)
        var from = lift ? Tabs.resolveMovedTab(lift.pane, lift.identity) : -1
        if (from >= 0)
            Tabs.move(lift.pane, from, at > from ? at - 1 : at)
        root.ownAccepted = true
    }

    function catcherDrop(x, y) {
        if (!root.outActive)
            return
        var result = Tabs.catcherOutcome(sourceGeometry.rect, sourceGeometry.strip,
                                        x, y, root.tabWidth, root.tabCount)
        root.traceTab("catcher-drop", "outcome=" + result.outcome + " global=" + x + "," + y
                      + (sourceGeometry.rect ? "" : " reason=unknown-source-rectangle"))
        if (result.outcome === "tearoff")
            root.tearOffAt()
        else if (result.outcome === "return")
            root.returnAt(result.at)
    }

    // The new window acknowledges only after listing; a failed spawn keeps the source tab.
    function tearOffAt() {
        if (root.outPath.length === 0)
            return
        var info = Tabs.parseTabMime(root.outMime[Tabs.TAB_MIME])
        Quickshell.execDetached(["env", "FLEA_TAB_SOURCE_PID=" + root.outPid,
            "FLEA_TAB_CURSOR=" + (info ? info.cursor : ""),
            "FLEA_TAB_TOKEN=" + root.outToken, Quickshell.env("FLEA_BIN") || "flea", root.outPath])
        root.outFinished(Qt.IgnoreAction)
    }

    // An own-window tab drag reorders its strip instead of opening another tab.
    function tabEnterOk(drag) {
        var ok = Tabs.enterAccepts(drag.formats, drag.getDataAsString(Tabs.TAB_MIME), undefined, root.pane ? Tabs.canReceive(root.pane) : false, root.outActive)
        root.traceTab("enter-strip", "formats=" + String(drag.formats) + " ok=" + ok)
        return ok
    }

    // A foreign tab opens only after its reserved peek confirms a directory.
    function acceptTabDrop(payload, info, at) {
        if (root.pendingTab || !root.pane || !Tabs.canReceive(root.pane)) {
            root.traceTab("drop-skip", "reason=strip-accept-refused")
            return
        }
        root.pendingTab = { payload: payload, pid: info.pid, token: info.token, path: info.path, at: at, pane: root.pane }
        tabDropDeadline.restart()
        root.traceTab("peek-sent", "path=" + info.path + " first=" + root.tabDropPeekFirst + " hidden=false")
        root.pane.backend.peek(info.path, root.tabDropPeekFirst, false, false)
    }

    function refuseTabDrop() {
        var pending = root.pendingTab
        if (!pending)
            return
        root.pendingTab = null
        tabDropDeadline.stop()
        var pane = root.pane || pending.pane
        if (pane)
            pane.message("That tab could not be received.", false)
        root.traceTab("receive", "ok=false refused-pending path=" + pending.path)
    }

    // Send the taken acknowledgment only after the receiving tab opens.
    function sendTaken(pid, token) {
        root.traceTab("taken-sent", "target=" + pid + " token=" + token)
        root.takenQueue = root.takenQueue.concat([{ pid: String(pid), token: String(token) }])
        root.pumpTaken()
    }
    // One Process runs one call, so overlapping acks queue behind it instead of vanishing inside execDetached.
    function pumpTaken() {
        if (takenAck.running || root.takenQueue.length === 0)
            return
        var next = root.takenQueue[0]
        root.takenQueue = root.takenQueue.slice(1)
        takenAck.command = ["qs", "ipc", "--pid", next.pid, "call", "fleatab", "taken", next.token]
        takenAck.running = true
    }

    Connections {
        target: root.pane ? root.pane.backend : null
        function onPeeked(path, hidden, total, rows, readFailed, mode, hiddenLast, first) {
            var pending = root.pendingTab
            if (!pending || path !== pending.path || hidden !== false || hiddenLast !== false || first !== root.tabDropPeekFirst)
                return
            root.traceTab("peek-answer", "path=" + path + " failed=" + readFailed + " total=" + total)
            root.pendingTab = null
            tabDropDeadline.stop()
            if (readFailed) {
                if (root.pane)
                    root.pane.message("That folder is no longer there.", false)
                root.traceTab("receive", "ok=false refused-missing path=" + path)
                return
            }
            var received = root.pane && Tabs.receiveTab(root.pane, pending.payload, pending.at)
            root.traceTab("receive", "ok=" + !!received + " path=" + pending.path + " at=" + pending.at)
            if (received)
                root.sendTaken(pending.pid, pending.token)
        }
    }

    visible: root.open
    implicitHeight: Theme.chromeHeight
    height: visible ? implicitHeight : 0

    function itemAt(index) {
        return repeater.itemAt(index)
    }

    Rectangle {
        anchors.fill: parent
        color: Glass.surfacePlane
    }

    Rectangle {
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        height: Theme.spacing.hairline
        color: Theme.color.foreground
        opacity: 0.12
    }

    Item {
        id: strip
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: root.tabCount * root.tabWidth + Theme.hitMin

        Repeater {
            id: repeater
            model: root.open ? root.tabCount : 0
            delegate: Item {
                id: tab
                required property int index
                readonly property int slot: root.layoutOrder.indexOf(tab.index)
                x: tab.slot * root.tabWidth
                width: root.tabWidth
                height: strip.height
                // The tab under the pointer draws ghosted while its drag runs.
                opacity: root.dragFrom === tab.index ? Theme.disabledOpacity : 1

                readonly property bool current: root.currentIndex === tab.index
                // Every input named, so the label re-reads when a tab opens or the pane navigates.
                readonly property string title: Tabs.label(
                    Tabs.pathAt(root.tabs, root.currentIndex, tab.index, root.path),
                    pane ? pane.home : "")

                Accessible.role: Accessible.PageTab
                Accessible.name: tab.title
                Accessible.onPressAction: if (pane) Tabs.selectAt(pane, tab.index)

                HoverHandler { cursorShape: Qt.PointingHandCursor }

                // GM's ruling: a drag resting on a tab selects it so the drop can land in that tab's
                // listing, and a drop on the tab itself lands there too, by path, see ui/DropInto.qml.
                Flea.DropInto {
                    anchors.fill: parent
                    pane: root.pane
                    enabled: DragOut.searchTabEnabled(root.pane ? root.pane.searchMode : "", tab.current)
                    switchesOnHover: true
                    refuseLoading: DragOut.refuseLoading(root.pane && root.pane.listInFlight, true, tab.current)
                    // The pane's drop path, not its drawn one: a tab selected by the hover switch is
                    // current before its listing lands, and until then pane.path is the tab left behind.
                    // Sidebar040: a history is not a directory, so a drop onto the tab standing on it
                    // is refused rather than landing in the root it stands on.
                    dest: root.pane && root.pane.recentMode.length > 0 && tab.current ? ""
                        : Tabs.pathAt(root.tabs, root.currentIndex, tab.index,
                                      root.pane ? root.pane.dropPath : root.path)
                    // Unknown while the listed reply is still out, because dirDev is then the directory a hover switch just left; unknown makes verbFor copy, never a move that turns into a cross-device delete.
                    destDev: Tabs.devAt(root.tabs, root.currentIndex, tab.index,
                                        root.pane && root.pane.backend && !root.pane.listInFlight ? root.pane.backend.dirDev : 0)
                    // Only an accepted enter arms the switch: Qt emits entered before it reads accepted,
                    // and a refused drag gets no exited, so the timer would otherwise never stop.
                    onEntered: function (drag) { if (drag.accepted) hoverSwitch.restart() }
                    onExited: hoverSwitch.stop()
                    onDropped: hoverSwitch.stop()
                }
                Timer {
                    id: hoverSwitch
                    interval: root.hoverSwitchMs
                    onTriggered: if (root.pane && !tab.current) Tabs.selectAt(root.pane, tab.index)
                }

                Rectangle {
                    anchors.fill: parent
                    color: tab.current ? Glass.backgroundPlane : "transparent"
                }

                // Flush on the strip's own bottom edge, replacing it rather than sitting inside the
                // plate: rendered in Quickshell on a low-chroma theme, an inset edge vanishes.
                Rectangle {
                    anchors.bottom: parent.bottom
                    anchors.left: parent.left
                    anchors.right: parent.right
                    height: Theme.accentEdge
                    color: Theme.color.accent
                    visible: tab.current
                }

                Rectangle {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    width: Theme.spacing.hairline
                    height: parent.height * 0.45
                    color: Theme.color.foreground
                    opacity: 0.12
                    visible: tab.slot < root.tabCount - 1 && !tab.current
                }

                Text {
                    id: titleText
                    anchors.left: parent.left
                    anchors.leftMargin: Theme.spacing.rowPaddingX
                    anchors.right: closeHit.left
                    anchors.verticalCenter: parent.verticalCenter
                    text: tab.title
                    color: tab.current ? Theme.color.foreground : Theme.color.muted
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                }

                Item {
                    id: closeHit
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    width: Theme.hitMin
                    height: parent.height

                    // The mark never moves and its target never shrinks; only the ink answers, so a
                    // crowded strip is no harder to hit than a tidy one.
                    Flea.Glyph {
                        anchors.centerIn: parent
                        width: Theme.chromeMarkSize
                        height: Theme.chromeMarkSize
                        name: "x"
                        color: tab.current || closeHover.hovered ? Theme.color.foreground : Theme.color.muted
                    }

                    HoverHandler { id: closeHover }
                }

                TapHandler {
                    acceptedButtons: Qt.LeftButton | Qt.MiddleButton
                    onTapped: function (eventPoint, button) {
                        if (!pane)
                            return
                        if (button === Qt.MiddleButton) {
                            Tabs.closeAt(pane, tab.index)
                            return
                        }
                        var local = closeHit.mapFromItem(tab, eventPoint.position.x, eventPoint.position.y)
                        if (local.x >= 0 && local.x <= closeHit.width && local.y >= 0 && local.y <= closeHit.height)
                            Tabs.closeAt(pane, tab.index)
                        else
                            Tabs.selectAt(pane, tab.index)
                    }
                }

                // A left-button drag reorders; a press without a move still taps above.
                // A file drag never enters here, so the hover switch answers only files.
                DragHandler {
                    acceptedButtons: Qt.LeftButton
                    target: null
                    function updateDrop() {
                        // Scene coordinates stay fixed while the preview moves the delegate beneath the pointer.
                        var pos = strip.mapFromItem(null, centroid.scenePosition.x, centroid.scenePosition.y)
                        var win = root.parent ? root.parent.mapFromItem(null, centroid.scenePosition.x, centroid.scenePosition.y) : pos
                        root.tabLiftMoved(pos.x, win.x, win.y)
                    }
                    onActiveChanged: {
                        // The threshold move precedes activation, so its centroid must be sampled here.
                        if (active) {
                            root.tabLiftBegan(tab.index)
                            updateDrop()
                        } else if (root.outActive || root.ownAccepted) {
                            // A platform drag owns its release, so no grab release reaches this handler to end the lift.
                            root.tabLiftEnded()
                        }
                    }
                    onCentroidChanged: if (active) updateDrop()
                    onGrabChanged: function (transition, point) {
                        if (transition !== PointerDevice.UngrabExclusive) return
                        // Qt deactivates before updating the centroid on release; the event point holds the release position.
                        var pos = strip.mapFromItem(null, point.scenePosition.x, point.scenePosition.y)
                        root.dragMoved(pos.x)
                        root.tabLiftEnded()
                    }
                    onCanceled: {
                        root.dragFrom = -1
                        root.dropAt = -1
                        if (!root.outActive && root.ackLiftedAt === 0) root.clearAck()
                    }
                }
            }
        }

        Item {
            id: addButton
            x: root.tabCount * root.tabWidth
            width: Theme.hitMin
            height: strip.height
            Accessible.role: Accessible.Button
            Accessible.name: "New tab"
            Accessible.onPressAction: if (pane) Tabs.openNew(pane)
            HoverHandler { cursorShape: Qt.PointingHandCursor }

            Flea.Glyph {
                anchors.centerIn: parent
                width: Theme.chromeMarkSize
                height: Theme.chromeMarkSize
                name: "plus"
                color: Theme.color.muted
            }

            TapHandler {
                acceptedButtons: Qt.LeftButton
                onTapped: if (pane) Tabs.openNew(pane)
            }
        }
    }

    // The strip catcher accepts tab drags without intercepting pointer input.
    DropArea {
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.right: parent.right
        keys: [Tabs.TAB_MIME]
        onEntered: function (drag) {
            if (!root.tabEnterOk(drag))
                drag.accepted = false
        }
        onExited: root.traceTab("leave-strip", "")
        // An own drag out and back tracks its insertion while it is over the strip.
        onPositionChanged: function (drag) {
            var info = Tabs.parseTabMime(drag.getDataAsString(Tabs.TAB_MIME))
            if (info && Tabs.isOwnTab(info) && root.outActive)
                root.dragMoved(drag.x)
        }
        onDropped: function (drop) {
            var payload = drop.getDataAsString(Tabs.TAB_MIME)
            root.traceTab("drop-strip", "empty=" + (payload.length === 0) + " len=" + payload.length)
            var info = Tabs.parseTabMime(payload)
            if (!info) {
                root.traceTab("drop-skip", "reason=strip-bad-payload")
                return
            }
            if (Tabs.isOwnTab(info)) {
                // An own-strip return reorders once before the late release clears the gesture.
                if (!root.outActive) {
                    root.traceTab("drop-skip", "reason=strip-own-inactive")
                    return
                }
                root.returnAt(Tabs.dropIndexAt(drop.x, root.tabWidth, root.tabCount))
                drop.accept(Qt.MoveAction)
                return
            }
            // The take decision answers Move at once; the peek behind it may still refuse, and then no ack goes out.
            if (Tabs.dropDecision(info, undefined, root.outActive, root.pane ? Tabs.canReceive(root.pane) : false) !== Tabs.DROP_TAKE) {
                root.traceTab("drop-skip", "reason=strip-decision-ignore")
                return
            }
            drop.accept(Qt.MoveAction)
            root.acceptTabDrop(payload, info, Tabs.dropIndexAt(drop.x, root.tabWidth, root.tabCount))
        }
    }

    // The catcher stays loaded only while an outgoing tab drag is active.
    Loader {
        id: tearPanels
        active: root.outActive
        source: "file://" + Quickshell.shellDir + "/tabtearoff.qml"
        onLoaded: {
            item.tabBar = root
            item.tabMime = Tabs.TAB_MIME
        }
    }

    Component.onCompleted: Tabs.setOwnPid(Quickshell.processId)

    // Tabs040 callout 1: the accent bar borders the ghost's leading edge through the strip's height.
    Rectangle {
        visible: root.dragFrom >= 0 && root.dropAt >= 0
        x: strip.x + root.dragTo * root.tabWidth - Theme.spacing.hairline
        width: 2 * Theme.spacing.hairline
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        color: Theme.color.accent
    }
}
