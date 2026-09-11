import QtQuick
import "." as Flea

// Favourite records keep their exact label/path; only the drag handle initiates reordering.
Item {
    id: root
    property var row: ({})
    signal activated()
    signal actionPicked(int action)
    signal moved(int to)
    readonly property bool actions: root.row.kind === "favouriteActions"
    readonly property Item dragItem: grip
    function actionItem(index) { return actionButtons.itemAt(index) }
    implicitHeight: actions ? Theme.hitMin + 2 * Theme.spacing.hairline + Theme.settings.railPaddingY : Theme.railRowHeight

    Row {
        visible: root.actions
        x: Theme.spacing.rowPaddingX + Theme.markSize + Theme.spacing.gap
        y: 2 * Theme.spacing.hairline
        spacing: Theme.spacing.rowPaddingY + Theme.spacing.hairline
        Repeater {
            id: actionButtons
            model: ["Add current folder", "Remove"]
            delegate: Rectangle {
                id: actionButton
                required property int index
                required property string modelData
                enabled: index === 0 || root.row.canRemove === true
                width: content.implicitWidth + 2 * Theme.settings.railPaddingY
                height: Theme.hitMin
                color: "transparent"
                border.width: Theme.spacing.hairline
                border.color: enabled && index === root.row.actionIndex ? Theme.color.accent : Theme.color.muted
                Accessible.role: Accessible.Button
                Accessible.name: modelData
                Accessible.onPressAction: if (enabled) root.actionPicked(index)
                Row {
                    id: content
                    anchors.centerIn: parent
                    spacing: Theme.spacing.rowPaddingY - Theme.spacing.hairline
                    Flea.Glyph {
                        anchors.verticalCenter: parent.verticalCenter
                        width: Theme.font.caption
                        height: width
                        name: actionButton.index === 0 ? "plus" : "minus"
                        color: label.color
                    }
                    Text {
                        id: label
                        text: actionButton.modelData
                        color: !actionButton.enabled ? Theme.color.muted : actionButton.index === root.row.actionIndex ? Theme.color.accent : Theme.color.foreground
                        font.family: Theme.font.family
                        font.pixelSize: Theme.font.caption
                        textFormat: Text.PlainText
                    }
                }
                TapHandler { onTapped: root.actionPicked(index) }
            }
        }
    }
    Flea.Glyph {
        id: icon
        visible: !root.actions
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        width: Theme.railIconSize
        height: width
        name: root.row.glyph || "folder"
        color: root.row.error ? Theme.color.error : Theme.color.muted
    }
    Text {
        visible: !root.actions
        anchors.left: icon.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.right: path.left
        anchors.rightMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        text: root.row.label || ""
        font.family: Theme.font.family
        font.pixelSize: Theme.font.body
        color: root.row.error ? Theme.color.error : Theme.color.foreground
        elide: Text.ElideRight
        textFormat: Text.PlainText
    }
    Text {
        id: path
        visible: !root.actions
        anchors.right: grip.left
        anchors.rightMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        width: Math.min(implicitWidth, root.width * 0.46)
        text: root.row.value || ""
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        color: root.row.error ? Theme.color.error : Theme.color.muted
        elide: Text.ElideMiddle
        textFormat: Text.PlainText
    }
    Flea.Glyph {
        id: grip
        visible: !root.actions
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        width: Theme.hitMin
        height: Theme.hitMin
        name: "list"
        color: Theme.color.muted
        DragHandler {
            id: drag
            target: null
            xAxis.enabled: false
            property real startY: 0
            onActiveChanged: {
                if (active) { startY = persistentTranslation.y; return }
                // Qt clears active translation before this release callback.
                var to = Math.max(0, Math.min(Favourites.records.length - 1,
                    root.row.favouriteIndex + Math.round((persistentTranslation.y - startY) / Theme.railRowHeight)))
                if (to !== root.row.favouriteIndex) root.moved(to)
            }
        }
    }
    TapHandler {
        enabled: !root.actions
        onTapped: root.activated()
    }
}
