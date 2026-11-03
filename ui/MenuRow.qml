// Bound: the brand-mark Components below read this row's root, and are only ever built by this row's own Loaders.
pragma ComponentBehavior: Bound

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

    signal activated()
    readonly property bool pressed: tap.pressed
    // The parent owns the cursor, so a pointer that moves onto the row asks for it; a menu opened under a resting pointer asks nothing, or Enter would fire the pointer's row (0d626ed).
    signal pointerMoved()
    // For a parent that lights the pointer's row without moving its own cursor, as ui/ShareBrowser.qml does.
    readonly property bool hovered: pointer.hovered
    // For ui/Ipc.qml's contextMenuRowProbe: whether the pointer is over this row and where, before a test judges a move.
    function probe() { return pointer.hovered + " " + Math.round(pointer.point.scenePosition.x) + " " + Math.round(pointer.point.scenePosition.y) + " " + Math.round(pointer.restingAt.x) + " " + Math.round(pointer.restingAt.y) }
    // The scene point (despite the name) the host's rows or ground last saw, or -1 when unseen since it opened; the host owns it.
    property point lastPointerGlobal: Qt.point(-1, -1)
    // A first report is motion only a pixel or more from a point the host saw: Qt rounds the point a ground takes on appearing under the pointer.
    function awayFrom(position, x, y) { return x >= 0 && (Math.abs(position.x - x) >= 1 || Math.abs(position.y - y) >= 1) }
    signal pointerSeen(point at)

    readonly property bool available: root.entry.disabled !== true
    readonly property bool isSeparator: root.entry.separator === true
    // Menu.hasSubmenu and not a local test, because a local one read submenu === true and the menu sets an array.
    readonly property bool isSubmenu: Menu.hasSubmenu(root.entry)
    // A danger row takes the theme's error role for both its mark and its label, never a hardcoded red.
    readonly property bool danger: root.entry.danger === true
    // Unavailable app rows use the theme's error role without a state word (GM 2026-10-29).
    readonly property bool errored: root.entry.errored === true
    // The key this row's action answers to, right-aligned per Menus.html. Derived from keys.toml
    // through the generated map, so an unbound action leaves the slot empty rather than guessing.
    // Empty with the Menus section's hints row off, which takes the slot's width with it.
    readonly property string hint: root.isSeparator ? "" : root.entry.hint !== undefined ? root.entry.hint
                                 : ViewState.keyHints ? Keymap.hintFor(root.entry.action) : ""
    // A Tags flyout row's box and swatch are built only for those rows (ui/TagMenuMark.qml), so every other row builds none.
    property var tagItem: null
    function mountTag() {
        if (root.entry.tagRow === undefined || root.tagItem !== null) return
        root.tagItem = Qt.createComponent("TagMenuMark.qml").createObject(root, { row: root, mark: markSlot, hint: hintText })
    }
    Component.onCompleted: root.mountTag()
    onEntryChanged: root.mountTag()
    readonly property color markColor: root.errored ? Theme.color.error
                                     : !root.available ? Theme.color.mutedFrame : root.danger ? Theme.color.error
                                     : root.picked ? Theme.color.accent : Theme.color.mutedFrame
    readonly property color labelColor: root.errored ? Theme.color.error
                                      : !root.available ? Theme.color.muted : root.danger ? Theme.color.error
                                      : root.entry.labelColor !== undefined ? root.entry.labelColor
                                      : root.picked ? Theme.color.accent : Theme.color.foreground

    // The hover lift Row.qml uses, so a menu row and a list row read alike.
    readonly property real hoverOpacity: Theme.washHover
    // The rail's mount-status square and the gap the Update Flea board puts between it and the version.
    readonly property int statusSquareSize: 6
    readonly property int statusSquareGap: 7
    // Menus.html resolves the separator to rowGap + hairline, 10 px at base size 14.
    readonly property int separatorHeight: Theme.spacing.gap + Theme.spacing.hairline
    readonly property real separatorOpacity: 0.4

    // Every menu row uses the listing's own mark slot and row height, so a rail menu reads as the main menu.
    readonly property int slotSize: Theme.markSize
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

    height: root.isSeparator ? root.separatorHeight : Theme.rowHeight

    // The width this row needs to draw whole, from the layout's own terms below; a wrapped caption takes what the label leaves.
    readonly property real wantedWidth: root.isSeparator ? 0
        : Math.ceil(2 * Theme.spacing.rowPaddingX + root.slotSize + 2 * Theme.spacing.gap + label.implicitWidth
                    + (root.entry.hintWrap === true ? 0 : hintText.implicitWidth)
                    + (statusSquare.visible ? statusSquare.width + statusSquare.anchors.rightMargin : 0)
                    + chevronSlot.width + hintText.anchors.rightMargin)

    // What this row laid out, read off its items and not off wantedWidth: ui/IpcMenuState.qml's width oracle sums these.
    function parts() {
        if (root.isSeparator) return {separator: true}
        return {separator: false, markLeft: markSlot.anchors.leftMargin, slot: markSlot.width,
                labelLeft: label.anchors.leftMargin, label: label.implicitWidth, labelRight: label.anchors.rightMargin,
                labelTruncated: label.truncated, hint: hintText.implicitWidth, hintWrap: root.entry.hintWrap === true,
                hintRight: hintText.anchors.rightMargin, status: statusSquare.visible ? statusSquare.width + statusSquare.anchors.rightMargin : 0,
                chevron: chevronSlot.width, chevronRight: chevronSlot.anchors.rightMargin}
    }

    Rectangle {
        objectName: "menuWash"
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
        color: Theme.color.mutedFrame
        opacity: root.separatorOpacity
    }

    // Every action row carries a mark: a menu where some rows are marked and some are not reads broken.
    // The slot sets the label's indent too, which the canvas draws at rowPaddingX + slot + gap.
    // ui/Glyph.qml already caps every mark at its slot, so the slot is the only size to set here.
    Item {
        id: markSlot
        visible: !root.isSeparator
        opacity: root.available ? 1 : Theme.disabledOpacity
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        width: root.slotSize
        height: root.slotSize

        // A brand mark is a reproduction and takes its own component; every other row is a cut glyph.
        Flea.Glyph {
            anchors.fill: parent
            visible: root.entry.mark === undefined && !appMark.visible && root.entry.tagRow === undefined
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

        // Each brand mark is built only on the row whose entry names it: carried hidden by every row, the
        // four were most of a menu's build, and a Loader has no pointer semantics to lose (rows stay eager).
        Loader {
            anchors.centerIn: parent
            active: root.entry.mark === "tailscale"
            sourceComponent: Component {
                Flea.TailscaleMark {
                    iconSize: root.slotSize
                    color: root.markColor
                }
            }
        }

        Loader {
            anchors.centerIn: parent
            active: root.entry.mark === "dropbox"
            sourceComponent: Component {
                Flea.DropboxMark {
                    iconSize: root.slotSize
                    color: root.markColor
                }
            }
        }

        Loader {
            anchors.centerIn: parent
            active: root.entry.mark === "localsend"
            sourceComponent: Component {
                Flea.LocalSendMark {
                    iconSize: root.slotSize
                    color: root.markColor
                }
            }
        }

        // The shelf is Flea's own destination, so it carries Flea's own mark rather than a cut glyph.
        Loader {
            anchors.centerIn: parent
            active: root.entry.mark === "flea"
            sourceComponent: Component {
                Flea.FleaMark {
                    width: root.slotSize
                    height: root.slotSize
                    color: root.markColor
                }
            }
        }
    }

    Text {
        id: label
        visible: !root.isSeparator
        anchors.left: markSlot.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.right: root.tagItem !== null && root.entry.tagRow !== undefined ? root.tagItem.left : statusSquare.visible ? statusSquare.left : hintText.left
        anchors.rightMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        text: root.entry.label !== undefined ? root.entry.label : ""
        opacity: root.available ? 1 : Theme.disabledOpacity
        color: root.labelColor
        font.family: Theme.font.family
        font.pixelSize: Theme.font.body
        textFormat: Text.PlainText
        elide: Text.ElideRight
    }

    // The rail's 6 px mount-status square, borrowed by a row whose hint is a state rather than a key: Update Flea's newer version.
    Rectangle {
        id: statusSquare
        visible: root.entry.hintSquare === true && root.hint.length > 0
        anchors.right: hintText.left
        anchors.rightMargin: root.statusSquareGap
        anchors.verticalCenter: parent.verticalCenter
        width: root.statusSquareSize
        height: root.statusSquareSize
        color: Theme.color.accent
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
        // MenuAdditions040: every key hint is muted on a live, lit or dead row; a dead row dims it with the label, a wrapped reason stays readable.
        color: root.entry.hintSquare === true ? root.labelColor
             : root.available || root.entry.hintWrap !== true ? Theme.color.muted : Theme.color.foreground
        opacity: root.available || root.entry.hintWrap === true ? 1 : label.opacity
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        // A version beside the status square is a number, so its digits keep one width the way the rail's sizes do.
        font.features: root.entry.hintSquare === true ? { "tnum": 1 } : ({})
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
            color: Theme.color.mutedFrame
        }
    }

    HoverHandler {
        id: pointer
        enabled: !root.isSeparator && root.available
        // Scene coordinates distinguish actual motion from a row moving beneath the resting pointer.
        property bool armed: false
        property point restingAt
        // A first hover at the menu's last point, or at one unseen since it opened (Qt 6.12 hovers up to 100 ms late), is a still pointer.
        onHoveredChanged: {
            if (!pointer.hovered) {
                pointer.armed = false
                return
            }
            var position = pointer.point.scenePosition
            pointer.armed = true
            pointer.restingAt = position
            // Read the coordinates out before reporting the new one: the shared property is live, so
            // holding it would compare the new position against itself and lose the move.
            var knownX = root.lastPointerGlobal.x, knownY = root.lastPointerGlobal.y
            root.pointerSeen(position)
            if (root.awayFrom(position, knownX, knownY))
                root.pointerMoved()
        }
        onPointChanged: {
            if (!pointer.hovered)
                return
            var position = pointer.point.scenePosition
            if (!pointer.armed) {
                var firstX = root.lastPointerGlobal.x, firstY = root.lastPointerGlobal.y
                pointer.armed = true
                pointer.restingAt = position
                root.pointerSeen(position)
                if (root.awayFrom(position, firstX, firstY))
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
        id: tap
        enabled: !root.isSeparator && root.available
        acceptedButtons: Qt.LeftButton
        gesturePolicy: TapHandler.ReleaseWithinBounds
        onTapped: root.activated()
    }
}
