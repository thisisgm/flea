import QtQuick
import qs.Commons
import "." as Flea
import "js/Archive.js" as Archive
import "js/ArchiveCard.js" as ArchiveCard

// The encrypted archive's card: its name, one masked field with the eye, and Extract; a wrong answer reopens it with its sentence and as many bullets as were typed.
Item {
    id: root

    property bool opened: false
    property var owner: null
    property var source: null
    property string leaf: ""
    property string errorText: ""
    property int focusPart: 0
    // The archive and destination a submit sent, and how many characters it held; the secret itself is cleared on submit.
    property string retryKey: ""
    property int retryLength: 0

    signal accepted(var source, string password)

    readonly property string password: passwordField.text
    readonly property bool canExtract: root.opened && root.password.length > 0
    // How many bullets stand for a refused answer; a count, never a character of it.
    readonly property int heldMarks: passwordField.heldMask
    readonly property var cancelItem: cancelButton
    readonly property var submitItem: extractButton
    readonly property var bodyItem: body
    readonly property var cardItem: card

    anchors.fill: parent
    visible: root.opened || root.opacity > 0
    // Closed means no input, even while the fade-out is still drawn.
    enabled: root.opened
    opacity: 0
    z: 2

    Flea.Reveal { target: root; shown: root.opened; lift: card }
    // The sentence and the title clear once the fade ended, so the card stays still while it fades out; open() sets both again.
    onOpacityChanged: if (root.opacity === 0 && !root.opened) { root.errorText = ""; root.leaf = "" }

    // The tab order, answered at the root because the field owns Tab as a signal; a disabled stop is skipped.
    function tabStops() {
        return [passwordField.input, cancelButton, extractButton].filter(function (s) { return s.enabled })
    }

    function stepTab(back) {
        var stops = root.tabStops()
        var at = -1
        for (var i = 0; i < stops.length; i++) {
            if (stops[i].activeFocus) { at = i; break }
        }
        var next = at < 0 ? (back ? stops.length - 1 : 0)
                          : (at + (back ? stops.length - 1 : 1)) % stops.length
        root.focusPart = stops[next] === cancelButton ? 1 : stops[next] === extractButton ? 2 : 0
        stops[next].forceActiveFocus()
    }

    Keys.onEscapePressed: function (event) { root.close(); event.accepted = true }
    Keys.onTabPressed: function (event) { root.stepTab((event.modifiers & Qt.ShiftModifier) !== 0); event.accepted = true }
    Keys.onBacktabPressed: function (event) { root.stepTab(true); event.accepted = true }

    // Test seam for the threat walk: types into the field the way keys would.
    function takeSecretForTest(secret) {
        passwordField.text = secret
    }

    // What a capture reads instead of the field: whether bullets are drawn, the error frame and the eye, never a typed character.
    function captureState() {
        return { filled: passwordField.text.length > 0 || passwordField.heldMask > 0, fieldError: passwordField.error,
                 revealed: passwordField.revealed, eye: passwordField.eyeCentre() }
    }

    function keyOf(path, dest) {
        return String(path) + "\n" + String(dest)
    }

    // The source is { path, dest } and err the backend's code; a first ask has no sentence, and a wrong password on this card's own submit draws as many bullets as were typed.
    function open(source, holder, err) {
        if (root.opened || !holder || !source) return
        var again = Archive.wrongPassword(err) && root.retryKey === root.keyOf(source.path, source.dest)
        passwordField.text = ""
        passwordField.heldMask = again ? root.retryLength : 0
        root.retryKey = ""
        root.retryLength = 0
        root.owner = holder
        root.source = source
        root.leaf = ArchiveCard.sourceLeaf(source)
        root.errorText = Archive.wrongPassword(err) ? ArchiveCard.wrongPasswordSentence(root.leaf) : ""
        root.focusPart = 0
        root.opened = true
        body.contentY = 0
        passwordField.takeFocus()
    }

    // The field clears on submit, cancel, esc and the ground, so a secret never outlives the card that took it.
    function close() {
        if (!root.opened)
            return
        passwordField.text = ""
        passwordField.heldMask = 0
        root.retryKey = ""
        root.retryLength = 0
        root.source = null
        root.opened = false
        if (root.owner && root.owner.listArea)
            root.owner.listArea.forceActiveFocus()
        root.owner = null
    }

    function commit() {
        if (!root.opened) return
        // Extract stays live, as the board draws it and a primary is never the dimmer button; an empty field asks for the password.
        if (!root.canExtract) {
            root.errorText = "Type a password first."
            root.focusPart = 0
            passwordField.takeFocus()
            return
        }
        // Accepted before close, so the owner's submit still sees it; close clears the field and only the count stays.
        var key = root.keyOf(root.source.path, root.source.dest)
        var typed = root.password.length
        root.accepted(root.source, root.password)
        root.close()
        root.retryKey = key
        root.retryLength = typed
    }

    // A dimmed ground, and a click on it is a cancel, the same shape the convert dialog uses.
    Flea.DialogScrim {
        onDismissed: root.close()
    }

    Rectangle {
        id: card
        x: Theme.cardOrigin(root.width, width)
        y: Theme.cardOrigin(root.height, height)
        width: Theme.cardSpan(Theme.card.form, root.width - 2 * Theme.card.inset)
        // Clamped to the window; the body scrolls whatever the clamp cut, see ui/CardScroll.qml.
        height: Theme.cardSpan(body.wanted + 2 * Theme.card.padding, root.height - 2 * Theme.card.inset)
        color: Theme.color.surface
        border.width: Theme.spacing.hairline
        border.color: Theme.color.mutedFrame
        radius: Style.cornerRadius

        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            hoverEnabled: true
            onClicked: {}
            onWheel: function (wheel) { wheel.accepted = true }
        }

        Flea.CardFrame {
            id: body
            bleedY: Theme.focusClearance
            anchors.fill: parent
            anchors.topMargin: Theme.card.padding - Theme.focusClearance
            anchors.bottomMargin: Theme.card.padding - Theme.focusClearance
            spacing: Theme.card.gap

            title: Flea.DialogHeader {
                id: header
                x: Theme.card.padding
                width: parent.width - 2 * Theme.card.padding
                text: ArchiveCard.encryptedTitle(root.leaf)
                elide: Text.ElideMiddle
            }

            buttons: Item {
                width: parent.width
                height: Math.max(cancelButton.implicitHeight, extractButton.implicitHeight)
                Row {
                    anchors.bottom: parent.bottom
                    anchors.right: parent.right
                    anchors.rightMargin: Theme.card.padding
                    spacing: Theme.spacing.gap

                    Flea.DialogButton {
                        id: cancelButton
                        label: "Cancel"
                        focused: root.focusPart === 1
                        available: true
                        enabled: true
                        onActivated: root.close()
                        onTabbed: function (from, back) { root.stepTab(back) }
                    }

                    Flea.DialogButton {
                        id: extractButton
                        label: "Extract"
                        primary: true
                        focused: root.focusPart === 2
                        onActivated: root.commit()
                        onTabbed: function (from, back) { root.stepTab(back) }
                    }
                }
            }

            // The wrong-password specimen: the field in the error frame, then its bare sentence directly under it.
            Column {
                x: Theme.card.padding
                width: parent.width - 2 * Theme.card.padding
                spacing: Theme.spacing.gap

                Flea.DialogField {
                    id: passwordField
                    width: parent.width
                    label: "Password"
                    secret: true
                    error: root.errorText.length > 0
                    onEdited: root.errorText = ""
                    onAccepted: root.commit()
                    onTabbed: function (from, back) { root.stepTab(back) }
                }

                Text {
                    width: parent.width
                    visible: root.errorText.length > 0
                    text: root.errorText
                    color: Theme.color.errorOnSurface
                    font { family: Theme.font.family; pixelSize: Theme.font.caption }
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }
            }
        }
    }
}
