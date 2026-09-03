import QtQuick
import qs.Commons
import "." as Flea
import "js/Format.js" as Format

// The transport the canvas draws under a media frame: a play/pause mark, a hairline seek with a
// square handle (the accent's one appearance here), and a tabular clock. No filled chrome anywhere.
Item {
    id: root

    property bool playing: false
    property real position: 0
    property real duration: 0

    // A tile borders four sides, which is the PreviewColumn artboard. Flush against the bottom of
    // the Quick Look overlay that same border reads as a box, so there it is one top hairline.
    property bool framed: true

    signal toggled()
    signal seeked(real ms)
    // Any contact at all, so a host that hides this over a video can re-arm its own timer.
    signal touched()

    readonly property real fraction: root.duration > 0
        ? Math.max(0, Math.min(1, root.position / root.duration)) : 0
    // A square handle, sized off the caption token so it scales with the text like everything else.
    readonly property int handle: Math.round(Theme.font.caption * 0.5)
    // One second a click, the step ui/Preview.qml set on the qs.Ui slider this replaced.
    readonly property int wheelStepMs: 1000

    // A drag in flight draws from the pointer, not from the player, whose own advance would fight it.
    property bool scrubbing: false
    property real scrubFraction: 0
    readonly property real shown: root.scrubbing ? root.scrubFraction : root.fraction

    implicitHeight: Theme.chromeHeight

    Rectangle {
        anchors.fill: parent
        color: Theme.color.surface
        border.width: root.framed ? Theme.spacing.hairline : 0
        border.color: Theme.color.muted
    }

    // The unframed form's own divider, the same hairline ChromeBar draws under itself.
    Rectangle {
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        visible: !root.framed
        height: Theme.spacing.hairline
        color: Theme.color.foreground
        opacity: 0.12
    }

    // A test clicks the mark itself rather than guessing at an offset inside the strip; seekItem is
    // the same seam for the track, which ui/shell.qml's previewSliderCentre reads through.
    readonly property var playItem: playSlot
    readonly property var seekItem: track

    Item {
        id: playSlot
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        width: Math.max(Theme.hitMin, Theme.font.bodySmall)
        height: Math.max(Theme.hitMin, Theme.font.bodySmall)

        Accessible.role: Accessible.Button
        Accessible.name: root.playing ? "Pause" : "Play"
        Accessible.onPressAction: { root.touched(); root.toggled() }

        Flea.Glyph {
            anchors.centerIn: parent
            width: Theme.font.bodySmall
            height: Theme.font.bodySmall
            name: root.playing ? "pause" : "play"
            color: Theme.color.foreground
        }

        TapHandler {
            acceptedButtons: Qt.LeftButton
            onTapped: {
                root.touched()
                root.toggled()
            }
        }
    }

    Item {
        id: track
        anchors.left: playSlot.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.right: clock.left
        anchors.rightMargin: Theme.spacing.gap
        anchors.top: parent.top
        anchors.bottom: parent.bottom

        function fractionAt(x) {
            return Math.max(0, Math.min(1, x / Math.max(1, track.width)))
        }

        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width
            height: Theme.spacing.hairline * 2
            color: Theme.color.muted
        }

        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width * root.shown
            height: Theme.spacing.hairline * 2
            color: Theme.color.accent
        }

        Rectangle {
            x: Math.round(parent.width * root.shown) - root.handle / 2
            anchors.verticalCenter: parent.verticalCenter
            width: root.handle
            height: root.handle
            color: Theme.color.accent
        }

        // Seeks once, on release: seeking on every dragged frame would fight the player's own
        // advance. A press and release without moving is a click, so this covers both.
        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onPressed: function (mouse) {
                root.scrubbing = true
                root.scrubFraction = track.fractionAt(mouse.x)
                root.touched()
            }
            onPositionChanged: function (mouse) {
                if (root.scrubbing)
                    root.scrubFraction = track.fractionAt(mouse.x)
                root.touched()
            }
            onReleased: {
                root.scrubbing = false
                root.touched()
                if (root.duration > 0)
                    root.seeked(root.duration * root.scrubFraction)
            }
            // A wheel is not a position change, so this is the one interaction that has to re-arm a
            // host's hide timer itself; the host clamps the absolute position this produces.
            onWheel: function (wheel) {
                root.touched()
                if (root.duration > 0)
                    root.seeked(root.position + (wheel.angleDelta.y > 0 ? root.wheelStepMs : -root.wheelStepMs))
            }
        }
    }

    Text {
        id: clock
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        text: Format.duration(root.position) + " / " + Format.duration(root.duration)
        color: Theme.color.muted
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
    }
}
