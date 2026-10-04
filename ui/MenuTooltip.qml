import QtQuick

// One overlay outside both menu clips; only overflowing text takes pointer input for scrolling.
Rectangle {
    id: root
    required property bool opened
    required property rect workArea
    property MenuRow row: null
    property bool ready: false
    readonly property bool eligible: root.opened && root.row !== null && root.row.visible
                                    && (root.row.hovered || hover.hovered) && root.row.current && root.row.tooltipText.length > 0
    readonly property point at: root.eligible ? root.row.mapToItem(root.parent, 0, root.row.height) : Qt.point(0, 0)
    readonly property real padding: Theme.spacing.gap

    // Once a name is revealed, adjacent rows reveal theirs immediately until the menu closes.
    onOpenedChanged: if (!root.opened) root.ready = false
    onRowChanged: scroll.contentY = 0
    Timer {
        interval: 400
        running: root.eligible && !root.ready
        onTriggered: root.ready = true
    }

    visible: root.eligible && root.ready
    z: 2
    width: Math.min(tip.implicitWidth + 2 * root.padding, Theme.menuWidth * 2, root.workArea.width)
    height: Math.min(tip.implicitHeight + 2 * root.padding, root.workArea.height)
    x: Math.max(root.workArea.x, Math.min(root.at.x, root.workArea.x + root.workArea.width - width))
    y: Math.max(root.workArea.y, root.at.y + height <= root.workArea.y + root.workArea.height
                ? root.at.y : root.at.y - (root.row ? root.row.height : 0) - height)
    color: Theme.color.surface
    border.color: Theme.color.muted
    border.width: Theme.spacing.hairline

    HoverHandler {
        id: hover
        enabled: scroll.interactive
        blocking: true
    }

    Flickable {
        id: scroll
        objectName: "tooltipScroll"
        x: root.padding
        y: root.padding
        width: Math.max(0, root.width - 2 * root.padding)
        height: Math.max(0, root.height - 2 * root.padding)
        clip: true
        contentWidth: width
        contentHeight: tip.implicitHeight
        interactive: contentHeight > height
        boundsBehavior: Flickable.StopAtBounds

        Text {
            id: tip
            objectName: "tooltipText"
            width: scroll.width
            text: root.row ? root.row.tooltipText : ""
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            color: Theme.color.foreground
            font.family: Theme.font.family
            font.pixelSize: Theme.font.body
        }
    }
}
