import QtQuick
import Quickshell.Io
import "." as Flea
import "js/Motion.js" as Motion

// Agent picker overlay: shows available code agents, j/k to navigate, Enter to launch.
Item {
    id: root

    property bool active: false
    property var agents: []
    property int cursorIndex: 0
    property string currentDir: ""
    // Which agents are installed, filled by the detection processes.
    property var installed: ({})

    signal closed()
    signal launched(string agent, string dir)

    visible: root.active || content.opacity > 0

    function open(dir) {
        root.currentDir = dir
        root.installed = {}
        root.cursorIndex = 0
        root.active = true
        detectNext(0)
    }

    function close() {
        if (!root.active) return
        root.active = false
        root.closed()
    }

    function moveCursor(delta) {
        if (root.agents.length === 0) return
        if (root.cursorIndex < 0) {
            root.cursorIndex = 0
            return
        }
        root.cursorIndex = Math.max(0, Math.min(root.agents.length - 1, root.cursorIndex + delta))
    }

    function activateCursor() {
        var idx = root.cursorIndex
        if (idx < 0 || idx >= root.agents.length) {
            // No selection: launch the default (first) agent.
            idx = 0
        }
        var agent = root.agents[idx]
        if (agent === undefined) return
        root.launched(agent.cmd, root.currentDir)
        root.close()
    }

    // Check agents one by one using `which`.
    property var candidates: [
        { name: "Claude", cmd: "claude" },
        { name: "Codex", cmd: "codex" },
        { name: "OpenCode", cmd: "opencode" },
        { name: "Gemini", cmd: "gemini" },
        { name: "Pi", cmd: "pi" }
    ]
    property int detectIndex: 0

    function detectNext(i) {
        if (i >= candidates.length) {
            buildList()
            return
        }
        detectIndex = i
        whichProcess.command = ["which", candidates[i].cmd]
        whichProcess.running = true
    }

    Process {
        id: whichProcess
        onExited: function (exitCode) {
            var name = root.candidates[root.detectIndex].name
            var cmd = root.candidates[root.detectIndex].cmd
            if (exitCode === 0) {
                var d = root.installed
                d[cmd] = name
                root.installed = d
            }
            detectNext(root.detectIndex + 1)
        }
    }

    function buildList() {
        var found = []
        // Default agent first.
        var defaultAgent = defaultProcessOutput.trim()
        if (defaultAgent.length > 0 && root.installed[defaultAgent] !== undefined) {
            found.push({ name: root.installed[defaultAgent], cmd: defaultAgent })
        }
        for (var cmd in root.installed) {
            if (cmd !== defaultAgent) {
                found.push({ name: root.installed[cmd], cmd: cmd })
            }
        }
        root.agents = found
    }

    property string defaultProcessOutput: ""

    Component.onCompleted: {
        defaultProcess.command = ["omarchy-default-agent"]
        defaultProcess.running = true
    }

    Process {
        id: defaultProcess
        onExited: function () {
            // Output captured via stdout handler below.
        }
        stdout: SplitParser {
            onRead: function (line) { root.defaultProcessOutput = line }
        }
    }

    // Opaque so the real listing behind it never shows through.
    Rectangle {
        id: content
        x: 0
        width: root.width
        height: root.height
        color: Theme.color.background
        y: root.active ? 0 : Motion.translateUpPx
        opacity: root.active ? 1 : 0

        Behavior on y {
            enabled: root.active && !Theme.reducedMotion
            NumberAnimation { duration: Motion.durMs.open; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.bezierCurve }
        }
        Behavior on opacity {
            enabled: !Theme.reducedMotion
            NumberAnimation {
                duration: root.active ? Motion.durMs.open : Motion.durMs.close
                easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.bezierCurve
            }
        }

        Column {
            anchors.centerIn: parent
            spacing: Theme.spacing.gap

            Flea.Glyph {
                name: "terminal"
                color: Theme.color.foreground
                maxSize: Theme.stateMarkSize
                width: Theme.stateMarkSize
                height: Theme.stateMarkSize
                anchors.horizontalCenter: parent.horizontalCenter
            }

            Text {
                text: "OPEN AGENT"
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                font.bold: true
                font.letterSpacing: 1.2
                color: Theme.color.foreground
                anchors.horizontalCenter: parent.horizontalCenter
            }

            Text {
                visible: root.agents.length === 0
                text: "No agents found"
                font.family: Theme.font.family
                font.pixelSize: Theme.font.bodySmall
                color: Theme.color.muted
                anchors.horizontalCenter: parent.horizontalCenter
            }

            Repeater {
                model: root.agents
                delegate: Row {
                    id: agentRow
                    height: Theme.rowHeight
                    anchors.horizontalCenter: parent.horizontalCenter
                    spacing: Theme.spacing.gap

                    Flea.Glyph {
                        name: "terminal"
                        color: index === root.cursorIndex ? Theme.color.accent : Theme.color.muted
                        maxSize: Theme.font.bodySmall
                        width: Theme.font.bodySmall
                        height: Theme.font.bodySmall
                        anchors.verticalCenter: parent.verticalCenter
                    }

                    Text {
                        text: modelData.name
                        font.family: Theme.font.family
                        font.pixelSize: Theme.font.bodySmall
                        color: index === root.cursorIndex ? Theme.color.accent : Theme.color.foreground
                        anchors.verticalCenter: parent.verticalCenter
                    }

                    Text {
                        visible: index === 0
                        text: "(default)"
                        font.family: Theme.font.family
                        font.pixelSize: Theme.font.caption
                        color: Theme.color.muted
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }
            }

            Text {
                text: "j/k move · Enter launch · Escape close"
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                color: Theme.color.muted
                anchors.horizontalCenter: parent.horizontalCenter
            }
        }
    }
}
