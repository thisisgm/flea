import QtQuick
import "js/Scroll.js" as Scroll

// Browser-speed vertical scrolling for Flea's three file views. This blocks Flickable's own wheel
// path, which ignored the useful touchpad magnitude on this box, while leaving pointer drags alone.
WheelHandler {
    required property var view

    target: null
    blocking: true
    acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad

    onWheel: function (event) {
        view.cancelFlick()
        view.contentY = Scroll.position(view.contentY, view.originY, view.contentHeight, view.height,
                                        event.pixelDelta.y, event.angleDelta.y, Theme.rowHeight)
        event.accepted = true
    }
}
