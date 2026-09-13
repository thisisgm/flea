import QtQuick
import qs.Commons
import "." as Flea
import "js/Keymap.js" as Keymap
import "js/Menu.js" as Menu

// A plain overlay, not a QQC Popup: the one Controls import cost 10 ms of warm startup.
Item {
    id: root

    // Fires with the row's own action string ("open", "trash"); a chosen Taildrop peer fires
    // "taildrop:<peerId>" instead, so one signal covers both without a second wire. The header's
    // rows fire "col:<key>" and "toggleHidden", routed in ui/Pane.qml's onChosen.
    signal chosen(string action)
    signal refused(string reason)
    signal snapshotRequested()

    property bool opened: false
    // Driven from ui/Pane.qml's own state, so this file owns no hidden-file logic itself.
    property bool showHidden: false
    // The application name ui/Opener.qml resolved for the cursor row, shown muted beside "Open".
    // [{id, label}], the reachable Taildrop targets; installed providers keep their disabled reason.
    property var taildropPeers: []
    property bool taildropInstalled: false
    property string taildropReason: ""
    property bool providersRefreshing: false
    // The archive formats this box actually probed, and whether a converter is installed at all.
    property var archiveFormats: []
    property bool canConvert: false
    property bool canExtract: false
    property bool clipboardAvailable: false
    // Whether the cursor row is an archive, and whether it is an image; both decided client-side.
    property bool rowIsArchive: false
    property bool rowIsImage: false
    property int rowMode: 0
    property int selectionCount: 0
    // OpenWith.html's flyout rows, filled by ui/PaneMenuActions.qml when the registry answers.
    property var openWithApps: []
    property bool openWithLoaded: false
    // The actual account directory is available only after a fresh provider status and identity check.
    property string dropboxPath: ""
    property bool dropboxInstalled: false
    property string dropboxReason: ""
    // A row already inside the account directory offers a share link instead of another move.
    property bool rowInDropbox: false
    // False on a listing's empty space, where Menus.html's background column is what opens instead.
    // openBackground() is its only writer and openAt() puts it back, because one instance serves both.
    property bool hasRow: true
    property string selectionIdentity: ""
    property string openedIdentity: ""
    // The owner intersects the live monitor work area with this application's viewport.
    property rect workArea: Qt.rect(0, 0, root.width, root.height)
    readonly property real workAreaInset: Theme.space(4)

    // The rail's own rows when ui/Sidebar.qml raised this menu, empty when the listing did. One
    // instance serves both: a second one in this tree takes the keyboard from the list, see AGENTS.md.
    property var railEntries: []
    // The key naming the rail row those rows belong to, see ui/js/Mounts.js "railKey": the rail
    // rebuilds on a poll, so an index would name a different row by the time one is chosen.
    property string railKey: ""
    readonly property bool forRail: root.railEntries.length > 0
    signal railChosen(string action, string key)

    // ui/Header.qml's own entrance, the third face of this one instance: openForHeader() flips the
    // entries to ui/js/Menu.js headerEntries (the column toggles and the hidden toggle, built from
    // qs module ViewState's hidden columns and the pane's showHidden), and every row flows back
    // through chosen() like the listing's own. A row's chosen verb routes by prefix in ui/Pane.qml.
    property bool forHeader: false
    function openForHeader(scenePoint) {
        root.forHeader = true
        root.place(scenePoint)
    }

    // The pane keeps its Keys handler on the list, so the menu has to hand focus back on close.
    property Item focusHolder: null
    // The original focus item may disappear while the menu is open; the owning view is the fallback.
    property Item focusOwner: null
    readonly property bool keyboardFocused: keyCatcher.activeFocus

    // The keyboard-highlighted top-level row, and which row's flyout is open beside it, or -1.
    property int cursor: 0
    property int openSubmenuRow: -1
    property int submenuCursor: 0
    readonly property bool submenuOpen: root.openSubmenuRow >= 0
    // The glyph every open flyout row draws, read back so a test can name it without OCR.
    function submenuGlyphs() {
        if (!root.submenuOpen)
            return ""
        var mark = Menu.submenuGlyph(root.entries[root.openSubmenuRow].action)
        var out = []
        for (var i = 0; i < root.submenuEntries.length; i++) {
            // What the row draws, not what the flyout defaults to: an Open with row carries its own
            // glyph or an application icon, and reporting the default made a check measure nothing.
            var row = root.submenuEntries[i]
            out.push(row.separator === true ? "" : row.icon ? "icon"
                   : row.glyph !== undefined ? row.glyph : mark)
        }
        return out.join("|")
    }

    // The entries the open flyout draws, which belong to the row that opened it.
    readonly property var submenuEntries: root.submenuOpen && root.entries[root.openSubmenuRow]
        ? root.entries[root.openSubmenuRow].submenu : []

    // The row list this menu currently offers; a test reads this back through shell.qml's IPC.
    property var entries: []

    // The construction lives in ui/js/Menu.js now, so the rows are unit-testable without a window:
    // listingEntries(p) builds the listing's rows from the pane's state, headerEntries() the column
    // titles' own rows on a right click (see ui/Header.qml), and this file only routes between them.
    function buildEntries() {
        // Which release a rail row offers is the rail's knowledge, not the listing's, so the rail
        // hands its rows in already built; see ui/js/Mounts.js "railMenu".
        if (root.forRail)
            return root.railEntries
        if (root.forHeader)
            return Menu.headerEntries(ViewState.hiddenCols, root.showHidden)
        return Menu.listingEntries({
            showHidden: root.showHidden,
            hasRow: root.hasRow,
            rowInDropbox: root.rowInDropbox,
            dropboxPath: root.dropboxPath,
            dropboxInstalled: root.dropboxInstalled,
            dropboxReason: root.dropboxReason,
            taildropPeers: root.taildropPeers,
            taildropInstalled: root.taildropInstalled,
            taildropReason: root.taildropReason,
            providersRefreshing: root.providersRefreshing,
            archiveFormats: root.archiveFormats,
            rowIsArchive: root.rowIsArchive,
            rowIsImage: root.rowIsImage,
            canConvert: root.canConvert,
            canExtract: root.canExtract,
            clipboardAvailable: root.clipboardAvailable,
            openWithApps: root.openWithApps,
            openWithLoaded: root.openWithLoaded,
            rowMode: root.rowMode,
            selectionCount: root.selectionCount,
            // The Menus settings section's stored set; ui/js/Menu.js applyHidden is what reads it.
            hiddenActions: ViewState.menuHidden
        })
    }

    // The row item at an index, for ui/Ipc.qml: a driven test clicks a menu row without deriving
    // its geometry from a row count the Menus settings can now change under it.
    function itemFor(index) { return menuRows.itemAt(index) }
    function submenuItemFor(index) { return subRows.itemAt(index) }
    readonly property var frameItem: frame
    readonly property var submenuFrameItem: flyout

    // A separator is never the cursor, so both key steps and the opening cursor skip over one.
    function stepCursor(from, delta) {
        var i = from + delta
        while (i >= 0 && i < root.entries.length) {
            if (root.entries[i].separator !== true && root.entries[i].disabled !== true)
                return i
            i += delta
        }
        return from
    }

    // The same rule inside a flyout: OpenWith.html's tail row sits under its own separator, and a
    // separator is not somewhere the cursor may rest.
    function stepSubmenu(from, delta) {
        var rows = root.submenuEntries, i = from + delta
        while (i >= 0 && i < rows.length) {
            if (rows[i].separator !== true && rows[i].disabled !== true) return i
            i += delta
        }
        return from
    }

    function firstRow() {
        return root.stepCursor(-1, 1)
    }

    anchors.fill: parent
    visible: root.opened
    z: 1

    // Takes a point in scene coordinates and keeps the whole menu inside the pane it belongs to.
    function openAt(scenePoint) {
        root.clearRail()
        root.forHeader = false
        root.hasRow = true
        root.place(scenePoint)
    }

    // The listing's other entrance, from a right click that landed on no row at all: ui/List.qml,
    // ui/GridArea.qml and ui/ColumnPane.qml each answer for their own empty space, and this one
    // instance then draws ui/js/Menu.js backgroundEntries instead of the cursor row's.
    function openBackground(scenePoint) {
        root.clearRail()
        root.forHeader = false
        root.hasRow = false
        root.place(scenePoint)
    }

    // ui/Sidebar.qml's own entrance to this same menu: the rail hands in its rows and the key that
    // names the row they came from, and a rail row with nothing to release opens no menu at all.
    function openForRail(key, entries, scenePoint) {
        if (!entries || entries.length === 0)
            return
        root.railKey = key
        root.railEntries = entries
        root.place(scenePoint)
    }

    // Cleared on both ends: a rail entry left standing would put Eject on a listing row's menu.
    function clearRail() {
        root.railEntries = []
        root.railKey = ""
    }

    // Where the menu was asked to open, in this item's own coordinates; clampFrame runs twice on it.
    property real placeX: 0
    property real placeY: 0

    // A Column hands its implicitHeight to the frame one polish after its model changes, so the
    // height place() reads is still the menu that was open before this one. Clamping again on the
    // real height lands before the first paint, so no menu is placed against another's size.
    function clampFrame() {
        frame.x = root.workArea.x + Menu.clamp(root.placeX - root.workArea.x, frame.width, root.workArea.width)
        frame.y = root.workArea.y + root.workAreaInset
                + Menu.clamp(root.placeY - root.workArea.y - root.workAreaInset,
                             frame.height, Math.max(0, root.workArea.height - 2 * root.workAreaInset))
    }

    // Where any row last saw the pointer, so a row can tell a pointer moving onto it from a row
    // arriving under a pointer that is standing still. Forgotten each time the menu is placed.
    property point pointerGlobal: Qt.point(-1, -1)

    function place(scenePoint) {
        if (!root.opened)
            root.focusHolder = root.Window.window ? root.Window.window.activeFocusItem : null
        root.pointerGlobal = Qt.point(-1, -1)
        var point = root.mapFromItem(null, scenePoint)
        root.placeX = point.x
        root.placeY = point.y
        root.entries = root.buildEntries()
        root.openedIdentity = root.selectionIdentity
        scroll.contentY = 0
        root.clampFrame()
        root.cursor = root.firstRow()
        root.openSubmenuRow = -1
        root.submenuCursor = 0
        root.opened = true
        if (root.hasRow && !root.forRail && !root.forHeader) root.snapshotRequested()
        keyCatcher.forceActiveFocus()
    }
    onWorkAreaChanged: if (root.opened) root.clampFrame()
    onCursorChanged: scroll.reveal(menuRows.itemAt(root.cursor))
    onSubmenuCursorChanged: subScroll.reveal(subRows.itemAt(root.submenuCursor))

    // Every wheel scroll calls this, so a shut menu costs nothing and never touches focus.
    function close() {
        if (!root.opened)
            return
        root.opened = false
        root.openSubmenuRow = -1
        root.clearRail()
        var holder = root.focusHolder && root.focusHolder.visible && root.focusHolder.enabled ? root.focusHolder : root.focusOwner
        if (holder)
            holder.forceActiveFocus()
    }

    // The menu closes before the action runs, so it never hangs over the listing that action opened.
    function choose(action) {
        if (!root.validateChoice(action, "")) return
        // Both read before close(), which is what clears them.
        var key = root.railKey
        var rail = root.forRail
        root.close()
        if (rail) {
            root.railChosen(action, key)
            return
        }
        root.chosen(action)
    }

    // One signal covers every submenu: the row's own action, a colon, and the entry chosen inside it.
    function chooseSub(id) {
        var entry = root.entries[root.openSubmenuRow]
        if (!entry || !root.validateChoice(entry.action, id)) return
        root.close()
        if (entry)
            root.chosen(entry.action + ":" + id)
    }

    function openSubmenu(index) {
        if (!Menu.hasSubmenu(root.entries[index]) || root.entries[index].disabled === true) return
        root.openSubmenuRow = index
        root.submenuCursor = 0
        subScroll.contentY = 0
    }

    // Fresh capabilities use the normal inventory; selection stays on its action and placement uses the existing clamp.
    function refreshProviderRows() {
        if (!root.opened || root.forRail || root.forHeader) return
        var next = root.buildEntries()
        var selection = Menu.refreshedCursor(root.entries, next, root.cursor, root.openSubmenuRow, root.submenuCursor)
        root.entries = next
        root.cursor = selection.cursor
        root.openSubmenuRow = selection.submenuRow
        root.submenuCursor = selection.submenuCursor
        Qt.callLater(function() { if (root.opened) scroll.reveal(menuRows.itemAt(root.cursor)) })
    }

    // Rebuild only to validate; rows stay fixed while the menu is open under the pointer.
    function validateChoice(action, subId) {
        var identityChanged = !root.forRail && !root.forHeader && root.hasRow
                              && root.openedIdentity !== root.selectionIdentity
        var live = root.buildEntries()
        for (var i = 0; !identityChanged && i < live.length; i++) {
            var entry = live[i]
            if (entry.action !== action || entry.disabled === true) continue
            if (!subId) return true
            var sub = entry.submenu || []
            for (var j = 0; j < sub.length; j++)
                if (sub[j].id === subId && sub[j].disabled !== true) return true
        }
        root.close()
        root.refused(identityChanged ? "Selected items changed; reopen the menu."
                                     : "That action is no longer available; reopen the menu.")
        return false
    }

    // Rows above the open one are a mix of full rows and separators, so the offset is summed, not multiplied.
    function submenuOffset() {
        var y = 0
        for (var i = 0; i < root.openSubmenuRow; i++)
            y += root.entries[i].separator === true ? separatorProbe.separatorHeight : Theme.rowHeight
        return y - scroll.contentY
    }

    // One row off the model, only so the two heights above are read from MenuRow rather than repeated here.
    Flea.MenuRow {
        id: separatorProbe
        visible: false
        entry: ({ separator: true })
    }

    // The ground owns every pointer event outside the rows: hover stops here, the wheel is swallowed, and the click that closes is taken on release so the row beneath never sees a press the close would have handed it.
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        hoverEnabled: true
        onClicked: root.close()
        onWheel: function (wheel) { wheel.accepted = true }
    }

    Rectangle {
        id: frame
        width: Math.max(0, Math.min(root.workArea.width, Theme.menuWidth))
        // The vertical inset keeps the first and last row's square highlight off the rounded corners.
        height: Math.max(0, Math.min(rows.implicitHeight + 2 * Theme.spacing.rowPaddingY,
                                    root.workArea.height - 2 * root.workAreaInset))
        // The height this menu is actually going to have, arriving after place() has already run.
        onHeightChanged: if (root.opened) root.clampFrame()
        color: Theme.color.surface
        border.width: Theme.spacing.hairline
        border.color: Theme.color.muted
        // Mirrors hyprland decoration:rounding, same as NetworkDialog; 0 on a stock box stays square.
        radius: Style.cornerRadius

        Flea.CardScroll {
            id: scroll
            anchors.fill: parent
            anchors.topMargin: Theme.spacing.rowPaddingY
            anchors.bottomMargin: Theme.spacing.rowPaddingY
        Column {
            id: rows
            width: parent.width

            Repeater {
                id: menuRows
                model: root.entries
                delegate: Flea.MenuRow {
                    id: row
                    required property var modelData
                    required property int index
                    width: rows.width
                    entry: row.modelData
                    compact: root.forRail && root.railKey !== "trash" && root.railKey !== "trashSelection"
                    current: !root.submenuOpen && root.cursor === row.index
                    lastPointerGlobal: root.pointerGlobal
                    onPointerSeen: function (at) { root.pointerGlobal = at }
                    onPointerMoved: {
                        root.cursor = row.index
                        if (Menu.hasSubmenu(row.modelData)) root.openSubmenu(row.index)
                        else root.openSubmenuRow = -1
                    }
                    onActivated: {
                        if (Menu.hasSubmenu(row.modelData))
                            root.openSubmenu(row.index)
                        else
                            root.choose(row.modelData.action)
                    }
                }
            }
        }
        }
        Rectangle {
            anchors.top: parent.top
            width: parent.width
            height: Theme.spacing.gap
            visible: scroll.contentY > 0
            gradient: Gradient {
                GradientStop { position: 0; color: Theme.color.surface }
                GradientStop { position: 1; color: Qt.rgba(Theme.color.surface.r, Theme.color.surface.g, Theme.color.surface.b, 0) }
            }
        }
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: Theme.spacing.gap
            visible: scroll.contentY + scroll.height < scroll.contentHeight
            rotation: 180
            gradient: Gradient {
                GradientStop { position: 0; color: Theme.color.surface }
                GradientStop { position: 1; color: Qt.rgba(Theme.color.surface.r, Theme.color.surface.g, Theme.color.surface.b, 0) }
            }
        }
    }

    // The flyout: a second frame beside whichever row opened it, only while one has.
    Rectangle {
        id: flyout
        visible: root.submenuOpen
        x: frame.x + frame.width + width <= root.workArea.x + root.workArea.width
           ? frame.x + frame.width : Math.max(root.workArea.x, frame.x - width)
        // peers.y already carries the inset, so the flyout frame itself stays on the row grid.
        y: Math.max(root.workArea.y + root.workAreaInset,
                    Math.min(frame.y + root.submenuOffset(), root.workArea.y + root.workArea.height - root.workAreaInset - height))
        width: Math.max(0, Math.min(Theme.menuWidth, root.workArea.width))
        height: Math.max(0, Math.min(peers.implicitHeight + 2 * Theme.spacing.rowPaddingY,
                                    root.workArea.height - 2 * root.workAreaInset))
        color: Theme.color.surface
        border.width: Theme.spacing.hairline
        border.color: Theme.color.muted
        radius: Style.cornerRadius

        Flea.CardScroll {
            id: subScroll
            anchors.fill: parent
            anchors.topMargin: Theme.spacing.rowPaddingY
            anchors.bottomMargin: Theme.spacing.rowPaddingY
        Column {
            id: peers
            width: parent.width

            Repeater {
                id: subRows
                model: root.submenuEntries
                delegate: Flea.MenuRow {
                    id: subRow
                    required property var modelData
                    required property int index
                    width: peers.width
                    // Which mark a whole flyout draws is ui/js/Menu.js submenuGlyph's to say, so the
                    // read-back submenuGlyphs() above and the drawn row cannot answer differently.
                    // A flyout row may carry its own mark and caption: OpenWith.html rides each
                    // application's own Icon in the mark slot and puts "default" in the hint slot,
                    // and its tail row sits under a separator. Every other flyout keeps one glyph.
                    entry: ({ label: subRow.modelData.label, action: "",
                              disabled: subRow.modelData.disabled === true,
                              separator: subRow.modelData.separator === true,
                              hint: subRow.modelData.hint,
                              icon: subRow.modelData.icon,
                              glyph: subRow.modelData.glyph !== undefined ? subRow.modelData.glyph
                                   : Menu.submenuGlyph(root.entries[root.openSubmenuRow].action) })
                    current: root.submenuCursor === subRow.index
                    onPointerMoved: if (subRow.modelData.separator !== true) root.submenuCursor = subRow.index
                    onActivated: root.chooseSub(subRow.modelData.id)
                }
            }
        }
        }
    }

    // One focus catcher for the whole menu: real QML focus never moves into the Repeater rows
    // themselves, so every key lands here regardless of which level is open. They arrive through
    // keys.toml's own table, so j and k step this list the way they step every other one.
    Item {
        id: keyCatcher
        anchors.fill: parent
        focus: true

        Keys.onPressed: function (event) {
            var action = Keymap.lookup(event.key, event.text, event.modifiers, "menu")
            event.accepted = true
            if (action === "escape") {
                if (root.submenuOpen)
                    root.openSubmenuRow = -1
                else
                    root.close()
                event.accepted = true
                return
            }
            if (action === "cursorDown") {
                if (root.submenuOpen)
                    root.submenuCursor = root.stepSubmenu(root.submenuCursor, 1)
                else
                    root.cursor = root.stepCursor(root.cursor, 1)
                event.accepted = true
                return
            }
            if (action === "cursorUp") {
                if (root.submenuOpen)
                    root.submenuCursor = root.stepSubmenu(root.submenuCursor, -1)
                else
                    root.cursor = root.stepCursor(root.cursor, -1)
                event.accepted = true
                return
            }
            if (action === "parent") { root.openSubmenuRow = -1; return }
            if (action === "menuRight") { root.openSubmenu(root.cursor); return }
            if (action === "open" || action === "preview") {
                if (root.submenuOpen) {
                    var sub = root.submenuEntries[root.submenuCursor]
                    if (sub)
                        root.chooseSub(sub.id)
                } else {
                    var entry = root.entries[root.cursor]
                    if (Menu.hasSubmenu(entry))
                        root.openSubmenu(root.cursor)
                    else if (entry && entry.separator !== true)
                        root.choose(entry.action)
                }
                event.accepted = true
            }
        }
    }
}
