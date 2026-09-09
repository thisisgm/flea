import QtQuick
import "js/Picker.js" as Picker

// The footer: how many rows the listing holds and what is checked on the left, the keys that act
// on it on the right. A message from the window takes the left slot in the accent while it lives,
// and a download in flight takes it for as long as it runs, with a slim bar along the top rule
// for how far along it is.
Item {
    id: root

    property var picker: null
    // ui/PickerFetch.qml, for the line and the bar while a typed URL downloads.
    property var fetch: null

    // The window's one-line notice. It lives four seconds, long enough to read and short enough
    // that a stale refusal never sits under a later action. A held one stays until the next say:
    // the share legs can take their whole deadline, and the line says so until they answer.
    property string message: ""

    function say(text, hold) {
        root.message = text
        life.stop()
        if (hold !== true)
            life.restart()
    }

    readonly property bool fetching: root.fetch !== null && root.fetch.fetching

    // The left slot's own line, what it says once no message and no download outranks it.
    readonly property string standing: Picker.footerLine(root.picker.listingState, root.picker.total,
        root.picker.marks.length, Picker.totalBytes(root.picker.marks))

    height: Theme.chromeHeight

    Timer {
        id: life
        interval: 4000
        onTriggered: root.message = ""
    }

    // The footer takes the chrome plane, the same strip the ask above it stands on.
    Rectangle {
        anchors.fill: parent
        color: Theme.color.surface
    }

    Rectangle {
        anchors.top: parent.top
        width: parent.width
        height: Theme.spacing.hairline
        color: root.picker.edge
    }

    // The download's bar, on the rule the footer already draws and no taller than a caption's
    // descender. With a total it fills; without one it runs, because an unknown total is not zero.
    Item {
        id: bar
        anchors.top: parent.top
        width: parent.width
        height: Math.max(2, Math.round(Theme.font.caption / 4))
        visible: root.fetching

        Rectangle {
            anchors.fill: parent
            color: Theme.color.muted
            opacity: 0.25
        }

        Rectangle {
            visible: root.fetching && root.fetch.total > 0
            width: parent.width * (root.fetching ? root.fetch.fraction : 0)
            height: parent.height
            color: Theme.color.accent
        }

        Rectangle {
            id: runner
            visible: root.fetching && root.fetch.total <= 0
            width: Math.round(parent.width / 5)
            height: parent.height
            color: Theme.color.accent
            // Reduced motion keeps the runner still at the start: the line beside it says it is working.
            SequentialAnimation on x {
                running: runner.visible && !Theme.reducedMotion
                loops: Animation.Infinite
                NumberAnimation { from: 0; to: bar.width - runner.width; duration: 900; easing.type: Easing.InOutQuad }
                NumberAnimation { to: 0; duration: 900; easing.type: Easing.InOutQuad }
            }
        }
    }

    Text {
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        text: root.message.length > 0 ? root.message
            : root.fetching ? root.fetch.line
            : root.standing
        color: root.message.length > 0 || root.fetching ? Theme.color.accent : Theme.color.foreground
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
    }

    Text {
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        text: Picker.hints(root.picker.req)
        color: Theme.color.foreground
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
    }
}
