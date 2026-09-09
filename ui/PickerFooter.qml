import QtQuick
import "js/Ops.js" as Ops
import "js/PickerA11y.js" as A11y
import "js/Picker.js" as Picker
import "js/Transfer.js" as Transfer

// The footer: how many rows the listing holds and what is checked on the left, the keys that act
// on it on the right. A running operation takes the left slot until it ends, then its result uses
// the four-second message slot. A slim bar along the top rule shows fetch or transfer progress.
Item {
    id: root

    property var picker: null
    // ui/PickerFetch.qml, for the line and the bar while a typed URL downloads.
    property var fetch: null

    // The window's one-line notice. It lives four seconds, long enough to read and short enough
    // that a stale refusal never sits under a later action. A held one stays until the next say:
    // the share legs can take their whole deadline, and the line says so until they answer.
    property string message: ""
    property bool messageIsError: false
    // An operation's line does not expire while that operation still owns it.
    property string sticky: ""

    // The last spoken line is also the test seam. Progress has separate memory for each operation.
    property string announcement: ""
    property bool announcementIsError: false
    property var transferAnnouncement: A11y.emptyProgress()
    property var fetchAnnouncement: A11y.emptyProgress()

    function announceNow(update) {
        if (update.text.length === 0)
            return
        root.announcement = update.text
        root.announcementIsError = update.assertive
        notice.speak(update.text, update.assertive)
    }

    function say(text, hold, isError) {
        root.messageIsError = isError === true
        root.message = text
        root.announceNow(A11y.notice(text, root.messageIsError))
        life.stop()
        if (hold !== true)
            life.restart()
    }

    // A new transfer supersedes the old notice. An error that arrives during it still takes the
    // one visible line until its normal timer ends, then progress becomes visible again.
    onTransferringChanged: if (root.transferring) root.say("", false, false)

    readonly property bool fetching: root.fetch !== null && root.fetch.fetching
    readonly property real fetchFraction: root.fetching ? root.fetch.fraction : 0
    readonly property bool transferring: root.picker !== null && root.picker.transfer.running
    readonly property string transferLine: root.transferring
        ? Ops.progressLine(root.picker.transfer) : ""
    readonly property real transferFraction: root.transferring
        ? Transfer.fraction(root.picker.transfer) : 0
    readonly property string operationLine: root.transferLine.length > 0 ? root.transferLine : root.sticky
    readonly property bool showingError: root.message.length > 0 && root.messageIsError
    // The visible notice without the standing count. The test seam reads this so held drag lines
    // remain observable after sticky and timed messages become separate slots.
    readonly property string noticeLine: root.showingError ? root.message
        : root.transferLine.length > 0 ? root.transferLine
        : root.fetching ? root.fetch.line
        : root.sticky.length > 0 ? root.sticky
        : root.message

    // The left slot's own line, what it says once no message and no download outranks it.
    readonly property string standing: Picker.footerLine(root.picker.listingState, root.picker.total,
        root.picker.marks.length, Picker.totalBytes(root.picker.marks))

    function updateTransferAnnouncement() {
        var key = root.transferring ? "transfer:" + root.picker.transfer.id : ""
        root.transferAnnouncement = A11y.progress(root.transferAnnouncement, key,
                                                  root.transferLine, root.transferFraction)
        root.announceNow(A11y.notice(root.transferAnnouncement.text, false))
    }

    function updateFetchAnnouncement() {
        root.fetchAnnouncement = A11y.progress(root.fetchAnnouncement, root.fetching ? "fetch" : "",
                                               root.fetching ? root.fetch.line : "", root.fetchFraction)
        root.announceNow(A11y.notice(root.fetchAnnouncement.text, false))
    }

    onTransferLineChanged: root.updateTransferAnnouncement()
    onTransferFractionChanged: root.updateTransferAnnouncement()
    onFetchingChanged: {
        if (root.fetching)
            Qt.callLater(root.updateFetchAnnouncement)
        else
            root.fetchAnnouncement = A11y.emptyProgress()
    }
    onFetchFractionChanged: if (root.fetching) root.updateFetchAnnouncement()

    height: Theme.chromeHeight

    Timer {
        id: life
        interval: 4000
        onTriggered: { root.message = ""; root.messageIsError = false }
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

    // The work bar, on the rule the footer already draws and no taller than a caption's descender.
    // A transfer fills across all items. A fetch with no known total runs instead.
    Item {
        id: bar
        anchors.top: parent.top
        width: parent.width
        height: Math.max(2, Math.round(Theme.font.caption / 4))
        visible: root.transferring || root.fetching
        Accessible.role: Accessible.ProgressBar
        Accessible.name: root.transferring ? root.transferLine : root.fetching ? root.fetch.line : ""
        Accessible.description: root.transferring
            ? Math.round(root.transferFraction * 100) + "% complete"
            : root.fetching && root.fetch.total > 0
                ? Math.round(root.fetch.fraction * 100) + "% complete" : "Progress unknown"

        Rectangle {
            anchors.fill: parent
            color: Theme.color.muted
            opacity: 0.25
        }

        Rectangle {
            visible: root.transferring || (root.fetching && root.fetch.total > 0)
            width: parent.width * (root.transferring ? root.transferFraction
                : root.fetching ? root.fetch.fraction : 0)
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
        id: notice
        function speak(text, assertive) {
            Accessible.announce(text, assertive ? Accessible.Assertive : Accessible.Polite)
        }
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.right: hints.left
        anchors.rightMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        text: root.noticeLine.length > 0 ? root.noticeLine : root.standing
        color: root.showingError ? Theme.color.error
            : root.operationLine.length > 0 || root.message.length > 0 || root.fetching
                ? Theme.color.accent : Theme.color.foreground
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        elide: Text.ElideRight
        textFormat: Text.PlainText
        Accessible.role: Accessible.StaticText
        Accessible.name: text
    }

    Text {
        id: hints
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
