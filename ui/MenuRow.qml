import QtQuick
import qs.Commons
import "." as Flea
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
    // The parent owns the cursor, so hover asks for it to move rather than writing over the binding.
    signal hoverEntered()
    // For a parent that lights the pointer's row without moving its own cursor, as ui/ShareBrowser.qml does.
    readonly property bool hovered: pointer.hovered

    readonly property bool isSeparator: root.entry.separator === true
    // Menu.hasSubmenu and not a local test, because a local one read submenu === true and the menu sets an array.
    readonly property bool isSubmenu: Menu.hasSubmenu(root.entry)
    // A danger row takes the theme's error role for both its mark and its label, never a hardcoded red.
    readonly property bool danger: root.entry.danger === true
    readonly property color markColor: root.danger ? Theme.color.error
                                     : root.picked ? Theme.color.accent : Theme.color.muted
    readonly property color labelColor: root.danger ? Theme.color.error
                                      : root.picked ? Theme.color.accent : Theme.color.foreground

    // The hover lift Row.qml uses, so a menu row and a list row read alike.
    readonly property real hoverOpacity: 0.08
    // A separator is a hairline with one gap of air around it, which is the mock's 11 px read from tokens instead.
    readonly property int separatorHeight: Theme.spacing.gap + Theme.spacing.hairline
    readonly property real separatorOpacity: 0.4

    // The rail's mark slot is its icon size, exactly as ui/SidebarRow.qml sizes its own.
    readonly property int slotSize: root.compact ? Theme.railIconSize : Theme.markSize

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
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        width: root.slotSize
        height: root.slotSize

        // A brand mark is a reproduction and takes its own component; every other row is a cut glyph.
        Flea.Glyph {
            anchors.fill: parent
            visible: root.entry.mark === undefined
            name: root.entry.glyph !== undefined ? root.entry.glyph : "file"
            color: root.markColor
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
        anchors.right: chevronSlot.left
        anchors.rightMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        text: root.entry.label !== undefined ? root.entry.label : ""
        color: root.labelColor
        font.family: Theme.font.family
        font.pixelSize: Theme.font.bodySmall
        textFormat: Text.PlainText
        elide: Text.ElideRight
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
        enabled: !root.isSeparator
        // pointer.hovered spelt out: root now carries a hovered of its own and would shadow here.
        onHoveredChanged: if (pointer.hovered) root.hoverEntered()
    }

    TapHandler {
        enabled: !root.isSeparator
        acceptedButtons: Qt.LeftButton
        gesturePolicy: TapHandler.ReleaseWithinBounds
        onTapped: root.activated()
    }
}
