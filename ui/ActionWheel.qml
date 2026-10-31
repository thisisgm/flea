pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Shapes
import "." as Flea
import "js/ActionWheel.js" as Wheel

// Tangerine's drag wheel, given Flea's six file actions. The wheel is built on the first open by
// ui/Pane.qml, so a pane that never asks for it pays for neither the paths nor the labels.
Item {
    id: root
    objectName: "actionWheel"

    required property var pane

    property bool opened: false
    // -1 means the hub is showing; a sector is lit from the pointer or the keyboard.
    property int highlighted: -1
    readonly property int actionCount: Wheel.ACTIONS.length
    readonly property real inner: Wheel.GEOMETRY.inner
    readonly property real outer: Wheel.GEOMETRY.outer
    readonly property real backdrop: Wheel.GEOMETRY.backdrop

    // The wheel's centre in this overlay's coordinates. It is put under the pointer when a click
    // asked for it and in the pane's middle when the key did, then clamped so no petal leaves.
    property real wheelX: width / 2
    property real wheelY: height / 2

    // The pane keeps its Keys handler on the view, so the wheel has to hand focus back on close.
    property Item focusHolder: null

    signal closed()
    signal chosen(string action)

    function placeAt(point) {
        var margin = root.backdrop + 10
        var x = point ? point.x : root.width / 2
        var y = point ? point.y : root.height / 2
        root.wheelX = root.width > margin * 2 ? Math.max(margin, Math.min(root.width - margin, x)) : root.width / 2
        root.wheelY = root.height > margin * 2 ? Math.max(margin, Math.min(root.height - margin, y)) : root.height / 2
    }

    function openAt(scenePoint) {
        if (!root.pane) return
        root.focusHolder = root.Window.window ? root.Window.window.activeFocusItem : null
        var at = scenePoint ? root.mapFromItem(null, scenePoint.x, scenePoint.y) : null
        root.placeAt(at)
        root.highlighted = -1
        root.opened = true
        keyCatcher.forceActiveFocus()
    }

    function close() {
        if (!root.opened) return
        root.opened = false
        root.highlighted = -1
        var holder = root.focusHolder && root.focusHolder.visible && root.focusHolder.enabled ? root.focusHolder : null
        if (!holder && root.pane) holder = root.pane.listArea
        if (holder) holder.forceActiveFocus()
        root.focusHolder = null
        root.closed()
    }

    function step(delta) {
        if (root.highlighted < 0)
            root.highlighted = delta > 0 ? 0 : root.actionCount - 1
        else
            root.highlighted = (root.highlighted + delta + root.actionCount) % root.actionCount
    }

    function choose(index) {
        if (index < 0 || index >= root.actionCount) return
        var action = Wheel.ACTIONS[index]
        root.close()
        Wheel.run(index, root.pane)
        root.chosen(action.id)
    }

    function pointerAt(x, y) {
        root.highlighted = Wheel.indexAt(x - root.backdrop, y - root.backdrop, root.actionCount)
    }

    anchors.fill: parent
    visible: root.opened || dial.opacity > 0
    enabled: root.opened
    z: 100
    Accessible.role: Accessible.Dialog
    Accessible.name: "File action wheel"

    // A quiet wash over the pane tells the eye where the wheel is and makes the closing click
    // outside it read as a dismissal rather than a click on a row that happens to be there.
    Rectangle {
        anchors.fill: parent
        color: Qt.rgba(0, 0, 0, 0.10)
        opacity: dial.opacity
    }

    MouseArea {
        id: ground
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        hoverEnabled: true
        onClicked: root.close()
        onWheel: function (wheel) { wheel.accepted = true }
    }

    Item {
        id: dial
        objectName: "actionWheelDial"
        width: root.backdrop * 2
        height: root.backdrop * 2
        x: Math.round(root.wheelX - width / 2)
        y: Math.round(root.wheelY - height / 2)
        opacity: root.opened ? 1 : 0
        scale: root.opened ? 1 : 0.78
        transformOrigin: Item.Center

        Behavior on opacity {
            NumberAnimation { duration: Theme.reducedMotion ? 0 : 140; easing.type: Easing.OutCubic }
        }
        Behavior on scale {
            NumberAnimation { duration: Theme.reducedMotion ? 0 : 180; easing.type: Easing.OutCubic }
        }

        // The glass disc Tangerine draws under the petals.
        Rectangle {
            anchors.fill: parent
            radius: width / 2
            color: Qt.rgba(Theme.color.surface.r, Theme.color.surface.g, Theme.color.surface.b, 0.78)
            border.width: Theme.spacing.hairline
            border.color: Qt.rgba(Theme.color.mutedFrame.r, Theme.color.mutedFrame.g, Theme.color.mutedFrame.b, 0.65)
        }

        Repeater {
            model: Wheel.ACTIONS
            delegate: Shape {
                id: petal
                objectName: "actionWheelPetal"
                required property var modelData
                required property int index

                readonly property string outlinePath: Wheel.sectorPath(petal.index, root.actionCount, root.backdrop, root.backdrop)
                width: root.backdrop * 2
                height: root.backdrop * 2
                antialiasing: true
                preferredRendererType: Shape.CurveRenderer

                ShapePath {
                    fillColor: petal.index === root.highlighted
                               ? Theme.color.accentFrame
                               : Qt.rgba(Theme.color.surface.r, Theme.color.surface.g, Theme.color.surface.b, 0.94)
                    strokeColor: petal.index === root.highlighted
                                 ? Theme.color.accent
                                 : Qt.rgba(Theme.color.mutedFrame.r, Theme.color.mutedFrame.g, Theme.color.mutedFrame.b, 0.7)
                    strokeWidth: Math.max(1, Theme.spacing.hairline)
                    PathSvg { path: petal.outlinePath }
                }
            }
        }

        Repeater {
            model: Wheel.ACTIONS
            delegate: Item {
                id: caption
                objectName: "actionWheelCaption"
                required property var modelData
                required property int index

                readonly property point at: Wheel.labelPoint(caption.index, root.actionCount)
                width: 92
                height: 42
                x: root.backdrop + caption.at.x - width / 2
                y: root.backdrop + caption.at.y - height / 2

                Accessible.role: Accessible.Button
                Accessible.name: caption.modelData.label
                Accessible.onPressAction: root.choose(caption.index)

                Column {
                    anchors.centerIn: parent
                    spacing: 1

                    Flea.Glyph {
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: 15
                        height: 15
                        maxSize: 15
                        name: caption.modelData.glyph
                        color: caption.index === root.highlighted ? Theme.color.background : Theme.color.foreground
                    }

                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: 92
                        text: caption.modelData.label
                        horizontalAlignment: Text.AlignHCenter
                        elide: Text.ElideRight
                        color: caption.index === root.highlighted ? Theme.color.background : Theme.color.foreground
                        font.family: Theme.font.family
                        font.pixelSize: Theme.font.bodySmall
                        font.weight: Font.DemiBold
                    }
                }
            }
        }

        // Tangerine's centre pill: the wheel's name at rest and the hovered action's name while a
        // sector is lit, which is the only text a pointer user needs to read.
        Rectangle {
            id: hub
            objectName: "actionWheelHub"
            anchors.centerIn: dial
            width: 92
            height: 42
            radius: height / 2
            color: Qt.rgba(Theme.color.surface.r, Theme.color.surface.g, Theme.color.surface.b, 0.95)
            border.width: Theme.spacing.hairline
            border.color: root.highlighted >= 0 ? Theme.color.accent : Theme.color.mutedFrame

            Column {
                anchors.centerIn: parent
                spacing: 0

                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: 82
                    text: Wheel.centerText(root.highlighted)
                    horizontalAlignment: Text.AlignHCenter
                    elide: Text.ElideRight
                    color: root.highlighted >= 0 ? Theme.color.accent : Theme.color.foreground
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.bodySmall
                    font.weight: Font.DemiBold
                }

                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: root.highlighted >= 0 ? "Enter to run" : root.actionCount + " file actions"
                    color: Theme.color.muted
                    font.family: Theme.font.family
                    font.pixelSize: Math.max(8, Theme.font.caption - 2)
                }
            }
        }

        // One hit area for the whole annulus, the same way Tangerine's one Ke() answers all petals.
        // The hub is deliberately inside it: a click there chooses nothing and a move there clears
        // the highlight rather than leaving a stale sector lit behind the pointer.
        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton
            onEntered: root.pointerAt(mouseX, mouseY)
            onPositionChanged: function (mouse) { root.pointerAt(mouse.x, mouse.y) }
            onExited: root.highlighted = -1
            onClicked: function (mouse) {
                var index = Wheel.indexAt(mouse.x - root.backdrop, mouse.y - root.backdrop, root.actionCount)
                if (index >= 0) root.choose(index)
            }
        }
    }

    // The wheel is modal while it is up. Every key is taken here so a wheel cannot leave a listing
    // step behind it, and the same four directions Tangerine reads work whichever preset is loaded.
    Item {
        id: keyCatcher
        objectName: "actionWheelKeys"
        anchors.fill: parent
        focus: true

        Keys.onPressed: function (event) {
            if (event.key === Qt.Key_Escape) root.close()
            else if (event.key === Qt.Key_Left || event.key === Qt.Key_Up || event.text === "h" || event.text === "k") root.step(-1)
            else if (event.key === Qt.Key_Right || event.key === Qt.Key_Down || event.text === "l" || event.text === "j") root.step(1)
            else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
                if (root.highlighted < 0) root.highlighted = 0
                else root.choose(root.highlighted)
            }
            event.accepted = true
        }
    }
}
