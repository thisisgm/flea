import QtQuick
import "." as Flea
import "js/Motion.js" as Motion

// The empty directory's next moves. The quiet EmptyState remains behind this
// compact card; this item owns only the actionable part of the empty view.
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

    readonly property var actions: [
        { name: "openAgent", label: "Open Agent", glyph: "terminal", key: "A" },
        { name: "openAgentPicker", label: "Agent Picker", glyph: "list-filter", key: "P" },
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
            root.openGitClone()
            return
        }
        root.action(act.name, root.currentDir)
        root.close()
    }

    // C reaches this directly; selecting the Git Clone row reaches it too.
    function openGitClone() {
        root.gitCloneMode = true
        gitInput.forceActiveFocus()
    }

    function handleKey(key, text) {
        if (!root.active || root.gitCloneMode) return false
        if (key === Qt.Key_Escape) {
            root.close()
            return true
        }
        if (key === Qt.Key_Return || key === Qt.Key_Enter) {
            root.activateCursor()
            return true
        }
        return false
    }

    function runGitClone() {
        if (root.gitUrl.length === 0) return
        root.action("gitClone:" + root.gitUrl, root.currentDir)
        root.close()
    }

    Rectangle {
        id: content
        anchors.centerIn: parent
        width: Math.min(420, Math.max(0, root.width - Theme.spacing.rowPaddingX * 4))
        height: Math.min(root.height - Theme.spacing.gap * 2,
                         panel.implicitHeight + Theme.spacing.rowPaddingY * 4)
        color: Theme.color.background
        border.color: Theme.color.muted
        border.width: Theme.spacing.hairline
        radius: Theme.spacing.gap
        clip: true
        opacity: root.active ? 1 : 0
        scale: root.active ? 1 : 0.96

        Behavior on opacity {
            enabled: !Theme.reducedMotion
            NumberAnimation {
                duration: root.active ? Motion.durMs.open : Motion.durMs.close
                easing.type: Easing.BezierSpline
                easing.bezierCurve: Motion.bezierCurve
            }
        }
        Behavior on scale {
            enabled: !Theme.reducedMotion
            NumberAnimation {
                duration: root.active ? Motion.durMs.open : Motion.durMs.close
                easing.type: Easing.BezierSpline
                easing.bezierCurve: Motion.bezierCurve
            }
        }

        Column {
            id: panel
            anchors.centerIn: parent
            width: parent.width - Theme.spacing.rowPaddingX * 4
            spacing: Theme.spacing.gap

            Flea.FleaMark {
                id: heroMark
                anchors.horizontalCenter: parent.horizontalCenter
                width: Theme.heroMarkSize
                height: Theme.heroMarkSize
            }

            Text {
                id: caption
                anchors.horizontalCenter: parent.horizontalCenter
                text: (root.gitCloneMode ? "Clone into this folder" : root.messages[root.messageIndex]).toUpperCase()
                color: root.dim
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                font.bold: true
                font.letterSpacing: 1.2
                textFormat: Text.PlainText
            }

            Rectangle {
                visible: root.gitCloneMode
                width: parent.width
                height: Theme.rowHeight + Theme.spacing.rowPaddingY
                color: "transparent"
                border.color: Theme.color.accent
                border.width: Theme.spacing.hairline
                radius: Theme.spacing.gap

                TextInput {
                    id: gitInput
                    anchors.fill: parent
                    anchors.margins: Theme.spacing.rowPaddingX
                    color: Theme.color.foreground
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.bodySmall
                    verticalAlignment: TextInput.AlignVCenter
                    text: root.gitUrl
                    cursorVisible: true
                    focus: root.gitCloneMode
                    clip: true
                    Keys.onReturnPressed: root.runGitClone()
                    Keys.onEnterPressed: root.runGitClone()
                    Keys.onEscapePressed: {
                        root.close()
                    }
                    Keys.onPressed: function (event) {
                        if (event.key === Qt.Key_Backspace) {
                            root.gitUrl = root.gitUrl.slice(0, -1)
                            event.accepted = true
                        }
                    }
                    onTextChanged: root.gitUrl = text
                }

                Text {
                    anchors.left: parent.left
                    anchors.leftMargin: Theme.spacing.rowPaddingX
                    anchors.verticalCenter: parent.verticalCenter
                    visible: root.gitUrl.length === 0
                    text: "Type or paste git URL, then Enter"
                    color: Theme.color.muted
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.bodySmall
                }
            }

            Column {
                visible: !root.gitCloneMode
                width: parent.width
                spacing: Theme.spacing.hairline

                Repeater {
                    model: root.actions
                    delegate: Rectangle {
                        width: parent.width
                        height: Theme.rowHeight
                        color: index === root.cursorIndex ? Theme.color.surface : "transparent"
                        border.color: Theme.color.accent
                        border.width: index === root.cursorIndex ? Theme.spacing.hairline : 0
                        radius: Theme.spacing.gap

                        Row {
                            anchors.fill: parent
                            anchors.leftMargin: Theme.spacing.rowPaddingX
                            anchors.rightMargin: Theme.spacing.rowPaddingX
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
                                width: parent.width - keyHint.width - Theme.font.bodySmall - Theme.spacing.gap * 2
                                anchors.verticalCenter: parent.verticalCenter
                                text: modelData.label
                                color: index === root.cursorIndex ? Theme.color.accent : Theme.color.foreground
                                font.family: Theme.font.family
                                font.pixelSize: Theme.font.bodySmall
                                elide: Text.ElideRight
                            }

                            Text {
                                id: keyHint
                                anchors.verticalCenter: parent.verticalCenter
                                visible: modelData.key.length > 0
                                text: modelData.key
                                color: Theme.color.muted
                                font.family: Theme.font.family
                                font.pixelSize: Theme.font.bodySmall
                            }
                        }

                        MouseArea {
                            anchors.fill: parent
                            hoverEnabled: true
                            onEntered: root.cursorIndex = index
                            onClicked: {
                                root.cursorIndex = index
                                root.activateCursor()
                            }
                        }
                    }
                }
            }

            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: !root.gitCloneMode
                text: "C clone · j/k move · Enter select · Esc close"
                color: Theme.color.muted
                font.family: Theme.font.family
                font.pixelSize: Theme.font.bodySmall
            }

            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: root.gitCloneMode
                text: "Esc returns to the action list"
                color: Theme.color.muted
                font.family: Theme.font.family
                font.pixelSize: Theme.font.bodySmall
            }
        }
    }

    Timer {
        interval: root.rotateMs
        running: root.active && !root.gitCloneMode && !Theme.reducedMotion
        repeat: true
        onTriggered: {
            fade.restart()
            heroMark.replay()
        }
    }

    SequentialAnimation {
        id: fade
        PropertyAnimation { target: caption; property: "opacity"; to: 0.0; duration: 180; easing.type: Easing.OutQuad }
        ScriptAction { script: root.messageIndex = (root.messageIndex + 1) % root.messages.length }
        PropertyAnimation { target: caption; property: "opacity"; to: 1.0; duration: 260; easing.type: Easing.InQuad }
    }
}
