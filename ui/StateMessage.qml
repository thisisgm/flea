import QtQuick
import qs.Commons
import "js/Errors.js" as Errors
import "js/Filter.js" as Filter
import "js/Format.js" as Format

// The one thing a pane shows instead of rows, and it carries all three parts the States board gives
// a pane state: the mark, then one sentence, then the detail the system can prove. The sentence used
// to live in the status bar about 1300px away while the mode string sat here alone. The empty state
// has its own overlay in ui/shell.qml, so this yields to it there.
Item {
    id: root

    property string message: ""
    property bool active: true
    property string listingState: "loading"
    property int total: 0
    // The filter narrows rows already listed, so a query that matches none leaves a ready listing
    // with nothing drawn in it; shown is -1 while no filter is up. loaded is how many of total the
    // filter has tested, -1 where the owner holds every row, as the columns do: the tile waits for
    // the whole folder, since no match among 321 held rows is not no match among 5,000 (issue 315).
    property string filterQuery: ""
    property int shown: -1
    property int loaded: -1
    // The st_mode of the directory the listing was denied. Zero whenever the backend could not stat
    // it either, which draws no line rather than a mode string that would be false.
    property int lockedMode: 0

    readonly property bool locked: root.listingState === "locked"
    // A listing past its wait is the Error state too, so the sentence draws with the alert mark.
    readonly property bool failed: root.locked || root.listingState === "error" || root.listingState === "waiting"
    readonly property bool nothingMatched: root.filterQuery.length > 0 && root.shown === 0 && root.total > 0
        && (root.loaded < 0 || root.loaded >= root.total)
    // The detail is the directory's own mode, which only a locked state has and only when the
    // backend could stat it; everything else proves nothing extra and draws nothing.
    readonly property string detail: root.locked ? Errors.lockedLine(root.lockedMode)
        : root.nothingMatched ? Filter.noMatch(root.total) : ""

    // The mark and lines at their unwrapped width, so ui/PaneStates.qml can tell whether they clear the listed columns.
    readonly property real naturalWidth: Math.max(Theme.stateMarkSize, sentence.implicitWidth,
        root.detail.length > 0 ? detailLine.implicitWidth : 0)

    visible: root.active && (root.nothingMatched
        || (root.total === 0 && root.message.length > 0 && root.listingState !== "empty"))

    Column {
        anchors.centerIn: parent
        spacing: Theme.spacing.gap

        // corner: "lock" must reach ui/js/Icons.js before any pane can reach the locked state, because Icons.pathFor falls back to the file mark in silence.
        Glyph {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: root.failed || root.nothingMatched
            // A failure mark stands alone rather than beside a row's text, so it opts in to the
            // pane-state ceiling; States.dc.html draws Locked and Error at 40 and the row mark is 19.
            maxSize: Theme.stateMarkSize
            width: Theme.stateMarkSize
            height: Theme.stateMarkSize
            name: root.locked ? "lock" : root.nothingMatched ? "search" : "alert"
            // Only a failure takes the urgent role, and only on the mark; the sentence stays readable.
            color: root.nothingMatched ? Theme.color.mutedFrame : Theme.color.error
        }

        Text {
            id: sentence
            width: root.width
            text: root.nothingMatched ? Format.fill(Filter.WORDS.nothingMatched, root.filterQuery) : root.message
            color: Theme.color.foreground
            font.family: Theme.font.family
            font.pixelSize: Theme.font.body
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
        }

        Text {
            id: detailLine
            width: root.width
            visible: root.detail.length > 0
            text: root.detail
            color: Theme.color.muted
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            horizontalAlignment: Text.AlignHCenter
            textFormat: Text.PlainText
        }
    }
}
