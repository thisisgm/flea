import QtQuick
import Quickshell
import qs.Commons
import "." as Flea
import "js/Keymap.js" as Keymap
import "js/Menu.js" as Menu

// One context-menu row: the glyph slot, the label, and the disclosure a submenu row carries.
Item {
    id: root

    // {label, action, glyph, danger?, submenu?} or {separator: true}; see ui/ContextMenu.qml's buildEntries.
    property var entry: ({})
    property bool current: false
    // A pick list's chosen row, drawn as the canvas draws the convert popup and the share list:
    // accent ink over an accent tint, where a plain menu row only takes the foreground lift below.
    property bool picked: false
    // A menu the rail raised takes the rail's own row height and mark slot, so it reads as part of the
    // rail instead of the listing's menu parked against it. ui/js/Mounts.js gives a rail row at most
    // one entry and never gives it a listing row's, so nobody can see one menu at two sizes.
    property bool compact: false

    signal activated()
    // The parent owns the cursor, so a pointer that moves onto the row asks for it; a menu opened under a resting pointer asks nothing, or Enter would fire the pointer's row (0d626ed).
    signal pointerMoved()
    // For a parent that lights the pointer's row without moving its own cursor, as ui/ShareBrowser.qml does.
    readonly property bool hovered: pointer.hovered
    // For ui/Ipc.qml's contextMenuRowProbe: whether the pointer is over this row and where, before a test judges a move.
    function probe() { return pointer.hovered + " " + Math.round(pointer.point.position.x) + " " + Math.round(pointer.point.position.y) + " " + Math.round(pointer.restingAt.x) + " " + Math.round(pointer.restingAt.y) }
    // Where any row of this menu last saw the pointer, so a row can tell a pointer that moved onto
    // it from a row that scrolled under a pointer standing still. ui/ContextMenu.qml owns the value.
    property point lastPointerGlobal: Qt.point(-1, -1)
    signal pointerSeen(point at)

    readonly property bool available: root.entry.disabled !== true
    readonly property bool isSeparator: root.entry.separator === true
    // Menu.hasSubmenu and not a local test, because a local one read submenu === true and the menu sets an array.
    readonly property bool isSubmenu: Menu.hasSubmenu(root.entry)
    // A danger row takes the theme's error role for both its mark and its label, never a hardcoded red.
    readonly property bool danger: root.entry.danger === true
    // A provider that cannot answer reads as an error in its own row, the way Move to Trash reads as
    // a danger: red, and no sentence beside it widening the menu. GM's ruling, 2026-09-10.
    readonly property bool errored: root.entry.errored === true
    // The key this row's action answers to, right-aligned per Menus.html. Derived from keys.toml
    // through the generated map, so an unbound action leaves the slot empty rather than guessing.
    // Empty with the Menus section's hints row off, which takes the slot's width with it.
    readonly property string hint: root.isSeparator ? "" : root.entry.hint !== undefined ? root.entry.hint
                                 : ViewState.keyHints ? Keymap.hintFor(root.entry.action) : ""
    readonly property color markColor: root.errored ? Theme.color.error
                                     : !root.available ? Theme.color.muted : root.danger ? Theme.color.error
                                     : root.picked ? Theme.color.accent : Theme.color.muted
    readonly property color labelColor: root.errored ? Theme.color.error
                                      : !root.available ? Theme.color.muted : root.danger ? Theme.color.error
                                      : root.entry.labelColor !== undefined ? root.entry.labelColor
                                      : root.picked ? Theme.color.accent : Theme.color.foreground

    // The hover lift Row.qml uses, so a menu row and a list row read alike.
    readonly property real hoverOpacity: 0.08
    // Menus.html resolves the separator to rowGap + hairline, 10 px at base size 14.
    readonly property int separatorHeight: Theme.spacing.gap + Theme.spacing.hairline
    readonly property real separatorOpacity: 0.4

    // The rail's mark slot is its icon size, exactly as ui/SidebarRow.qml sizes its own.
    readonly property int slotSize: root.compact ? Theme.railIconSize : Theme.markSize
    // OpenWith.html: an application's own Icon= rides in the mark slot, full colour and no plate.
    // The ladder is AppLibrary.qml's iconSource: the backend's app and device index has already
    // answered with a path where it could, and only a name it could not place reaches the themed
    // lookup here. Where that service falls back to application-x-executable, the board rules the
    // muted app-window glyph instead, so an unresolved name leaves the mark below standing.
    readonly property string appIcon: root.entry.icon !== undefined ? String(root.entry.icon) : ""
    readonly property string appIconSource: root.appIcon.length === 0 ? ""
                                          : root.appIcon.indexOf("file://") === 0 ? root.appIcon
                                          : root.appIcon.charAt(0) === "/" ? Util.fileUrl(root.appIcon)
                                          : Quickshell.iconPath(root.appIcon, true)
    // That board draws the mark at 16 of the slot's 19 units, where a stroked glyph takes the slot whole.
    readonly property int appIconSize: Math.round(root.slotSize * 16 / 19)

    height: root.isSeparator ? root.separatorHeight
          : (root.compact ? Theme.railRowHeight : Theme.rowHeight)

    Rectangle {
        anchors.fill: parent
        visible: !root.isSeparator
        // selectedAccentFill already carries the theme's own selected alpha, so the tint is a
        // colour here and an opacity for the plain lift, never both at once.
        color: root.picked ? Style.selectedAccentFill : Theme.color.foreground
        opacity: root.picked ? 1 : (root.current ? root.hoverOpacity : 0)
    }

    Rectangle {
        visible: root.isSeparator
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        height: Theme.spacing.hairline
        color: Theme.color.muted
        opacity: root.separatorOpacity
    }

    // Every action row carries a mark: a menu where some rows are marked and some are not reads broken.
    // The slot sets the label's indent too, which the canvas draws at rowPaddingX + slot + gap.
    // ui/Glyph.qml already caps every mark at its slot, so the slot is the only size to set here.
    Item {
        id: markSlot
        visible: !root.isSeparator
        opacity: root.available ? 1 : 0.55
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        width: root.slotSize
        height: root.slotSize

        // A brand mark is a reproduction and takes its own component; every other row is a cut glyph.
        Flea.Glyph {
            anchors.fill: parent
            visible: root.entry.mark === undefined && !appMark.visible
            name: root.entry.glyph !== undefined ? root.entry.glyph : "file"
            color: root.markColor
        }

        // The theme that cannot name the entry leaves the glyph above standing in its muted role.
        Image {
            id: appMark
            anchors.centerIn: parent
            width: root.appIconSize
            height: root.appIconSize
            visible: root.appIconSource.length > 0 && appMark.status === Image.Ready
            source: root.appIconSource
            sourceSize.width: root.appIconSize
            sourceSize.height: root.appIconSize
            fillMode: Image.PreserveAspectFit
            asynchronous: true
        }

        Flea.TailscaleMark {
            anchors.centerIn: parent
            visible: root.entry.mark === "tailscale"
            iconSize: root.slotSize
            color: root.markColor
        }

        Flea.DropboxMark {
            anchors.centerIn: parent
            visible: root.entry.mark === "dropbox"
            iconSize: root.slotSize
            color: root.markColor
        }
    }

    Text {
        id: label
        visible: !root.isSeparator
        anchors.left: markSlot.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.right: hintText.left
        anchors.rightMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        text: root.entry.label !== undefined ? root.entry.label : ""
        opacity: root.available ? 1 : 0.55
        color: root.labelColor
        font.family: Theme.font.family
        font.pixelSize: Theme.font.body
        textFormat: Text.PlainText
        elide: Text.ElideRight
    }

    // The shortcut hint. An unbound row draws nothing and takes no width, so a menu of unbound rows
    // reads exactly as it did before this slot existed.
    Text {
        id: hintText
        visible: root.hint.length > 0
        anchors.right: chevronSlot.left
        anchors.rightMargin: root.isSubmenu ? Theme.spacing.gap : 0
        anchors.verticalCenter: parent.verticalCenter
        text: root.hint
        // Permissions.html alone leaves its refusal span wrappable, and it asks for that by name. The
        // constraint subtracts the label's unelided width, so a short caption beside a long label
        // collapsed to nothing: the flyout's own "default" is the caption that has to survive.
        width: root.entry.hintWrap === true
               ? Math.max(0, Math.min(implicitWidth, root.width - 2 * Theme.spacing.rowPaddingX
                                     - root.slotSize - 2 * Theme.spacing.gap - label.implicitWidth))
               : implicitWidth
        wrapMode: root.entry.hintWrap === true ? Text.WordWrap : Text.NoWrap
        horizontalAlignment: Text.AlignRight
        // An unavailable row retains its reason while its label takes the muted role.
        color: root.available ? root.labelColor : Theme.color.foreground
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
    }

    // The disclosure is a cut glyph, not the "▸" font dingbat the menu used to mix into a path language.
    Item {
        id: chevronSlot
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        width: root.isSubmenu ? Theme.font.caption : 0
        height: Theme.font.caption

        Flea.Glyph {
            anchors.fill: parent
            visible: root.isSubmenu
            name: "chevron-right"
            color: Theme.color.muted
        }
    }

    HoverHandler {
        id: pointer
        enabled: !root.isSeparator && root.available
        // Global coordinates distinguish actual motion from a row moving beneath the resting pointer.
        property bool armed: false
        property point restingAt
        // A menu that opens under a pointer standing still delivers no hover at all here, measured on
        // this box, so the hover that does arrive was caused by the pointer moving and must light the
        // row. The one exception is a row arriving under a pointer that has not moved, which reports
        // the position the menu last saw; that one only records where the pointer is.
        onHoveredChanged: {
            if (!pointer.hovered) {
                pointer.armed = false
                return
            }
            var position = root.mapToGlobal(pointer.point.position)
            pointer.armed = true
            pointer.restingAt = position
            // Read the coordinates out before reporting the new one: the shared property is live, so
            // holding it would compare the new position against itself and lose the move.
            var knownX = root.lastPointerGlobal.x, knownY = root.lastPointerGlobal.y
            root.pointerSeen(position)
            if (knownX < 0 || position.x !== knownX || position.y !== knownY)
                root.pointerMoved()
        }
        onPointChanged: {
            if (!pointer.hovered)
                return
            var position = root.mapToGlobal(pointer.point.position)
            if (!pointer.armed) {
                var firstX = root.lastPointerGlobal.x, firstY = root.lastPointerGlobal.y
                pointer.armed = true
                pointer.restingAt = position
                root.pointerSeen(position)
                if (firstX < 0 || position.x !== firstX || position.y !== firstY)
                    root.pointerMoved()
                return
            }
            var moved = position.x !== pointer.restingAt.x || position.y !== pointer.restingAt.y
            pointer.restingAt = position
            root.pointerSeen(position)
            if (moved)
                root.pointerMoved()
        }
    }

    TapHandler {
        enabled: !root.isSeparator && root.available
        acceptedButtons: Qt.LeftButton
        gesturePolicy: TapHandler.ReleaseWithinBounds
        onTapped: root.activated()
    }
}
