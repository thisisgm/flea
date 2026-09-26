import QtQuick
import "." as Flea
import "js/Scroll.js" as Scroll

// An overlay on the viewport, never content; inside a Flickable it anchors to parent, never the Flickable id.
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
    readonly property bool overflow: Scroll.range(root.contentLength, root.viewportLength) > Scroll.OVERFLOW_PX
    readonly property bool dragging: pointer.pressed && pointer.onHandle
    // Finder's overlay scroller: drawn only while the view moves, the pointer is in the lane or a press is down.
    property bool moving: false
    // A new listing resets the position in the same frame its length changes, which is not a scroll.
    property bool settling: false
    // A HoverHandler, not the MouseArea's containsMouse: a press reparents that area onto the window, and its hover goes stale.
    readonly property bool inLane: lane.hovered
    // Keep the handle visible whenever the viewport has content to scroll.
    readonly property bool shown: root.overflow
    readonly property bool wide: root.inLane || pointer.pressed
    readonly property real knobWidth: Math.round((root.wide ? Scroll.KNOB_WIDE_PX : Scroll.KNOB_REST_PX) * Theme.sizeRatio)
    readonly property real knobInset: Math.round(Scroll.KNOB_INSET_PX * Theme.sizeRatio)
    readonly property real knobAlpha: pointer.pressed ? Scroll.KNOB_PRESSED_ALPHA
                                                      : root.wide ? Scroll.KNOB_HOVER_ALPHA : Scroll.KNOB_ALPHA

    onContentLengthChanged: {
        root.settling = true
        Qt.callLater(function() { root.settling = false })
    }
    onContentPositionChanged: {
        if (root.settling)
            return
        root.moving = true
        hold.restart()
    }

    HoverHandler {
        id: lane
    }

    Timer {
        id: hold
        interval: Scroll.HOLD_MS
        onTriggered: root.moving = false
    }

    visible: root.overflow && root.flickable.visible
    width: root.vertical ? Theme.spacing.rowPaddingX : root.flickable.width
    height: root.vertical ? root.flickable.height : Theme.spacing.rowPaddingX
    z: Scroll.BAR_Z

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

    // Every press saved the view's own flag; a second end (released, then canceled) finds onHandle false.
    function endDrag() {
        pointer.parent = root
        if (pointer.onHandle)
            root.flickable.interactive = pointer.savedInteractive
        pointer.onHandle = false
    }

    Accessible.role: Accessible.ScrollBar
    Accessible.name: root.vertical ? "Vertical scroll bar" : "Horizontal scroll bar"
    Accessible.description: "Scroll position"
    Accessible.onIncreaseAction: root.page(1)
    Accessible.onDecreaseAction: root.page(-1)

    // Everything stays drawn while content overflows the viewport.
    Item {
        anchors.fill: parent
        opacity: root.shown ? 1 : 0
        Behavior on opacity {
            enabled: !root.shown && !Theme.reducedMotion
            NumberAnimation { duration: Scroll.FADE_MS }
        }

        // The track fills the lane with the pointer in it, with a hairline on the content side.
        Rectangle {
            visible: root.wide
            x: 0
            y: 0
            width: root.vertical ? root.width : root.trackLength
            height: root.vertical ? root.trackLength : root.height
            color: Qt.rgba(Theme.color.foreground.r, Theme.color.foreground.g, Theme.color.foreground.b, Scroll.TRACK_ALPHA)
            Rectangle {
                width: root.vertical ? Theme.spacing.hairline : parent.width
                height: root.vertical ? parent.height : Theme.spacing.hairline
                color: Qt.rgba(Theme.color.foreground.r, Theme.color.foreground.g, Theme.color.foreground.b, Scroll.TRACK_LINE_ALPHA)
            }
        }

        Rectangle {
            id: handle
            x: root.vertical ? root.width - root.knobInset - width : root.handleOffset
            y: root.vertical ? root.handleOffset : root.height - root.knobInset - height
            width: root.vertical ? root.knobWidth : root.handleLength
            height: root.vertical ? root.handleLength : root.knobWidth
            radius: Math.min(width, height) / 2
            color: Qt.rgba(Theme.color.foreground.r, Theme.color.foreground.g, Theme.color.foreground.b, root.knobAlpha)
        }
    }

    // The drag reparents this grab onto the window, so no Flickable child over recycling rows becomes Qt's retarget.
    MouseArea {
        id: pointer
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton
        hoverEnabled: true
        preventStealing: true
        z: pointer.parent === root ? 0 : Scroll.GRAB_Z
        property bool onHandle: false
        property real grabOffset: 0
        property bool savedInteractive: true

        onPressed: function(mouse) {
            var at = root.localAt(mouse)
            var start = root.handleOffset
            if (at < start || at > start + root.handleLength) {
                // Finder's jump to the spot clicked: the knob's centre goes under the pointer, and the press drags on from there.
                var room = Scroll.travel(root.trackLength, root.contentLength, root.viewportLength, Theme.hitMin)
                start = Scroll.jumpOffset(at, root.handleLength, room)
                root.setPosition(Scroll.positionForHandle(start, root.origin, root.contentLength,
                    root.viewportLength, root.trackLength, Theme.hitMin))
            }
            pointer.onHandle = true
            pointer.grabOffset = at - start
            pointer.savedInteractive = root.flickable.interactive
            root.flickable.interactive = false
            root.flickable.cancelFlick()
            var win = root.Window.window
            if (win && win.contentItem)
                pointer.parent = win.contentItem
        }
        onPositionChanged: function(mouse) {
            if (!pointer.pressed || !pointer.onHandle)
                return
            // A track no longer than the minimum handle leaves it no travel, and a drag with none moves nothing.
            if (Scroll.travel(root.trackLength, root.contentLength, root.viewportLength, Theme.hitMin) === 0)
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
