import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import "." as Flea
import "js/Jump.js" as Jump
import "js/Recent.js" as Recent

// The path bar's folder jump, the Jump board: a name typed into the bar lists matching folders from
// Flea's favourites, zoxide's ranking and the desktop's recent history in a dropdown flush under the
// field, at its width and in the menu recipe. ui/ChromeBar.qml owns the field and forwards its keys
// here; ui/js/Jump.js decides the rows. Nothing is written anywhere, and zoxide is asked once per open.
Item {
    id: root

    property bool editing: false
    // The field's line as typed; ui/js/Jump.js decides whether it is a name to jump by or a path.
    property string query: ""
    property string home: ""
    // The tallest the dropdown may be before it scrolls: the window less the chrome strip above and the
    // status bar below, both a chrome strip tall, and a gap clear of the status bar.
    readonly property real room: (root.Window.window ? root.Window.window.height : 0) - 2 * Theme.chromeHeight - Theme.spacing.gap

    // The backend's answer for this open, { favourites, zoxide, recent }, empty until it arrives.
    property var sources: ({})
    readonly property bool answered: root.sources.favourites !== undefined
    // The same answer folded once for ranking: every keystroke ranks out of this instead of folding
    // every candidate again, so a keystroke costs the alignment alone. Dropped with sources below,
    // so one open never keeps the next one's folds.
    property var prepared: null
    // Tab is the path bar's own completion, so a line it has touched is a path for the rest of the edit:
    // with two children sharing a prefix it stops short of the slash, and Enter still means ./that.
    property bool pathTyped: false
    readonly property var entries: root.editing && !root.pathTyped && root.prepared !== null
        ? Jump.rowsPrepared(root.prepared, root.query) : []
    property int cursor: -1
    readonly property bool shown: root.entries.length > 0
    // The high-water row count of this open: the Repeater holds this many delegates standing and
    // hides the tail past the entries, so a keystroke that narrows the list rebinds rows instead
    // of rebuilding them. Reset with the open below.
    property int rowHighWater: 0
    // Every ask carries a new id and take() keeps only the newest open's answers, so a slow answer to
    // an earlier ask can never land in this one, or move a cursor the user has already moved.
    property int asked: 0
    // An open whose history is stale asks twice: a provisional ask with what is kept, so the
    // dropdown fills from the backend's answer instead of waiting out the history read, and the whole
    // ask under a new id once the read lands. take() shows the provisional rows but only the whole
    // answer spends a held Enter or settles wholeTaken; a provisional answer landing after the whole
    // one is dropped like a stale one. The whole ask waits for the provisional answer as well as the
    // read, so the two backend calls never overlap on the backend's one zoxide slot.
    property int provisionalId: 0
    property bool wholeTaken: false
    // The provisional answer has landed, and the whole ask has been sent; one whole ask per open.
    property bool provTaken: false
    property bool wholeAsked: false
    // Enter on a name before this open's answer is in: held, then taken by the answer, so the same keys
    // open the same folder however fast they were typed.
    property bool enterWaiting: false
    // Where any row last saw the pointer, so rows changing under a resting pointer never move the cursor; see ui/MenuRow.qml.
    property point pointerGlobal: Qt.point(-1, -1)
    // Rows appearing under a resting pointer must not take the cursor either: moves are ignored
    // until the first frame after rows appear, the way ui/ContextMenu.qml settles them.
    property bool pointerSettling: false
    Connections {
        target: root.pointerSettling ? root.Window.window : null
        function onAfterAnimating() { root.pointerSettling = false }
    }

    // ui/WindowBody.qml carries these to the pane's backend and back, the way it carries the bar's Tab.
    // ranking is the provisional ask this whole ask follows, or 0: the backend answers it from that ask's zoxide run.
    signal requested(int id, int ranking, var favourites, var recent)
    signal chosen(string path)
    // Enter that the dropdown did not take after all: nothing matched, so the bar resolves the line as a path.
    signal declined()
    // A click on the dropdown's own padding: the bar closes with nothing opened, the menu recipe.
    signal dismissed()
    // For tests/jump-ui.sh: whether the dropdown's frame stands, and the frame itself while it does.
    readonly property bool dropBuilt: drop.item !== null
    readonly property var dropItem: drop.item

    // The keys arrive through the field's Keys.forwardTo, which skips an invisible item, so this stays
    // visible for the whole edit and hands back every key it does not use; only the dropdown unloads.
    visible: root.editing
    // The parent is the bar's path slot, which stops a hairline above the strip's bottom edge.
    y: root.parent ? root.parent.height + Theme.spacing.hairline : 0
    width: root.parent ? root.parent.width : 0
    height: drop.item ? drop.item.height : 0

    onEntriesChanged: {
        root.rowHighWater = Math.max(root.rowHighWater, root.entries.length)
        root.cursor = Jump.step(root.entries, -1, 1)
    }
    // The rows live behind the Loader below, so a cursor moved while they stand unbuilt asks nothing.
    onCursorChanged: { if (drop.item) drop.item.reveal(root.cursor) }
    onShownChanged: {
        if (!root.shown) {
            return
        }
        root.pointerGlobal = Qt.point(-1, -1)
        root.pointerSettling = true
        // An appearance that changes nothing drawn schedules no frame, so it asks for the one
        // that ends the settle, the way ui/ContextMenu.qml's place() does.
        if (root.Window.window) root.Window.window.update()
    }
    onEditingChanged: {
        root.sources = ({})
        root.rowHighWater = 0
        root.prepared = null
        root.enterWaiting = false
        root.pathTyped = false
        root.provisionalId = 0
        root.wholeTaken = false
        root.provTaken = false
        root.wholeAsked = false
        root.pointerGlobal = Qt.point(-1, -1)
        root.pointerSettling = false
        root.asked += 1
        if (!root.editing) {
            return
        }
        if (root.historyKept && root.historyReadAt === root.historyChanges) {
            root.wholeAsked = true
            root.ask(0)
        } else {
            root.provisionalId = root.asked
            root.ask(0)
            root.readHistory()
        }
    }

    // The recent history, read once and kept until the file changes, so an open with it unchanged asks at
    // once: re-reading 5,000 bookmarks on every open cost the dropdown some 70 ms, measured.
    property var recentPaths: []
    property bool historyKept: false
    property bool historyReading: false
    // Every change the watcher has reported, and the count the kept paths were read at.
    property int historyChanges: 0
    property int historyReadAt: -1
    // How many times the history has been parsed; tests/jump-ui.sh reads it.
    property int historyReads: 0

    function ask(ranking) {
        root.requested(root.asked, ranking, root.favouritePaths(), root.recentPaths)
    }

    function askWhole() {
        // One whole ask per open, only once the provisional answer and the history read are both in,
        // so the two backend calls never overlap on the backend's one zoxide slot. The read's own
        // staleness is the next open's business: this one asks with the newest paths it has.
        if (!root.editing || root.wholeAsked || !root.provTaken) {
            return
        }
        if (root.provisionalId !== 0 && !(root.historyKept && !root.historyReading)) {
            return
        }
        root.wholeAsked = true
        root.asked += 1
        root.ask(root.provisionalId)
    }

    // The watch starts before the read, so a change landing while it runs is counted and read next time.
    function readHistory() {
        root.historyReading = true
        root.historyReadAt = root.historyChanges
        watcher.path = Recent.historyPath(Quickshell.env("XDG_DATA_HOME"), Quickshell.env("HOME"))
        recent.active = true
        recent.item.refresh()
    }

    // Watching only: it never loads the file, and it reports a rename over it, a delete and a re-create alike.
    FileView {
        id: watcher
        preload: false
        watchChanges: true
        onFileChanged: root.historyChanges += 1
    }

    // The backend's jumped line; one for another open, or landing after the bar closed, is dropped.
    // A provisional answer draws its rows but never spends a held Enter and never settles the open.
    function take(id, favourites, zoxide, recentFolders, frecency) {
        if (!root.editing) {
            return
        }
        var provisional = id === root.provisionalId && root.provisionalId !== 0 && !root.wholeTaken
        if (id !== root.asked && !provisional) {
            return
        }
        var answer = { favourites: favourites, zoxide: zoxide, recent: recentFolders, frecency: frecency || ({}) }
        root.sources = answer
        root.prepared = Jump.prepare(answer, root.home)
        if (provisional) {
            root.provTaken = true
            root.askWhole()
            return
        }
        root.wholeTaken = true
        if (root.enterWaiting) {
            root.enterWaiting = false
            root.enter()
        }
    }

    // Enter itself: the cursor row when there is one, and otherwise the line as a path, as before the jump.
    function enter() {
        if (root.cursor >= 0) {
            root.chosen(root.entries[root.cursor].path)
        } else {
            root.declined()
        }
    }

    function favouritePaths() {
        var records = Flea.Favourites.records
        var out = []
        for (var i = 0; i < records.length; i++) {
            out.push(String(records[i].path || ""))
        }
        return out
    }

    // The picker's own reader of recently-used.xbel, built for a read and dropped after it, so a window whose
    // bar is never typed into loads no XML module at all.
    Loader {
        id: recent
        active: false
        source: "PickerRecent.qml"
    }

    Connections {
        target: recent.item
        function onRefreshed() {
            root.recentPaths = recent.item.paths
            root.historyKept = true
            root.historyReading = false
            root.historyReads += 1
            // The parsed model goes once its newest paths are kept, which bounds what stays in memory at
            // Recent.LIMIT paths; later, because the reader is the one emitting this signal.
            Qt.callLater(function () { if (!root.historyReading) recent.active = false })
            // The whole ask waits for the provisional answer too, which take() records; either order
            // of the two landings converges on it, and a closed bar asks nothing more.
            root.askWhole()
        }
    }

    // The backend answers inside its own zoxide and stat budgets, docs/protocol.md "jump"; one that never
    // answers at all, a backend gone, must not hold an Enter for good, so past this the line is a path.
    readonly property int answerLimitMs: 4000
    Timer {
        id: enterLimit
        interval: root.answerLimitMs
        running: root.enterWaiting
        onTriggered: {
            root.enterWaiting = false
            root.declined()
        }
    }

    Keys.onPressed: function (event) {
        // A held Enter stands for the line as it was, so nothing typed after it changes what it opens; esc still closes.
        if (root.enterWaiting && event.key !== Qt.Key_Escape) {
            event.accepted = true
            return
        }
        if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) {
            root.pathTyped = true
            event.accepted = false
            return
        }
        if (root.shown && (event.key === Qt.Key_Down || event.key === Qt.Key_Up)) {
            root.cursor = Jump.step(root.entries, root.cursor, event.key === Qt.Key_Down ? 1 : -1)
            event.accepted = true
            return
        }
        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
            if (root.shown) {
                root.enter()
                event.accepted = true
                return
            }
            // Held until the whole answer, not the provisional one, so the same keys open the same
            // folder however fast they were typed; a visible row above already took the press itself.
            if ((!root.answered || !root.wholeTaken) && !root.pathTyped && Jump.isQuery(root.query)) {
                root.enterWaiting = true
                event.accepted = true
                return
            }
        }
        event.accepted = false
    }

    readonly property int drawnRows: drop.item === null ? 0 : Math.min(root.entries.length, Math.ceil((drop.item.scrollY + drop.item.height - 2 * Theme.spacing.rowPaddingY) / Theme.rowHeight)) - Math.floor(drop.item.scrollY / Theme.rowHeight)
    // The dropdown's frame, built only while rows show: the scroll body, its window listener,
    // both fades and the rows are nothing the first frame shows.
    Loader {
        id: drop
        active: root.shown
        sourceComponent: dropFrame
    }

    Component {
        id: dropFrame
        Rectangle {
            id: frame
            width: root.width
            height: Math.max(0, Math.min(rows.implicitHeight + 2 * Theme.spacing.rowPaddingY, root.room))
            color: Theme.color.surface
            border.width: Theme.spacing.hairline
            border.color: Theme.color.mutedFrame
            radius: Style.cornerRadius

            function reveal(index) { scroll.reveal(rowItems.itemAt(index)) }
            // For tests/jump-ui.sh: how many delegates stand, and the one at an index.
            readonly property int liveCount: rowItems.count
            // For the picker's path shots: how far the rows have scrolled.
            readonly property real scrollY: scroll.contentY
            function liveAt(index) { return rowItems.itemAt(index) }
            Component.onCompleted: reveal(root.cursor)

            // The ground takes every pointer event the rows leave, so nothing reaches the listing beneath.
            MouseArea {
                id: ground
                anchors.fill: parent
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                hoverEnabled: true
                // The sink records the point, so rows appearing under a resting pointer compare
                // against where it already is, the menu's own recipe.
                onEntered: root.pointerGlobal = ground.mapToItem(null, ground.mouseX, ground.mouseY)
                onPositionChanged: function (mouse) { root.pointerGlobal = ground.mapToItem(null, mouse.x, mouse.y) }
                // The recipe's close: a click that hit no row closes the bar with nothing opened.
                onClicked: root.dismissed()
                onWheel: function (wheel) { wheel.accepted = true }
            }

            Flea.CardScroll {
                id: scroll
                // The dropdown steps the highlight like a menu: one row a notch, one row per
                // row height of gained touchpad travel. The cursor's reveal() follows.
                highlightSteps: true
                stepBy: function (delta) { root.cursor = Jump.step(root.entries, root.cursor, delta) }
                anchors.fill: parent
                anchors.topMargin: Theme.spacing.rowPaddingY
                anchors.bottomMargin: Theme.spacing.rowPaddingY

                Column {
                    id: rows
                    width: parent.width

                    Repeater {
                        id: rowItems
                        // The high-water count, so a keystroke that narrows the list leaves every
                        // delegate standing: rows rebind to entries[index] instead of rebuilding, and
                        // the tail past the entries hides. The max against the entries covers a growth
                        // the high-water handler has not recorded yet, whichever runs first.
                        model: Math.max(root.rowHighWater, root.entries.length)
                        // The menu's own row, so the lift, the mark slot and the pointer rules are the context menu's;
                        // the label is the path the chrome would draw, laid over its empty one.
                        delegate: Flea.MenuRow {
                            id: row
                            required property int index
                            // The row at this slot of the newest answer, undefined mid-swap or past it.
                            // A hidden tail keeps its delegate standing at zero height, because an
                            // invisible item still holds its Column position.
                            readonly property var rowData: root.entries[row.index]
                            readonly property bool rowStanding: row.index < root.entries.length
                            visible: row.rowStanding
                            height: row.rowStanding ? Theme.rowHeight : 0
                            width: rows.width
                            entry: ({ glyph: "folder", label: "", hint: "" })
                            current: root.cursor === row.index
                            lastPointerGlobal: root.pointerGlobal
                            onPointerSeen: function (at) { root.pointerGlobal = at }
                            onPointerMoved: { if (!root.pointerSettling) root.cursor = row.index }
                            onActivated: { if (row.rowData !== undefined) root.chosen(row.rowData.path) }

                            Flea.JumpPath {
                                entry: row.rowData !== undefined ? row.rowData : ({})
                                anchors.left: parent.left
                                anchors.leftMargin: Theme.spacing.rowPaddingX + row.slotSize + Theme.spacing.gap
                                anchors.right: parent.right
                                anchors.rightMargin: Theme.spacing.rowPaddingX
                                anchors.verticalCenter: parent.verticalCenter
                            }
                        }
                    }
                }
            }

            Flea.MenuEdgeFade {
                anchors.top: parent.top
                visible: scroll.contentY > 0
            }

            Flea.MenuEdgeFade {
                anchors.bottom: parent.bottom
                visible: scroll.contentY + scroll.height < scroll.contentHeight
                rotation: 180
            }
        }
    }
}
