import QtQuick
import qs.Commons
import "." as Flea
import "js/Archive.js" as Archive
import "js/Keymap.js" as Keymap
import "js/Menu.js" as Menu

// A plain overlay, not a QQC Popup: the one Controls import cost 10 ms of warm startup.
Item {
    id: root

    // Fires with the row's own action string ("open", "trash"); a chosen Taildrop peer fires
    // "taildrop:<peerId>" instead, so one signal covers both without a second wire.
    signal chosen(string action)

    property bool opened: false
    // Driven from ui/Pane.qml's own state, so this file owns no hidden-file logic itself.
    property bool showHidden: false
    // [{id, label}], the reachable Taildrop targets; empty self-hides the whole row, see ui/Taildrop.qml.
    property var taildropPeers: []
    // The archive formats this box actually probed, and whether a converter is installed at all.
    property var archiveFormats: []
    property bool canConvert: false
    // Whether the cursor row is an archive, and whether it is an image; both decided client-side.
    property bool rowIsArchive: false
    property bool rowIsImage: false
    // Empty until the stock Dropbox service is installed and authenticated, which is what gates the row.
    property string dropboxPath: ""
    // True when the cursor row already lives under ~/Dropbox, where a share link is the useful action.
    property bool rowInDropbox: false
    // False on a listing's empty space, where only the two rows that need no row make sense.
    property bool hasRow: true

    // The rail's own rows when ui/Sidebar.qml raised this menu, empty when the listing did. One
    // instance serves both: a second one in this tree takes the keyboard from the list, see AGENTS.md.
    property var railEntries: []
    // The key naming the rail row those rows belong to, see ui/js/Mounts.js "railKey": the rail
    // rebuilds on a poll, so an index would name a different row by the time one is chosen.
    property string railKey: ""
    readonly property bool forRail: root.railEntries.length > 0
    signal railChosen(string action, string key)

    // The pane keeps its Keys handler on the list, so the menu has to hand focus back on close.
    property Item focusHolder: null

    // The keyboard-highlighted top-level row, and which row's flyout is open beside it, or -1.
    property int cursor: 0
    property int openSubmenuRow: -1
    property int submenuCursor: 0
    readonly property bool submenuOpen: root.openSubmenuRow >= 0
    // The glyph every open flyout row draws, read back so a test can name it without OCR.
    function submenuGlyphs() {
        if (!root.submenuOpen)
            return ""
        var mark = root.entries[root.openSubmenuRow].action === "taildrop" ? "server" : "archive"
        var out = []
        for (var i = 0; i < root.submenuEntries.length; i++)
            out.push(mark)
        return out.join("|")
    }

    // The entries the open flyout draws, which belong to the row that opened it.
    readonly property var submenuEntries: root.submenuOpen && root.entries[root.openSubmenuRow]
        ? root.entries[root.openSubmenuRow].submenu : []

    // The row list this menu currently offers; a test reads this back through shell.qml's IPC.
    readonly property var entries: root.buildEntries()

    // The canvas's own order and grouping. Compress, Extract, Convert and Move to Dropbox belong
    // between Duplicate and Taildrop and arrive with their own plans; a row whose action does
    // nothing is worse than a row that is not there yet.
    function buildEntries() {
        // Which release a rail row offers is the rail's knowledge, not the listing's, so the rail
        // hands its rows in already built; see ui/js/Mounts.js "railMenu".
        if (root.forRail)
            return root.railEntries
        var out = []
        if (root.hasRow) {
            out.push({ label: "Open", action: "open", glyph: "folder-open" })
            out.push({ separator: true })
            out.push({ label: "Rename", action: "rename", glyph: "rename" })
            out.push({ label: "Duplicate", action: "duplicate", glyph: "file-plus" })
            var ops = []
            // The submenu is exactly the table the backend probed, so a box with no tool offers nothing.
            if (root.archiveFormats.length > 0)
                ops.push({ label: "Compress", action: "compress", glyph: "archive",
                           submenu: Archive.formatEntries(root.archiveFormats) })
            if (root.rowIsArchive)
                ops.push({ label: "Extract", action: "extract", glyph: "archive-out" })
            if (root.canConvert && root.rowIsImage)
                ops.push({ label: "Convert", action: "convert", glyph: "sliders" })
            if (ops.length > 0) {
                out.push({ separator: true })
                for (var i = 0; i < ops.length; i++) out.push(ops[i])
            }
            var share = []
            if (root.taildropPeers.length > 0)
                share.push({ label: "Send with Taildrop", action: "taildrop", mark: "tailscale",
                             submenu: root.taildropPeers })
            // Moving a file into the folder it already lives in is not an action, so the row hides there.
            if (root.dropboxPath.length > 0 && !root.rowInDropbox)
                share.push({ label: "Move to Dropbox", action: "dropbox", mark: "dropbox" })
            // A share link is inherently per file, so it appears only for a row already in Dropbox.
            if (root.rowInDropbox)
                share.push({ label: "Copy share link", action: "sharelink", glyph: "network" })
            if (share.length > 0) {
                out.push({ separator: true })
                for (var s = 0; s < share.length; s++) out.push(share[s])
            }
            out.push({ separator: true })
            // No confirm anywhere behind this row: the undo journal is the safety, see the operations design.
            out.push({ label: "Move to Trash", action: "trash", glyph: "trash", danger: true })
            out.push({ separator: true })
        }
        // The last group is the two rows that need no row under the cursor, which is also the whole
        // menu on a listing's empty space. Operations.dc.html draws neither the row nor this divider,
        // and a create action sitting directly under the destructive one is what earns the divider.
        out.push({ label: "New folder", action: "newFolder", glyph: "folder-plus" })
        out.push({
            label: root.showHidden ? "Hide hidden files" : "Show hidden files",
            action: "toggleHidden",
            glyph: root.showHidden ? "eye-off" : "eye"
        })
        return out
    }

    // A separator is never the cursor, so both key steps and the opening cursor skip over one.
    function stepCursor(from, delta) {
        var i = from + delta
        while (i >= 0 && i < root.entries.length) {
            if (root.entries[i].separator !== true)
                return i
            i += delta
        }
        return from
    }

    function firstRow() {
        return root.entries.length > 0 && root.entries[0].separator === true ? root.stepCursor(0, 1) : 0
    }

    anchors.fill: parent
    visible: root.opened
    z: 1

    // Takes a point in scene coordinates and keeps the whole menu inside the pane it belongs to.
    function openAt(scenePoint) {
        root.clearRail()
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
        frame.x = Menu.clamp(root.placeX, frame.width, root.width)
        frame.y = Menu.clamp(root.placeY, frame.height, root.height)
    }

    function place(scenePoint) {
        var point = root.mapFromItem(null, scenePoint)
        root.placeX = point.x
        root.placeY = point.y
        root.clampFrame()
        root.cursor = root.firstRow()
        root.openSubmenuRow = -1
        root.submenuCursor = 0
        root.focusHolder = root.focusedSibling()
        root.opened = true
        keyCatcher.forceActiveFocus()
    }

    // Whichever sibling holds active focus when the menu opens, which is the pane's list today.
    function focusedSibling() {
        var siblings = root.parent ? root.parent.children : []
        for (var i = 0; i < siblings.length; i++) {
            if (siblings[i] !== root && siblings[i].activeFocus)
                return siblings[i]
        }
        return null
    }

    // Every wheel scroll calls this, so a shut menu costs nothing and never touches focus.
    function close() {
        if (!root.opened)
            return
        root.opened = false
        root.openSubmenuRow = -1
        root.clearRail()
        if (root.focusHolder)
            root.focusHolder.forceActiveFocus()
    }

    // The menu closes before the action runs, so it never hangs over the listing that action opened.
    function choose(action) {
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
        root.close()
        if (entry)
            root.chosen(entry.action + ":" + id)
    }

    function openSubmenu(index) {
        root.openSubmenuRow = index
        root.submenuCursor = 0
    }

    // Rows above the open one are a mix of full rows and separators, so the offset is summed, not multiplied.
    function submenuOffset() {
        var y = 0
        for (var i = 0; i < root.openSubmenuRow; i++)
            y += root.entries[i].separator === true ? separatorProbe.separatorHeight : Theme.rowHeight
        return y
    }

    // One row off the model, only so the two heights above are read from MenuRow rather than repeated here.
    Flea.MenuRow {
        id: separatorProbe
        visible: false
        entry: ({ separator: true })
    }

    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onPressed: root.close()
    }

    // Declared before both frames so neither ring can darken the panel beside it; the canvas draws
    // this shadow under the menu and under the flyout alike.
    Flea.Shadow {
        surface: frame
    }

    Flea.Shadow {
        surface: flyout
    }

    Rectangle {
        id: frame
        width: Theme.menuWidth
        // The vertical inset keeps the first and last row's square highlight off the rounded corners.
        height: rows.implicitHeight + 2 * Theme.spacing.rowPaddingY
        // The height this menu is actually going to have, arriving after place() has already run.
        onHeightChanged: if (root.opened) root.clampFrame()
        color: Theme.color.surface
        border.width: Theme.spacing.hairline
        border.color: Theme.color.muted
        // Mirrors hyprland decoration:rounding, same as NetworkDialog; 0 on a stock box stays square.
        radius: Style.cornerRadius

        Column {
            id: rows
            width: parent.width
            y: Theme.spacing.rowPaddingY

            Repeater {
                model: root.entries
                delegate: Flea.MenuRow {
                    id: row
                    required property var modelData
                    required property int index
                    width: rows.width
                    entry: row.modelData
                    compact: root.forRail
                    current: !root.submenuOpen && root.cursor === row.index
                    onHoverEntered: root.cursor = row.index
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

    // The flyout: a second frame beside whichever row opened it, only while one has.
    Rectangle {
        id: flyout
        visible: root.submenuOpen
        x: frame.x + frame.width
        // peers.y already carries the inset, so the flyout frame itself stays on the row grid.
        y: frame.y + root.submenuOffset()
        width: Theme.menuWidth
        height: peers.implicitHeight + 2 * Theme.spacing.rowPaddingY
        color: Theme.color.surface
        border.width: Theme.spacing.hairline
        border.color: Theme.color.muted
        radius: Style.cornerRadius

        Column {
            id: peers
            width: parent.width
            y: Theme.spacing.rowPaddingY

            Repeater {
                model: root.submenuEntries
                delegate: Flea.MenuRow {
                    id: subRow
                    required property var modelData
                    required property int index
                    width: peers.width
                    // A Taildrop peer is a machine and takes the sidebar's own server mark; an archive
                    // format is a file about to exist and takes the archive mark.
                    entry: ({ label: subRow.modelData.label, action: "",
                              glyph: root.entries[root.openSubmenuRow].action === "taildrop" ? "server" : "archive" })
                    current: root.submenuCursor === subRow.index
                    onHoverEntered: root.submenuCursor = subRow.index
                    onActivated: root.chooseSub(subRow.modelData.id)
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
            var action = Keymap.lookup(event.key, event.text, event.modifiers)
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
                    root.submenuCursor = Math.min(root.submenuEntries.length - 1, root.submenuCursor + 1)
                else
                    root.cursor = root.stepCursor(root.cursor, 1)
                event.accepted = true
                return
            }
            if (action === "cursorUp") {
                if (root.submenuOpen)
                    root.submenuCursor = Math.max(0, root.submenuCursor - 1)
                else
                    root.cursor = root.stepCursor(root.cursor, -1)
                event.accepted = true
                return
            }
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
