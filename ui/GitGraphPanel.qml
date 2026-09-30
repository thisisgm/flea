import QtQuick
import Quickshell
import qs.Commons
import "." as Flea
import "js/GitGraph.js" as GitGraph

// Overlay panel for the Switchboard-style git branch graph. Opens on Ctrl+G from the listing.
Item {
    id: root

    property bool opened: false
    property Item focusHolder: null
    property var pane: null
    property int requestId: 0
    property var commits: []
    property string branch: ""
    property string repoRoot: ""
    property string head: ""
    property string error: ""
    property bool loading: false

    readonly property int panelWidth: Math.min(width - 2 * clampMargin, Math.round(720 * Theme.font.bodySmall / 13))
    readonly property int clampMargin: 8
    readonly property real rowH: Theme.fileRowHeight
    readonly property real laneW: Math.round(14 * Theme.font.bodySmall / 13)
    readonly property real laneLeft: Theme.spacing.rowPaddingX
    readonly property int maxLane: GitGraph.maxWidth(root.commits)
    readonly property real lanePad: root.laneLeft + root.maxLane * root.laneW + 10

    function open(holder) {
        root.focusHolder = holder
        root.pane = holder
        root.opened = true
        root.forceActiveFocus()
        root.reload()
    }

    function close() {
        root.opened = false
        root.loading = false
        if (root.focusHolder) root.focusHolder.forceActiveFocus()
    }

    function reload() {
        if (!root.pane || !root.pane.backend || !root.pane.listingPath) {
            root.error = "No directory"
            root.commits = []
            return
        }
        root.loading = true
        root.error = ""
        root.commits = []
        root.requestId += 1
        root.pane.backend.askGitGraph(root.requestId, root.pane.listingPath, 100)
    }

    function takeGraph(id, path, repoRoot, head, branch, error, commits) {
        if (id !== root.requestId) return
        root.loading = false
        root.repoRoot = repoRoot || ""
        root.head = head || ""
        root.branch = branch || ""
        root.error = error || ""
        root.commits = commits || []
        canvas.requestPaint()
    }

    visible: root.opened
    focus: root.opened
    Keys.onPressed: function (event) {
        if (event.key === Qt.Key_Escape) { root.close(); event.accepted = true }
        else if (event.key === Qt.Key_R && (event.modifiers & Qt.ControlModifier)) { root.reload(); event.accepted = true }
    }

    MouseArea {
        anchors.fill: parent
        onClicked: root.close()
    }

    Rectangle {
        anchors.fill: parent
        color: Theme.color.background
        opacity: 0.45
    }

    Rectangle {
        id: card
        width: root.panelWidth
        height: Math.min(parent.height - 2 * root.clampMargin,
                         Math.round(root.rowH * 18 + Theme.chromeHeight + 2 * Theme.spacing.rowPaddingY))
        anchors.centerIn: parent
        radius: Style.cornerRadius
        color: Theme.color.surface
        border.width: Theme.spacing.hairline
        border.color: Theme.color.muted
        clip: true

        MouseArea { anchors.fill: parent; onClicked: {} }

        Column {
            anchors.fill: parent
            anchors.margins: Theme.spacing.hairline

            Item {
                width: parent.width
                height: Theme.chromeHeight
                Text {
                    anchors.left: parent.left
                    anchors.leftMargin: Theme.spacing.rowPaddingX
                    anchors.verticalCenter: parent.verticalCenter
                    text: GitGraph.title(root.branch, root.repoRoot)
                    color: Theme.color.foreground
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.body
                    textFormat: Text.PlainText
                }
                Text {
                    anchors.right: parent.right
                    anchors.rightMargin: Theme.spacing.rowPaddingX
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.loading ? "loading…" : (root.head ? root.head : "")
                    color: Theme.color.muted
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    textFormat: Text.PlainText
                }
                Rectangle {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    height: Theme.spacing.hairline
                    color: Theme.color.foreground
                    opacity: 0.12
                }
            }

            Item {
                width: parent.width
                height: parent.height - Theme.chromeHeight

                Text {
                    anchors.centerIn: parent
                    visible: !root.loading && root.commits.length === 0
                    text: root.error.length ? root.error : "No commits to graph."
                    color: Theme.color.muted
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.bodySmall
                    textFormat: Text.PlainText
                }

                Flickable {
                    id: flick
                    anchors.fill: parent
                    contentWidth: width
                    contentHeight: Math.max(height, root.commits.length * root.rowH)
                    clip: true
                    boundsBehavior: Flickable.StopAtBounds
                    visible: root.commits.length > 0

                    Canvas {
                        id: canvas
                        x: 0
                        y: 0
                        width: root.lanePad
                        height: Math.max(flick.height, root.commits.length * root.rowH)
                        onPaint: {
                            var ctx = getContext("2d")
                            ctx.reset()
                            var commits = root.commits
                            if (!commits || !commits.length) return
                            var rowH = root.rowH
                            var laneW = root.laneW
                            var left = root.laneLeft
                            function xOf(lane) { return left + Number(lane) * laneW }
                            function yOf(row) { return row * rowH + rowH / 2 }
                            for (var i = 0; i < commits.length; i++) {
                                var c = commits[i]
                                var y = yOf(i)
                                var nextY = i + 1 < commits.length ? yOf(i + 1) : y + rowH / 2
                                var through = c.through || []
                                for (var t = 0; t < through.length; t++) {
                                    var lane = through[t]
                                    ctx.strokeStyle = GitGraph.laneColor(lane)
                                    ctx.lineWidth = 2
                                    ctx.beginPath()
                                    ctx.moveTo(xOf(lane), y - rowH / 2)
                                    ctx.lineTo(xOf(lane), y + rowH / 2)
                                    ctx.stroke()
                                }
                                var edges = c.edges || []
                                for (var e = 0; e < edges.length; e++) {
                                    var edge = edges[e]
                                    var x1 = xOf(edge.from)
                                    var x2 = xOf(edge.to)
                                    var sameRow = Number(edge.to) === Number(c.lane) && Number(edge.from) !== Number(c.lane)
                                    ctx.strokeStyle = GitGraph.laneColor(edge.to)
                                    ctx.lineWidth = 2
                                    ctx.beginPath()
                                    if (sameRow) {
                                        ctx.moveTo(x1, y - 8)
                                        ctx.bezierCurveTo(x1, y, x2, y, x2, y)
                                    } else if (x1 === x2) {
                                        ctx.moveTo(x1, y)
                                        ctx.lineTo(x2, nextY)
                                    } else {
                                        ctx.moveTo(x1, y)
                                        ctx.bezierCurveTo(x1, y + 11, x2, nextY - 11, x2, nextY)
                                    }
                                    ctx.stroke()
                                }
                                ctx.fillStyle = GitGraph.laneColor(c.lane)
                                ctx.beginPath()
                                ctx.arc(xOf(c.lane), y, c.is_head ? 5.5 : 4, 0, Math.PI * 2)
                                ctx.fill()
                                if (c.is_head) {
                                    ctx.strokeStyle = Theme.color.foreground
                                    ctx.lineWidth = 1.5
                                    ctx.stroke()
                                }
                            }
                        }
                    }

                    Column {
                        id: rows
                        width: parent.width
                        Repeater {
                            model: root.commits
                            delegate: Flea.GitGraphRow {
                                required property var modelData
                                commit: modelData
                                lanePad: root.lanePad
                                width: rows.width
                            }
                        }
                    }
                }
            }
        }
    }
}
