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
    // The hero mark alone, for ui/Ipc.qml's painted-pixel count: the hint would light the wider rectangle.
    readonly property alias markItem: heroMark
    readonly property alias captionItem: caption
    // Sentence case, matching the OEM's activePhrases; caption.text below uppercases at render, like PanelHero, not in the source.
    readonly property var messages: [
        "Nothing here yet",
        "A very tidy folder",
        "Not a file in sight",
        "Quiet in here",
        "No clutter to report",
        "Waiting for something to land",
        "Empty, and that is fine"
    ]
    // The OEM phraseTimer's own interval, tailscale's and airpods' Panel.qml.
    readonly property int rotateMs: 2800
    // PanelHero's own secondary ink, the metadata role, not the row's muted one.
    readonly property color dim: Theme.color.metadata
    // A data-held folder skips this for one landing, so it draws whole like the picture hold did.
    property bool animateEntrance: true

    // The widest the hero draws, the longest rotating phrase included, so ui/PaneStates.qml places it once per state.
    readonly property real naturalWidth: Math.max(root.hint.length > 0 ? hintText.implicitWidth : 0,
        root.caption.length > 0 ? caption.implicitWidth : root.widestPhrase(phraseFace.font))

    FontMetrics { id: phraseFace; font: caption.font }
    // face is read only so a font change re-measures, as GridTile's caption marks do.
    function widestPhrase(face) {
        return root.messages.reduce(function (widest, phrase) {
            return Math.max(widest, phraseFace.advanceWidth(phrase.toUpperCase()))
        }, 0)
    }

    // Entrance only: the caller (shell.qml) cuts visible straight to false on the way out, so
    // this only ever animates the appear direction, never a disappearance.
    opacity: root.visible ? 1 : 0

    Behavior on opacity {
        id: entranceOpacity
        enabled: root.animateEntrance && root.visible && !Theme.reducedMotion
        NumberAnimation { duration: Motion.durMs.open; easing.type: Easing.OutCubic }
    }

    Column {
        id: content
        anchors.centerIn: parent
        anchors.verticalCenterOffset: root.visible ? 0 : Motion.translateUpPx
        spacing: Theme.spacing.gap

        Behavior on anchors.verticalCenterOffset {
            id: entranceOffset
            enabled: root.animateEntrance && root.visible && !Theme.reducedMotion
            NumberAnimation { duration: Motion.durMs.open; easing.type: Easing.OutCubic }
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
            color: Theme.color.metadataFrame
        }

        Text {
            id: caption
            anchors.horizontalCenter: parent.horizontalCenter
            text: (root.caption.length > 0 ? root.caption : root.messages[root.messageIndex]).toUpperCase()
            color: root.caption.length > 0 ? root.dim : Theme.color.foreground
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            font.bold: true
            font.letterSpacing: 1.2
            textFormat: Text.PlainText
        }

        Text {
            id: hintText
            visible: root.hint.length > 0
            anchors.horizontalCenter: parent.horizontalCenter
            text: root.hint
            color: root.dim
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            textFormat: Text.PlainText
        }
    }

    // Three beats per appearance, then rest on the third phrase with the mark drawn: an endless rotation kept a core 12% busy.
    readonly property int beatLimit: 3
    property int beats: 0
    // Rest is no beat left with the fade stopped and the mark drawn whole; reduced motion and a fixed caption keep no beat, so the fade and the opacity gate those.
    readonly property bool atRest: Theme.reducedMotion || (!fade.running && root.opacity === 1 && (root.caption.length > 0 || (root.beats >= root.beatLimit && heroMark.settled)))
    onVisibleChanged: root.beats = 0

    // root.visible mirrors the caller's listingState === "empty" gate, so the rotation timer stops once a real listing arrives.
    Timer {
        interval: root.rotateMs
        running: root.visible && root.caption.length === 0 && !Theme.reducedMotion && root.beats < root.beatLimit
        repeat: true
        // One beat for both, per the operator: the mark repaints as the sentence changes.
        onTriggered: { root.beats += 1; fade.restart(); heroMark.replay() }
    }

    // The OEM's phraseSwap order (tailscale's and airpods' Panel.qml): fade out, swap the index, fade in, on Flea's OutCubic and Motion durations.
    SequentialAnimation {
        id: fade
        PropertyAnimation { target: caption; property: "opacity"; to: 0.0; duration: Motion.durMs.close; easing.type: Easing.OutCubic }
        ScriptAction { script: root.messageIndex = (root.messageIndex + 1) % root.messages.length }
        PropertyAnimation { target: caption; property: "opacity"; to: 1.0; duration: Motion.durMs.open; easing.type: Easing.OutCubic }
    }
    Connections {
        target: Theme
        function onReducedMotionChanged() {
            if (Theme.reducedMotion) {
                // Disabling a Behavior leaves its current animation running until the next property write.
                entranceOpacity.enabled = Qt.binding(function() { return root.animateEntrance && root.visible && !Theme.reducedMotion })
                entranceOffset.enabled = Qt.binding(function() { return root.animateEntrance && root.visible && !Theme.reducedMotion })
                root.opacity = Qt.binding(function() { return root.visible ? 1 : 0 })
                content.anchors.verticalCenterOffset = Qt.binding(function() { return root.visible ? 0 : Motion.translateUpPx })
                fade.stop()
                caption.opacity = 1
            }
        }
    }
}
