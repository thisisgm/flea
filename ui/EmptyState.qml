import QtQuick
import "js/Motion.js" as Motion

// The empty-directory touch: the operator's mark above a rotating caption, matched exact off /usr/share/omarchy/shell/Ui/PanelHero.qml and the tailscale/airpods phraseTimer pair (see AGENTS.md for the quoted values); visibility and geometry come from the caller (ui/shell.qml) via Pane's listingState and listArea.
Item {
    id: root

    property int messageIndex: 0
    // A fixed caption with its own mark, for a state that is not "this directory is empty": the
    // search's no-match answer names the query, so the rotation would be wrong there.
    property string caption: ""
    property string mark: ""
    // Sentence case, not uppercased: the rotating caption is OEM all-caps; this is the next action.
    property string hint: ""
    // Sentence case, matching the OEM's activePhrases; caption.text below uppercases at render, like PanelHero, not in the source.
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
    // The OEM phraseTimer's own interval, tailscale's and airpods' Panel.qml.
    readonly property int rotateMs: 2800
    // PanelHero's own secondary ink: Qt.darker(foreground, 1.4), not the row's muted role.
    readonly property color dim: Qt.darker(Theme.color.foreground, 1.4)

    // Entrance only: the caller (shell.qml) cuts visible straight to false on the way out, so
    // this only ever animates the appear direction, never a disappearance.
    opacity: root.visible ? 1 : 0

    Behavior on opacity {
        enabled: root.visible && !Motion.reduced
        NumberAnimation { duration: Motion.durMs.open; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.bezierCurve }
    }

    Column {
        id: content
        anchors.centerIn: parent
        anchors.verticalCenterOffset: root.visible ? 0 : Motion.translateUpPx
        spacing: Theme.spacing.gap

        Behavior on anchors.verticalCenterOffset {
            enabled: root.visible && !Motion.reduced
            NumberAnimation { duration: Motion.durMs.open; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.bezierCurve }
        }

        FleaMark {
            id: heroMark
            visible: root.caption.length === 0
            anchors.horizontalCenter: parent.horizontalCenter
            // The brand moment, which States.dc.html draws at 48; four row heights was 148.
            width: Theme.heroMarkSize
            height: Theme.heroMarkSize
        }

        Glyph {
            visible: root.caption.length > 0
            anchors.horizontalCenter: parent.horizontalCenter
            // A search that found nothing is a failure to find, not an arrival: Search.dc.html draws 40.
            maxSize: Theme.stateMarkSize
            width: Theme.stateMarkSize
            height: Theme.stateMarkSize
            name: root.mark
            color: root.dim
        }

        Text {
            id: caption
            anchors.horizontalCenter: parent.horizontalCenter
            text: (root.caption.length > 0 ? root.caption : root.messages[root.messageIndex]).toUpperCase()
            color: root.dim
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            font.bold: true
            font.letterSpacing: 1.2
            textFormat: Text.PlainText
        }

        Text {
            visible: root.hint.length > 0
            anchors.horizontalCenter: parent.horizontalCenter
            text: root.hint
            color: root.dim
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            textFormat: Text.PlainText
        }
    }

    // root.visible mirrors the caller's listingState === "empty" gate, so the rotation timer stops once a real listing arrives.
    Timer {
        interval: root.rotateMs
        running: root.visible && root.caption.length === 0 && !Motion.reduced
        repeat: true
        // One beat for both, per the operator: the mark repaints as the sentence changes.
        onTriggered: { fade.restart(); heroMark.replay() }
    }

    // The OEM's phraseSwap SequentialAnimation (tailscale's and airpods' Panel.qml): fade out, swap the index, fade in.
    SequentialAnimation {
        id: fade
        PropertyAnimation { target: caption; property: "opacity"; to: 0.0; duration: 180; easing.type: Easing.OutQuad }
        ScriptAction { script: root.messageIndex = (root.messageIndex + 1) % root.messages.length }
        PropertyAnimation { target: caption; property: "opacity"; to: 1.0; duration: 260; easing.type: Easing.InQuad }
    }
}
