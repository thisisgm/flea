import QtQuick
import "." as Flea
import "js/Motion.js" as Motion

// Quick actions shown when a directory is empty: animated logo + rotating text above, actions below.
Item {
    id: root

    property bool active: false
    property int cursorIndex: 0
    property string currentDir: ""
    property bool gitCloneMode: false
    property string gitUrl: ""
    property int messageIndex: 0

    signal closed()
    signal action(string name, string dir)

    visible: root.active || content.opacity > 0

    // When cloning, show "Cloning..." instead of rotating text.
    readonly property bool cloning: root.gitCloneMode

    readonly property var messages: [
        "Nothing here yet",
        "A very tidy directory",
        "Not a file in sight",
        "This folder keeps its secrets",
        "Quiet in here",
        "No clutter to report",
        "Waiting for something to land",
        "Empty, and that is fine"
    ]
    readonly property int rotateMs: 2800
    readonly property color dim: Qt.darker(Theme.color.foreground, 1.4)

    // The five quick actions.
    readonly property var actions: [
        { name: "openAgent", label: "Open Agent", glyph: "terminal", key: "A" },
        { name: "gitClone", label: "Git Clone", glyph: "folder-git-2", key: "C" },
        { name: "terminal", label: "Terminal", glyph: "terminal", key: "T" },
        { name: "newFolder", label: "New Folder", glyph: "folder-plus", key: "N" }
    ]

    function open(dir) {
        root.currentDir = dir
        root.cursorIndex = 0
        root.gitCloneMode = false
        root.gitUrl = ""
        root.messageIndex = 0
        root.active = true
    }

    function close() {
        if (!root.active) return
        root.active = false
        root.gitCloneMode = false
        root.gitUrl = ""
        root.closed()
    }

    function moveCursor(delta) {
        if (root.gitCloneMode) return
        root.cursorIndex = Math.max(0, Math.min(root.actions.length - 1, root.cursorIndex + delta))
    }

    function activateCursor() {
        if (root.gitCloneMode) {
            runGitClone()
            return
        }
        var act = root.actions[root.cursorIndex]
        if (act === undefined) return
        if (act.name === "gitClone") {
            root.gitCloneMode = true
            root.gitUrl = ""
            return
        }
        root.action(act.name, root.currentDir)
        root.close()
    }

    function handleKey(key, text) {
        if (!root.active) return false
        if (root.gitCloneMode) {
            if (key === Qt.Key_Return || key === Qt.Key_Enter) {
                runGitClone()
                return true
            }
            if (key === Qt.Key_Escape) {
                root.close()
                return true
            }
            if (key === Qt.Key_Backspace) {
                root.gitUrl = root.gitUrl.slice(0, -1)
                return true
            }
            if (text.length === 1 && text >= " ") {
                root.gitUrl += text
                return true
            }
            return false
        }
        if (key === Qt.Key_Escape) { root.close(); return true }
        if (key === Qt.Key_Return || key === Qt.Key_Enter) { activateCursor(); return true }
        return false
    }

    function runGitClone() {
        if (root.gitUrl.length === 0) return
        root.action("gitClone:" + root.gitUrl, root.currentDir)
        root.close()
    }

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

            // Animated logo (same as original EmptyState)
            Flea.FleaMark {
                id: heroMark
                anchors.horizontalCenter: parent.horizontalCenter
                width: Theme.heroMarkSize
                height: Theme.heroMarkSize
            }

            // Rotating caption (same as original EmptyState)
            Text {
                id: caption
                anchors.horizontalCenter: parent.horizontalCenter
                text: root.cloning ? "CLONING..." : root.messages[root.messageIndex].toUpperCase()
                color: root.dim
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                font.bold: true
                font.letterSpacing: 1.2
                textFormat: Text.PlainText
            }

            // Git clone input mode
            Rectangle {
                visible: root.gitCloneMode
                width: Math.min(400, root.width - Theme.spacing.rowPaddingX * 4)
                height: Theme.rowHeight + Theme.spacing.rowPaddingY
                color: "transparent"
                border.color: Theme.color.muted
                border.width: 1
                anchors.horizontalCenter: parent.horizontalCenter

                TextInput {
                    id: gitInput
                    anchors.fill: parent
                    anchors.margins: Theme.spacing.rowPaddingX
                    color: Theme.color.foreground
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.bodySmall
                    text: root.gitUrl
                    cursorVisible: true
                    focus: root.active && root.gitCloneMode
                    enabled: root.active && root.gitCloneMode
                    clip: true
                    Keys.onReturnPressed: root.runGitClone()
                    Keys.onEnterPressed: root.runGitClone()
                    Keys.onEscapePressed: { root.gitCloneMode = false; root.gitUrl = "" }
                    Keys.onPressed: function (event) {
                        if (event.key === Qt.Key_Backspace) {
                            root.gitUrl = root.gitUrl.slice(0, -1)
                            event.accepted = true
                        }
                    }
                    onTextChanged: root.gitUrl = text
                }

                // Placeholder
                Text {
                    visible: root.gitUrl.length === 0
                    text: "Paste git URL and press Enter"
                    color: Theme.color.muted
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.bodySmall
                    anchors.centerIn: parent
                }
            }

            // Action list
            Repeater {
                model: root.gitCloneMode ? [] : root.actions
                delegate: Row {
                    anchors.horizontalCenter: parent.horizontalCenter
                    spacing: Theme.spacing.gap

                    Flea.Glyph {
                        name: modelData.glyph
                        color: index === root.cursorIndex ? Theme.color.accent : Theme.color.muted
                        maxSize: Theme.font.bodySmall
                        width: Theme.font.bodySmall
                        height: Theme.font.bodySmall
                        anchors.verticalCenter: parent.verticalCenter
                    }

                    Text {
                        text: modelData.label
                        font.family: Theme.font.family
                        font.pixelSize: Theme.font.bodySmall
                        color: index === root.cursorIndex ? Theme.color.accent : Theme.color.foreground
                        anchors.verticalCenter: parent.verticalCenter
                    }

                    Text {
                        text: modelData.key
                        font.family: Theme.font.family
                        font.pixelSize: Theme.font.caption
                        color: Theme.color.muted
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }
            }

            Text {
                visible: !root.gitCloneMode
                text: "j/k move · Enter select · Esc close"
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                color: Theme.color.muted
                anchors.horizontalCenter: parent.horizontalCenter
            }
        }
    }

    // Rotating caption timer (same as original EmptyState)
    Timer {
        interval: root.rotateMs
        running: root.active && !root.cloning && !Theme.reducedMotion
        repeat: true
        onTriggered: { fade.restart(); heroMark.replay() }
    }

    SequentialAnimation {
        id: fade
        PropertyAnimation { target: caption; property: "opacity"; to: 0.0; duration: 180; easing.type: Easing.OutQuad }
        ScriptAction { script: root.messageIndex = (root.messageIndex + 1) % root.messages.length }
        PropertyAnimation { target: caption; property: "opacity"; to: 1.0; duration: 260; easing.type: Easing.InQuad }
    }
}
