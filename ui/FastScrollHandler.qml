import QtQuick
import "js/Scroll.js" as Scroll

// Writes the bounded position directly, on both axes. A MouseArea because a Flickable consumes wheel
// events before a child WheelHandler can answer them; acceptedButtons: Qt.NoButton leaves taps and
// drags alone. The arithmetic is ui/js/Scroll.js and the two rates are ui/Theme.qml's.
MouseArea {
    id: root

    required property var flickable
    property var ctrlWheelAction: null

    anchors.fill: parent
    acceptedButtons: Qt.NoButton
    z: 1000

    function scrollDistance(pixelDelta, angleDelta) {
        return Scroll.distance(pixelDelta, angleDelta, Application.styleHints.wheelScrollLines,
                               Theme.scroll.notchPx, Theme.scroll.multiplier)
    }

    onWheel: function (wheel) {
        if ((wheel.modifiers & Qt.ControlModifier) && root.ctrlWheelAction !== null) {
            wheel.accepted = root.ctrlWheelAction(wheel)
            if (wheel.accepted) return
        }
        var down = root.scrollDistance(wheel.pixelDelta.y, wheel.angleDelta.y)
        var across = root.scrollDistance(wheel.pixelDelta.x, wheel.angleDelta.x)
        if ((down === 0 && across === 0) || !root.flickable.interactive) {
            wheel.accepted = false
            return
        }
        var previousY = root.flickable.contentY
        var previousX = root.flickable.contentX
        root.flickable.cancelFlick()
        if (down !== 0)
            root.flickable.contentY = Scroll.bounded(previousY - down, root.flickable.originY,
                                                     root.flickable.contentHeight, root.flickable.height)
        // A tilt or a diagonal touchpad stroke pans a content wider than the view, a zoomed PDF page.
        if (across !== 0)
            root.flickable.contentX = Scroll.bounded(previousX - across, root.flickable.originX,
                                                     root.flickable.contentWidth, root.flickable.width)
        wheel.accepted = Scroll.moved(previousY, root.flickable.contentY) || Scroll.moved(previousX, root.flickable.contentX)
    }
}
