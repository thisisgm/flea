import QtQuick

// Multiplies both smooth touchpad deltas and discrete wheel notches, then writes the bounded
// position directly. A MouseArea is intentional: a Flickable consumes wheel events before a child
// WheelHandler can answer them, while acceptedButtons: Qt.NoButton leaves taps and drags alone.
MouseArea {
    id: root

    required property var flickable
    property real speedMultiplier: 4
    property real mouseWheelStep: Math.max(1,
        Number(Application.styleHints.wheelScrollLines) || 3) * 24

    anchors.fill: parent
    acceptedButtons: Qt.NoButton
    z: 1000

    function scrollDistance(pixelDeltaY, angleDeltaY) {
        var distance = Number(pixelDeltaY) || 0
        if (distance === 0)
            distance = (Number(angleDeltaY) || 0) / 120 * root.mouseWheelStep
        return distance * root.speedMultiplier
    }

    function boundedContentY(value) {
        var minimum = Number(root.flickable.originY) || 0
        var maximum = Math.max(minimum,
                               minimum + Math.max(0, Number(root.flickable.contentHeight) || 0)
                               - Math.max(0, Number(root.flickable.height) || 0))
        return Math.max(minimum, Math.min(maximum, value))
    }

    function scrollByDeltas(pixelDeltaY, angleDeltaY) {
        var distance = root.scrollDistance(pixelDeltaY, angleDeltaY)
        if (distance === 0 || !root.flickable.interactive)
            return root.flickable.contentY
        var previous = root.flickable.contentY
        root.flickable.cancelFlick()
        root.flickable.contentY = root.boundedContentY(root.flickable.contentY - distance)
        return root.flickable.contentY
    }

    onWheel: function (wheel) {
        var distance = root.scrollDistance(wheel.pixelDelta.y, wheel.angleDelta.y)
        if (distance === 0 || !root.flickable.interactive) {
            wheel.accepted = false
            return
        }
        var previous = root.flickable.contentY
        var current = root.scrollByDeltas(wheel.pixelDelta.y, wheel.angleDelta.y)
        wheel.accepted = Math.abs(current - previous) > 0.01
    }
}
