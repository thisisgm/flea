import QtQuick
import "." as Flea

// Both axes of a pannable document, sharing the trailing corner rather than painting through it.
Item {
    id: root
    required property var flickable
    anchors.fill: parent
    z: 2000

    Flea.ViewportScrollBar {
        parent: root
        anchors { top: root.top; right: root.right }
        flickable: root.flickable
        endInset: Theme.spacing.rowPaddingX
    }
    Flea.ViewportScrollBar {
        parent: root
        anchors { left: root.left; bottom: root.bottom }
        flickable: root.flickable
        orientation: Qt.Horizontal
        endInset: Theme.spacing.rowPaddingX
    }
}
