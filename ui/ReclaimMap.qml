import QtQuick
import qs.Commons
import "." as Flea
import "js/ReclaimTree.js" as ReclaimTree
import "js/ReclaimPaint.js" as ReclaimPaint
import "js/Format.js" as Format

// One scan, eight readings: the reclaim results drawn as a treemap, folder cards, a sunburst, a
// flame, bubbles, a mind map, ranked bars and an age map. b opens this over the results and
// cycles the readings once it is up; a click on anything lands the cursor on the row it drew.
// The strip's Quick Wins never deletes: it selects every tree the scan found and steps the
// overlay aside, so the seeing comes before the dd, and undo stays behind both.
Rectangle {
    id: root

    property var pane: null
    property string mode: "treemap"
    property var pickedNode: null
    // Fired by the strip's Quick Wins: everything the scan found is selected and the overlay
    // steps aside, so the operator sees the trees highlighted in the listing before acting.
    signal stageRequested()
    // Rebuilt beside every rows reply: one tree, and the mode strip is only a choice of reading.
    readonly property var model: pane ? ReclaimTree.build(pane.rows, pane.held, pane.path) : null
    readonly property int treeCount: model ? ReclaimTree.leaves(model).length : 0
    property var shapes: []
    readonly property var palette: ({
        accent: String(Theme.color.accent),
        bg: String(Theme.color.background),
        fg: String(Theme.color.foreground),
        muted: String(Theme.color.muted),
        onAccent: String(Theme.color.background),
        small: Math.round(Theme.font.caption) + "px \"" + Theme.font.family + "\""
    })

    color: Theme.color.background

    // b on an open map cycles to the next reading; the strip's buttons reach any of them directly.
    function cycle() {
        var at = ReclaimTree.MODES.indexOf(root.mode)
        root.mode = ReclaimTree.MODES[(at + 1) % ReclaimTree.MODES.length]
        canvas.requestPaint()
    }

    function hit(x, y) {
        for (var i = root.shapes.length - 1; i >= 0; i--) {
            var s = root.shapes[i]
            if (!s.node || s.node.row < 0) {
                continue
            }
            if (s.k === "rect") {
                if (x >= s.x && x <= s.x + s.w && y >= s.y && y <= s.y + s.h)
                    return s.node
            } else if (s.k === "dot" || s.k === "disc") {
                var dx = x - s.cx
                var dy = y - s.cy
                if (dx * dx + dy * dy <= s.r * s.r)
                    return s.node
            } else if (s.k === "arc") {
                var ax = x - s.cx
                var ay = y - s.cy
                var d2 = ax * ax + ay * ay
                if (d2 >= s.r0 * s.r0 && d2 <= s.r1 * s.r1) {
                    var a = Math.atan2(ay, ax)
                    while (a < s.a0) a += Math.PI * 2
                    if (a <= s.a1) return s.node
                }
            } else if (s.k === "clabel") {
                if (Math.abs(x - s.cx) <= s.w / 2 && Math.abs(y - s.cy) <= 8)
                    return s.node
            }
        }
        return null
    }

    // The strip: eight readings on the left, the staging shortcut on the right.
    Item {
        id: strip
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        height: Theme.chromeHeight

        Row {
            id: modes
            anchors.left: parent.left
            anchors.leftMargin: Theme.spacing.rowPaddingX
            anchors.verticalCenter: parent.verticalCenter
            spacing: Theme.spacing.gap

            Repeater {
                model: ReclaimTree.MODES

                delegate: Text {
                    required property var modelData
                    readonly property bool active: root.mode === modelData
                    text: ReclaimTree.LABELS[modelData]
                    color: active ? Theme.color.accent : Theme.color.muted
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    textFormat: Text.PlainText

                    HoverHandler { cursorShape: Qt.PointingHandCursor }

                    TapHandler {
                        acceptedButtons: Qt.LeftButton
                        onSingleTapped: root.mode = modelData
                    }
                }
            }
        }

        Rectangle {
            id: quickWins
            anchors.right: parent.right
            anchors.rightMargin: Theme.spacing.rowPaddingX
            anchors.verticalCenter: parent.verticalCenter
            radius: 4
            border.width: 1
            border.color: Theme.color.accent
            color: qwTap.pressed ? Theme.color.accent : "transparent"
            width: qwLabel.width + 16
            height: Theme.chromeHeight - 6

            Text {
                id: qwLabel
                anchors.centerIn: parent
                // Staging, not deleting: the press selects everything the scan found and hands
                // the listing back with the trees highlighted, so the seeing comes before the dd.
                text: "Quick Wins · stage " + root.treeCount + " trees (" + (root.model ? Format.size(root.model.bytes) : "0 B") + ")"
                color: qwTap.pressed ? Theme.color.background : Theme.color.accent
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                textFormat: Text.PlainText
            }

            HoverHandler { cursorShape: Qt.PointingHandCursor }

            TapHandler {
                id: qwTap
                acceptedButtons: Qt.LeftButton
                onSingleTapped: {
                    // Staging waits for the scan to finish: a walk still running would rank its
                    // rows once more and reshuffle the selection being handed back.
                    if (root.treeCount > 0 && root.pane && !root.pane.searchRunning) {
                        root.pane.selectAll()
                        root.stageRequested()
                    }
                }
            }
        }
    }

    Canvas {
        id: canvas
        anchors.top: strip.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        property var shapes: []

        onPaint: {
            if (!root.pane || !root.model) {
                return
            }
            var ctx = canvas.getContext("2d")
            var drawn = ReclaimTree.layout(root.mode, root.model, width, height, Date.now() / 1000)
            canvas.shapes = drawn
            root.shapes = drawn
            ReclaimPaint.draw(ctx, drawn, root.palette, root.pickedNode, width, height)
        }

        MouseArea {
            anchors.fill: parent
            onClicked: function (mouse) {
                var node = root.hit(mouse.x, mouse.y)
                if (node && root.pane) {
                    root.pickedNode = node
                    root.pane.setCursor(node.row)
                }
            }
        }
    }

    Connections {
        target: root

        function onModelChanged() { canvas.requestPaint() }
        function onModeChanged() { canvas.requestPaint() }
        function onPickedNodeChanged() { canvas.requestPaint() }
    }

    Component.onCompleted: canvas.requestPaint()
}
