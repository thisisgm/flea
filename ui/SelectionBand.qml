import QtQuick
import qs.Commons
import "js/Filter.js" as Filter
import "js/Marquee.js" as Marquee
import "js/Scroll.js" as Scroll

MouseArea {
    id: root

    required property var pane
    required property var flickable
    property int columns: 1
    property real cellWidth: flickable.width
    property real cellHeight: Theme.fileRowHeight
    property var shown: pane ? pane.shown : null
    property int offset: 0
    readonly property int count: flickable.count
    property var gesture: null
    property bool tracking: false
    property point start: Qt.point(0, 0)
    property point anchor: Qt.point(0, 0)
    property bool additive: false
    readonly property bool bandActive: gesture !== null
    readonly property point end: root.endpoint()
    readonly property int scrollDirection: mouseY < 0 ? -1 : mouseY > height ? 1 : 0
    // One accelerated list-wheel notch per second, with frame time preserving the rate at every refresh rate.
    readonly property real scrollRate: Scroll.distance(0, Scroll.NOTCH_UNITS,
        Application.styleHints.wheelScrollLines, Theme.scroll.notchPx, Theme.scroll.multiplier)

    anchors.fill: parent
    z: 1
    acceptedButtons: Qt.LeftButton
    preventStealing: true
    enabled: pane !== null && !pane.listInFlight && pane.renamingIndex < 0 && pane.listingState === "ready"
    clip: true

    function endpoint() {
        return Qt.point(root.flickable.contentX + Math.max(0, Math.min(root.width, root.mouseX)),
                        root.flickable.contentY + Math.max(0, Math.min(root.height, root.mouseY)))
    }

    function update() {
        if (!root.gesture) return
        var position = root.endpoint()
        var box = Marquee.cells(root.anchor.x - root.flickable.originX, root.anchor.y - root.flickable.originY,
            position.x - root.flickable.originX, position.y - root.flickable.originY,
            root.cellWidth, root.cellHeight, root.columns, root.count)
        Marquee.update(root.pane, root.gesture, box, root.columns, root.count,
            position.x < root.anchor.x, position.y < root.anchor.y,
            function (index) { return root.offset + Filter.at(root.shown, index) })
    }

    function finish(cancelled) {
        if (!root.tracking) return
        var gesture = root.gesture
        root.gesture = null
        if (gesture) Marquee.finish(root.pane, gesture, cancelled)
        root.tracking = false
        if (root.pane.selectionBand === root) root.pane.selectionBand = null
    }

    function cancel() { root.finish(true) }

    onPressed: function (mouse) {
        if (root.flickable.indexAt(root.flickable.contentX + mouse.x, root.flickable.contentY + mouse.y) >= 0) {
            mouse.accepted = false
            return
        }
        root.flickable.cancelFlick()
        root.start = Qt.point(mouse.x, mouse.y)
        root.anchor = Qt.point(root.flickable.contentX + mouse.x, root.flickable.contentY + mouse.y)
        root.additive = (mouse.modifiers & Qt.ControlModifier) !== 0
        root.tracking = true
        root.pane.selectionBand = root
        root.pane.focusRequested()
        root.pane.focusView = "list"
        root.pane.listArea.forceActiveFocus()
    }
    onPositionChanged: function (mouse) {
        if (!root.tracking) return
        if (!root.gesture) {
            var dx = mouse.x - root.start.x, dy = mouse.y - root.start.y
            var threshold = Application.styleHints.startDragDistance
            if (dx * dx + dy * dy < threshold * threshold) return
            root.gesture = Marquee.begin(root.pane, root.additive)
        }
        root.update()
    }
    onReleased: { root.update(); root.finish(false) }
    onCanceled: root.cancel()
    onVisibleChanged: if (!visible) root.cancel()
    onEnabledChanged: if (!enabled) root.cancel()
    onWidthChanged: root.cancel()
    onHeightChanged: root.cancel()
    onColumnsChanged: root.cancel()
    onCellWidthChanged: root.cancel()
    onCellHeightChanged: root.cancel()
    onShownChanged: root.cancel()
    onOffsetChanged: root.cancel()
    onCountChanged: root.cancel()
    Component.onDestruction: root.cancel()

    Connections {
        target: root.flickable
        function onContentYChanged() { root.update() }
    }
    Connections {
        target: root.pane
        function onPathChanged() { root.cancel() }
    }

    FrameAnimation {
        running: root.bandActive && root.scrollDirection !== 0
        onTriggered: root.flickable.contentY = Scroll.bounded(
            root.flickable.contentY + root.scrollDirection * root.scrollRate * frameTime,
            root.flickable.originY, root.flickable.contentHeight, root.flickable.height)
    }

    Rectangle {
        visible: root.bandActive
        x: Math.min(root.anchor.x, root.end.x) - root.flickable.contentX
        y: Math.min(root.anchor.y, root.end.y) - root.flickable.contentY
        width: Math.abs(root.end.x - root.anchor.x)
        height: Math.abs(root.end.y - root.anchor.y)
        color: Qt.alpha(Theme.color.accent, Style.selectionFillAlpha)
        border { width: 1; color: Theme.color.accent }
        radius: 0
    }
}
