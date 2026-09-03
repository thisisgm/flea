import QtQuick
import qs.Commons

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

    // A share or a removable volume carries a mount-state dot; the internal disk is always there
    // and always mounted, so a dot on it would say nothing, and a favourite is not a mount at all.
    readonly property bool showsDot: (root.modelData.group === "network" && root.modelData.kind !== "dropbox")
        || (root.modelData.group === "device" && root.modelData.kind === "volume")
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
            ? (root.focused ? Style.selectedFill : Style.normalFill)
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

    Component {
        id: glyphMark
        Glyph {
            name: root.modelData.glyph
            color: root.cursor ? Theme.color.foreground : Theme.color.muted
        }
    }

    // The brand marks are reproductions, so they take their own component rather than Glyph sizing.
    Component {
        id: dropboxMark
        DropboxMark {
            iconSize: Theme.railIconSize
            color: root.cursor ? Theme.color.foreground : Theme.color.muted
        }
    }

    Text {
        visible: !root.renaming
        anchors.left: mark.right
        anchors.leftMargin: Style.spacing.rowGap
        anchors.right: dot.left
        anchors.rightMargin: Style.spacing.rowGap
        anchors.verticalCenter: parent.verticalCenter
        text: root.modelData.label
        color: root.cursor ? Theme.color.foreground : Theme.color.muted
        font.family: Theme.font.family
        font.pixelSize: Theme.font.bodySmall
        elide: Text.ElideRight
        textFormat: Text.PlainText
    }

    // Same slot as the label above; only one of the two is ever visible. ui/RenameField.qml is the
    // one editor in the product, so the rail gets rail type, an accent hairline frame and stem-only
    // preselection where the stock control brought body type, its own 30 px height and a filled ground.
    RenameField {
        id: renameField
        visible: root.renaming
        anchors.left: mark.right
        anchors.leftMargin: Style.spacing.rowGap
        anchors.right: dot.left
        anchors.rightMargin: Style.spacing.rowGap
        anchors.verticalCenter: parent.verticalCenter
        height: Theme.railRowHeight - 2 * Theme.spacing.rowPaddingY
        name: root.modelData.label
        onCommitted: function (newName) { root.renameCommitted(root.index, newName) }
        onAbandoned: root.renameCancelled(root.index)
    }

    // What the editor holds right now, for tests through ui/Ipc.qml's railRenameEditorText.
    readonly property string editorText: renameField.current
    readonly property bool editorShown: renameField.visible

    // Every right-aligned mark in the rail is centred in a caption-wide slot, so this dot and the
    // NETWORK header's "+" share one centre line whatever their ink does: align by slot, never by ink.
    Item {
        id: dot
        visible: root.showsDot
        anchors.right: parent.right
        anchors.rightMargin: Style.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        width: Theme.font.caption
        height: Theme.font.caption

        // Green once gio mount -l lists it, muted at half strength while it is only a bookmark waiting
        // to be mounted. A square, not a disc: the cut is hard corners, and the canvas draws it square.
        Rectangle {
            anchors.centerIn: parent
            width: root.dotSize
            height: root.dotSize
            color: root.modelData.mounted ? Theme.color.executable : Theme.color.muted
            opacity: root.modelData.mounted ? 1 : root.unmountedOpacity
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
            root.activated(root.index)
        }
    }
}
