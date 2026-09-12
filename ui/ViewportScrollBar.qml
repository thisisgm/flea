import QtQuick
import "." as Flea
import "js/Scroll.js" as Scroll

// One scrollbar for one Flickable axis. It is an overlay on the viewport, never content: no row,
// tile or model entry is created for it, and assigning contentX/Y reaches the view's existing
// settle and window-fetch paths exactly as a wheel does.
Item {
    id: root

    required property var flickable
    property int orientation: Qt.Vertical
    property var ctrlWheelAction: null
    property real endInset: 0

    readonly property bool vertical: root.orientation === Qt.Vertical
    readonly property real contentLength: root.vertical ? root.flickable.contentHeight : root.flickable.contentWidth
    readonly property real viewportLength: root.vertical ? root.flickable.height : root.flickable.width
    readonly property real origin: root.vertical ? root.flickable.originY : root.flickable.originX
    readonly property real contentPosition: root.vertical ? root.flickable.contentY : root.flickable.contentX
    readonly property real trackLength: Math.max(0, (root.vertical ? root.height : root.width) - root.endInset)
    readonly property real handleLength: Scroll.handleLength(root.trackLength, root.contentLength,
                                                              root.viewportLength, Theme.hitMin)
    readonly property real handleOffset: Scroll.handleOffset(root.contentPosition, root.origin,
                                                              root.contentLength, root.viewportLength,
                                                              root.trackLength, Theme.hitMin)
    readonly property bool overflow: Scroll.range(root.contentLength, root.viewportLength) > 0.5
    readonly property bool dragging: pointer.pressed && pointer.onHandle

    visible: root.overflow && root.flickable.visible
    width: root.vertical ? Theme.spacing.rowPaddingX : root.flickable.width
    height: root.vertical ? root.flickable.height : Theme.spacing.rowPaddingX
    z: 2000

    function setPosition(value) {
        root.flickable.cancelFlick()
        if (root.vertical)
            root.flickable.contentY = Scroll.bounded(value, root.origin, root.contentLength, root.viewportLength)
        else
            root.flickable.contentX = Scroll.bounded(value, root.origin, root.contentLength, root.viewportLength)
    }

    function page(direction) {
        root.setPosition(root.contentPosition + direction * root.viewportLength)
    }

    function localAt(mouse) {
        if (pointer.parent === root)
            return root.vertical ? mouse.y : mouse.x
        var p = pointer.mapToItem(root, mouse.x, mouse.y)
        return root.vertical ? p.y : p.x
    }

    function endDrag() {
        pointer.parent = root
        root.flickable.interactive = pointer.savedInteractive
        pointer.onHandle = false
    }

    Accessible.role: Accessible.ScrollBar
    Accessible.name: root.vertical ? "Vertical scroll bar" : "Horizontal scroll bar"
    Accessible.description: "Scroll position"
    Accessible.onIncreaseAction: root.page(1)
    Accessible.onDecreaseAction: root.page(-1)

    Rectangle {
        x: root.vertical ? Math.round((root.width - width) / 2) : 0
        y: root.vertical ? 0 : Math.round((root.height - height) / 2)
        width: root.vertical ? Theme.spacing.hairline : root.trackLength
        height: root.vertical ? root.trackLength : Theme.spacing.hairline
        color: Theme.color.muted
        opacity: 0.12
    }

    Rectangle {
        id: handle
        x: root.vertical ? Math.round((root.width - width) / 2) : root.handleOffset
        y: root.vertical ? root.handleOffset : Math.round((root.height - height) / 2)
        width: root.vertical ? Math.max(2 * Theme.spacing.hairline, Math.round(Theme.spacing.gap / 2))
                             : root.handleLength
        height: root.vertical ? root.handleLength
                              : Math.max(2 * Theme.spacing.hairline, Math.round(Theme.spacing.gap / 2))
        radius: Math.min(width, height) / 2
        color: pointer.pressed ? Theme.color.accent : pointer.containsMouse ? Theme.color.foreground : Theme.color.muted
        opacity: pointer.containsMouse || pointer.pressed ? 1 : 0.72
    }

    // The grab lives on this MouseArea. For the drag it is reparented onto the window so a
    // Flickable child sitting over recycling row DragHandlers cannot be the item Qt retargets.
    MouseArea {
        id: pointer
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton
        hoverEnabled: true
        preventStealing: true
        z: pointer.parent === root ? 0 : 1000000
        property bool onHandle: false
        property real grabOffset: 0
        property bool savedInteractive: true

        onPressed: function(mouse) {
            var at = root.localAt(mouse)
            var start = root.handleOffset
            pointer.onHandle = at >= start && at <= start + root.handleLength
            if (pointer.onHandle) {
                pointer.grabOffset = at - start
                pointer.savedInteractive = root.flickable.interactive
                root.flickable.interactive = false
                root.flickable.cancelFlick()
                var win = root.Window.window
                if (win && win.contentItem)
                    pointer.parent = win.contentItem
            } else {
                root.page(at < start ? -1 : 1)
            }
        }
        onPositionChanged: function(mouse) {
            if (!pointer.pressed || !pointer.onHandle)
                return
            var at = root.localAt(mouse) - pointer.grabOffset
            root.setPosition(Scroll.positionForHandle(at, root.origin, root.contentLength,
                root.viewportLength, root.trackLength, Theme.hitMin))
        }
        onReleased: root.endDrag()
        onCanceled: root.endDrag()
    }

    Flea.FastScrollHandler {
        parent: root
        flickable: root.flickable
        ctrlWheelAction: root.ctrlWheelAction
    }
}
