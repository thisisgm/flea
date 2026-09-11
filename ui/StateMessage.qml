import QtQuick
import qs.Commons
import "js/Errors.js" as Errors

// The one thing a pane shows instead of rows. States.dc.html draws each of these as a mark over a
// line: Error is the alert in the error role over the sentence, Locked is the lock in the muted role
// over the directory's own mode string. The empty state has its own overlay in ui/shell.qml, so this
// yields to it there, and a state that is not a failure draws no mark at all.
Item {
    id: root

    property string message: ""
    property bool active: true
    property string listingState: "loading"
    property int total: 0
    // The st_mode of the directory the listing was denied. Zero whenever the backend could not stat
    // it either, which draws no line rather than a mode string that would be false.
    property int lockedMode: 0

    readonly property bool locked: root.listingState === "locked"
    readonly property bool failed: root.locked || root.listingState === "error"
    readonly property string line: Errors.paneLine(root.listingState, root.message, root.lockedMode)

    visible: root.active && root.total === 0 && root.line.length > 0 && root.listingState !== "empty"

    Column {
        anchors.centerIn: parent
        spacing: Theme.spacing.gap

        // corner: "lock" must reach ui/js/Icons.js before any pane can reach the locked state, because Icons.pathFor falls back to the file mark in silence.
        Glyph {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: root.failed
            // A failure mark stands alone rather than beside a row's text, so it opts in to the
            // pane-state ceiling; States.dc.html draws Locked and Error at 40 and the row mark is 19.
            maxSize: Theme.stateMarkSize
            width: Theme.stateMarkSize
            height: Theme.stateMarkSize
            name: root.locked ? "lock" : "alert"
            color: root.locked ? Theme.color.muted : Theme.color.error
        }

        Text {
            width: root.width
            text: root.line
            color: root.locked ? Theme.color.muted : Theme.color.foreground
            font.family: Theme.font.family
            font.pixelSize: Theme.font.body
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
        }
    }
}
