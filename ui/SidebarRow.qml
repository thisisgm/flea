import QtQuick
import qs.Commons
import "js/Format.js" as Format
import "js/Mounts.js" as Mounts
import "js/RailMenu.js" as RailMenu

// One rail row, shared by the Favorites, Network and Devices groups so the three read alike; see
// ui/Sidebar.qml "The rail" for the entry shape each group feeds this delegate.
Item {
    id: root

    required property int index
    required property var modelData
    property bool cursor: false
    property bool focused: false
    // Network only, driven from ui/Sidebar.qml's renamingIndex; swaps the label for the editor.
    property bool renaming: false

    signal activated(int index)
    // Right click asks the rail to raise the menu over this row, carrying the point it opens at.
    // The rail decides what the menu offers and opens none on a row with nothing to release. It is
    // ui/Pane.qml's own single ui/ContextMenu.qml: a second instance in this tree took the keyboard
    // away from the list, see AGENTS.md "A second ContextMenu instance breaks the whole window's
    // keyboard focus", which is why the rail had no menu at all until now.
    signal menuRequested(int index, var scenePosition)
    signal renameCommitted(int index, string text)
    signal renameCancelled(int index)
    // Issue 76: the release the row's own menu offers, pressed on the row itself. It carries the
    // action and the key rather than the index, so the rail dispatches it through the exact
    // releaseChosen a chosen menu row takes, stale-index rule included.
    signal releaseRequested(string action, string key)

    // Issue 76's ask: a mounted row that can release draws that release where its mounted dot sat,
    // because a row with a release is by definition mounted, so the mark carries the dot's own news.
    // ui/js/RailMenu.js releaseMark is the policy, NFS's carve-out included.
    readonly property string releaseAction: RailMenu.releaseMark(root.modelData)

    // A share or a removable volume carries a mount-state dot; the internal disk is always there
    // and always mounted, so a dot on it would say nothing, and a favourite is not a mount at all.
    readonly property var placesState: ViewState.state.places || ({})
    readonly property string detail: root.modelData.kind === "trash"
        ? (root.placesState.trashCount === true && root.modelData.count > 0 ? String(root.modelData.count) : "")
        : root.modelData.group === "device" && root.placesState.driveSize === true && root.modelData.size !== null
            ? Format.size(root.modelData.size) : ""
    // PhoneMark: a phone is a volume, so its row carries the same mount-state square a removable
    // disk and a share carry, in the same fixed slot.
    readonly property bool showsDot: (root.modelData.group === "network" && root.modelData.kind !== "dropbox")
        || (root.modelData.group === "device"
            && (root.modelData.kind === "volume" || root.modelData.kind === "phone"))
    // The Trash row draws its count where every other row draws its indicator, and never both.
    readonly property bool countIsIndicator: root.modelData.kind === "trash" && root.detail.length > 0
    // Small and fixed: a status dot is not part of the type or icon scale.
    readonly property int dotSize: 6
    // The canvas's own value for a bookmark nothing has mounted yet.
    readonly property real unmountedOpacity: 0.5

    width: parent ? parent.width : 0
    // The rail reads denser than the list it sits beside; see Theme.qml's railRowHeight comment.
    height: Theme.railRowHeight

    Accessible.role: Accessible.ListItem
    Accessible.name: root.modelData.label

    // A rail row is a row, so its cursor is the list row's own: a square full-bleed fill and the
    // accent bar, per the canvas and the icon spec's "the rail rounds nothing"; see ui/Row.qml.
    Rectangle {
        anchors.fill: parent
        color: root.cursor
            ? (root.modelData.kind === "trash" ? Style.selectedAccentFill : root.focused ? Style.selectedFill : Style.normalFill)
            : "transparent"
    }

    Rectangle {
        visible: root.cursor
        width: Theme.spacing.hairline * 2
        height: parent.height
        color: Theme.color.accent
    }

    Loader {
        id: mark
        anchors.left: parent.left
        anchors.leftMargin: Style.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        width: Theme.railIconSize
        height: Theme.railIconSize
        sourceComponent: root.modelData.kind === "dropbox" ? dropboxMark : glyphMark
    }

    // ThemeRoles.html gives every rail label and glyph the foreground role, selected rows included:
    // the cursor reads through the fill and the accent edge above, never by dimming the rows it is not on.
    Component {
        id: glyphMark
        Glyph {
            name: root.modelData.glyph
            color: root.cursor && root.modelData.kind === "trash" ? Theme.color.accent : Theme.color.foreground
        }
    }

    // The brand marks are reproductions, so they take their own component rather than Glyph sizing.
    Component {
        id: dropboxMark
        DropboxMark {
            iconSize: Theme.railIconSize
            color: Theme.color.foreground
        }
    }

    Text {
        id: label
        visible: !root.renaming
        anchors.left: mark.right
        anchors.leftMargin: Style.spacing.rowGap
        // The numbers column bounds a label only on a row that draws a number; a row with no detail
        // runs its label to the 12 px slot itself, whose own 3 px of air either side of the dot is
        // the gap. Measured: a phone's label needs 126 px and the numbers column left it 110.
        anchors.right: root.detail.length > 0 ? detailText.left : dot.left
        anchors.rightMargin: root.detail.length > 0 ? Style.spacing.rowGap : 0
        anchors.verticalCenter: parent.verticalCenter
        text: root.modelData.label
        color: root.modelData.error ? Theme.color.error : Theme.color.foreground
        font.family: Theme.font.family
        font.pixelSize: Theme.font.body
        // SettingsRest rule 5: a share reads "minipc · nvme-share" and the tail is the half that names it, so a label too long for the rail loses its middle rather than the end that identifies it.
        // Board rule 5: a rail label elides from the head, because the tail is the name that tells
        // two places apart; the middle-elide this carried was the overseer's, and GM overruled it.
        elide: Text.ElideLeft
        textFormat: Text.PlainText
    }

    // The label's slot, holding the product's one editor, ui/RenameField.qml, in rail type.
    Loader {
        id: renameLoader
        active: root.renaming
        anchors.left: mark.right
        anchors.leftMargin: Style.spacing.rowGap
        anchors.right: root.detail.length > 0 ? detailText.left : dot.left
        anchors.rightMargin: root.detail.length > 0 ? Style.spacing.rowGap : 0
        anchors.verticalCenter: parent.verticalCenter
        height: Theme.railRowHeight - 2 * Theme.spacing.rowPaddingY
        sourceComponent: RenameField {
            name: root.modelData.label
            onCommitted: function (newName) { root.renameCommitted(root.index, newName) }
            onAbandoned: root.renameCancelled(root.index)
        }
    }

    // What the editor holds right now, for tests through ui/Ipc.qml's railRenameEditorText.
    readonly property string editorText: renameLoader.item ? renameLoader.item.current : ""
    readonly property bool editorShown: renameLoader.item !== null && renameLoader.item.visible
    // The rail's real trailing indicator slot, so ui/Ipc.qml measures this dot instead of recomputing it.
    readonly property Item indicatorSlot: dot
    readonly property Item detailItem: detailText
    readonly property Item labelItem: label
    readonly property bool indicatorVisible: root.showsDot

    Text {
        id: detailText
        anchors.right: dot.left
        anchors.rightMargin: dot.width > 0 ? Style.spacing.rowGap : 0
        anchors.verticalCenter: parent.verticalCenter
        text: root.detail
        color: Theme.color.muted
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        font.features: { "tnum": 1 }
        horizontalAlignment: Text.AlignRight
        textFormat: Text.PlainText
    }

    // Every right-aligned mark in the rail is centred in a caption-wide slot, so this dot and the
    // NETWORK header's "+" share one centre line whatever their ink does: align by slot, never by ink.
    Item {
        id: dot
        anchors.right: parent.right
        anchors.rightMargin: Style.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        // RailDetails rules 1 and 3 with their 2026-09-14 amendment: a drive size is a number, so
        // every device row ends its size on one x with this slot reserved after it, dot or no dot;
        // the Trash count is an indicator rather than a size, so it sits in the slot itself, on the
        // same edge as the mount dots and the NETWORK plus.
        width: root.countIsIndicator ? 0 : (root.showsDot || root.detail.length > 0 ? Theme.font.caption : 0)
        height: Theme.font.caption

        // Green once gio mount -l lists it, muted at half strength while it is only a bookmark waiting
        // to be mounted. A square, not a disc: the cut is hard corners, and the canvas draws it square.
        Rectangle {
            visible: root.showsDot && root.releaseAction.length === 0
            anchors.centerIn: parent
            width: root.dotSize
            height: root.dotSize
            color: root.modelData.mounted ? Theme.color.executable : Theme.color.muted
            opacity: root.modelData.mounted ? 1 : root.unmountedOpacity
        }

        // The release mark, ink at the slot's own size the way the NETWORK plus draws its glyph; the
        // press target is the tap handler's own hitMin band below, never only these few pixels.
        Glyph {
            visible: root.releaseAction.length > 0
            anchors.centerIn: parent
            width: Theme.font.caption
            height: width
            name: "eject"
            color: Theme.color.muted
        }
    }

    TapHandler {
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onTapped: function (eventPoint, button) {
            // The field owns clicks inside itself while editing; this only covers the rest of the row.
            if (root.renaming)
                return
            // A right click never activates, whatever the row is: it either raised a menu or the
            // row had nothing to offer, and it must not mount and open a stick nobody asked to open.
            if (button === Qt.RightButton) {
                root.menuRequested(root.index, eventPoint.scenePosition)
                return
            }
            // The release mark's target: the indicator slot grown to the minimum hit size, centred
            // on the slot the way the NETWORK plus centres its own. One handler decides, the pointer
            // convention this tree already keeps, so the mark can never also activate the row.
            if (root.releaseAction.length > 0
                    && eventPoint.position.x >= dot.x + dot.width / 2 - Theme.hitMin / 2) {
                root.releaseRequested(root.releaseAction, Mounts.railKey(root.modelData))
                return
            }
            root.activated(root.index)
        }
    }
}
