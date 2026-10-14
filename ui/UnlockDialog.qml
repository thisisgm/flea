import QtQuick
import Quickshell.Io
import qs.Commons
import "." as Flea
import "js/Buttons.js" as Buttons
import "js/Format.js" as Format
import "js/Unlock.js" as Unlock

// Unlock (U039): the encrypted-drive dialog the board draws; the passphrase goes to udisksctl unlock on stdin, never argv.
FocusScope {
    id: root
    anchors.fill: parent
    visible: opened || opacity > 0
    // Closed means no input, even while the fade-out is still drawn.
    enabled: opened
    opacity: 0
    property bool opened: false
    property string device: ""
    property string driveLabel: ""
    property double driveSize: 0
    property string crypto: "LUKS"
    // typing is the default, unlocking dims the field, wrong carries the error.
    property string phase: "typing"
    property string noteText: ""
    property bool noteIsError: false
    property string _secret: ""
    property string _unlockOutput: ""
    property string _unlockErr: ""
    property string _mountOutput: ""
    property bool _started: false
    // The child in flight: unlock, mount; relock is a failed mount's lock, abandon a Cancel the late exit still settles.
    property string _stage: "idle"
    property string _lockLabel: ""
    // A late lock from an abandoned attempt never refuses the dialog, reopened or not.
    property bool _quietLock: false
    // The drive a Cancelled attempt was unlocking, so its late lock closes that drive and names it, whatever the dialog opened since.
    property string _abandonDevice: ""
    property string _abandonLabel: ""
    // A submit made while an earlier attempt's client or its late lock still runs; it starts when they end.
    property bool _queued: false
    property int unlockTimeoutMs: 15000
    readonly property var cardItem: card
    // Read by the capture harness: the controls unlockState names, no look change.
    readonly property var fieldItem: field
    function buttonItem(name) { return ({cancel: cancelButton, unlock: unlockButton})[name] || null }
    signal unlocked(string path, string label, string note)
    signal closed()
    signal lockFailed(string note)

    function openFor(devicePath, label, size, fstype, fsver) {
        root.device = String(devicePath || "")
        root.describe(label, size, fstype, fsver)
        root.phase = "typing"
        root.noteText = ""
        root.noteIsError = false
        root._secret = ""
        field.text = ""
        field.heldMask = 0
        field.error = false
        root.opened = true
        field.takeFocus()
    }

    // The name row and every sentence name the drive through this, so a later rail row can name it for flea --unlock.
    function describe(label, size, fstype, fsver) {
        root.driveLabel = String(label || root.device)
        root.driveSize = Number(size) || 0
        root.crypto = Unlock.cryptoName(String(fstype || ""), String(fsver || ""))
    }

    // The tab order: the field, then Cancel, then Unlock; while Unlocking, Cancel is the one live control.
    function tabStops() {
        return root.phase === "unlocking" ? [cancelButton] : [field.input, cancelButton, unlockButton]
    }

    function stepTab(back) {
        var stops = root.tabStops()
        var at = -1
        for (var i = 0; i < stops.length; i++) {
            if (stops[i].activeFocus) { at = i; break }
        }
        var next = at < 0 ? (back ? stops.length - 1 : 0) : (at + (back ? stops.length - 1 : 1)) % stops.length
        stops[next].forceActiveFocus()
    }

    // Read by the capture harness and ui/UnlockHost.qml: which stop holds the keyboard, and whether the field holds typed or held keys.
    function focusPart() {
        return field.input.activeFocus ? "field" : cancelButton.activeFocus ? "cancel" : unlockButton.activeFocus ? "unlock" : ""
    }
    readonly property bool masked: field.text.length > 0 || field.heldMask > 0
    readonly property string subtitleText: Unlock.subtitle(root.driveLabel, root.driveSize > 0 ? Format.size(root.driveSize) : "", root.crypto)

    function close() {
        unlockTimeout.stop()
        root._queued = false
        if (root._stage === "unlock") {
            // A Cancelled unlock may finish on the daemon after its client is gone, so the client runs on and the late exit locks back.
            root._stage = "abandon"
            root._abandonDevice = root.device
            root._abandonLabel = root.driveLabel
        } else if (root._stage === "relock") {
            root._stage = "idle"
        } else if (root._stage === "mount") {
            // The late exit of the killed mount locks the container, so the dialog leaves it as it found it.
            root._stage = "abandon"
            root._abandonDevice = root.device
            root._abandonLabel = root.driveLabel
            if (mountProcess.running)
                mountProcess.running = false
            else
                root.lockContainer(false, root._abandonDevice, root._abandonLabel)
        }
        field.text = ""
        field.heldMask = 0
        root._secret = ""
        root.opened = false
        root.closed()
    }

    // Ends an attempt with a sentence the user can act on; no path leaves the passphrase held.
    function refuse(note) {
        unlockTimeout.stop()
        root._stage = "idle"
        root._secret = ""
        root.showRefusal(note)
    }

    function showRefusal(note) {
        root.phase = "wrong"
        field.error = true
        root.noteText = note
        root.noteIsError = true
        field.takeFocus()
    }

    // A dialog that ends without a mounted volume leaves the container locked; relock keeps the dialog open on the mount sentence.
    function lockContainer(relock, device, label) {
        root._stage = relock ? "relock" : "idle"
        root._lockLabel = label || root.driveLabel
        lockProcess.command = ["udisksctl", "lock", "--block-device", device || root.device]
        lockProcess.running = true
    }

    function submit() {
        if (!root.opened || root.phase === "unlocking")
            return
        if (field.text.length === 0) {
            // An empty field refuses without touching _stage, so a Cancelled attempt's late exit still locks back.
            root.showRefusal(Unlock.wrongNote(root.driveLabel))
            return
        }
        root._secret = field.text
        // Only the submitted length is kept, so the dots draw from a count.
        field.heldMask = field.text.length
        field.text = ""
        root.phase = "unlocking"
        cancelButton.forceActiveFocus()
        field.error = false
        root.noteText = Unlock.unlockingNote(root.driveLabel)
        root.noteIsError = false
        // An earlier attempt's client, its killed mount or its late lock still runs, so this one waits under the same bound.
        if (unlockProcess.running || lockProcess.running || root._stage === "abandon") {
            root._queued = true
            unlockTimeout.restart()
            return
        }
        root._startUnlock()
    }

    function _startQueued() {
        if (!root._queued)
            return
        root._queued = false
        unlockTimeout.stop()
        root._startUnlock()
    }

    function _startUnlock() {
        root._unlockOutput = ""
        root._unlockErr = ""
        root._started = false
        root._stage = "unlock"
        root._quietLock = false
        unlockProcess.command = Unlock.unlockArgv(root.device)
        // The last attempt closed stdin to send EOF, so a retry opens it again before starting.
        unlockProcess.stdinEnabled = true
        unlockProcess.running = true
        unlockTimeout.restart()
    }

    Flea.Reveal { target: root; shown: root.opened; lift: card }
    Keys.onTabPressed: function (event) { root.stepTab((event.modifiers & Qt.ShiftModifier) !== 0); event.accepted = true }
    Keys.onBacktabPressed: function (event) { root.stepTab(true); event.accepted = true }
    Keys.onPressed: function(event) {
        if (event.key === Qt.Key_Escape) {
            root.close()
            event.accepted = true
            return
        }
        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
            root.submit()
            event.accepted = true
        }
    }

    Timer {
        id: unlockTimeout
        interval: root.unlockTimeoutMs
        repeat: false
        // Only a child that never started is ended here: a slow KDF has no bound, and Cancel leaves a running one to its late exit.
        onTriggered: {
            // A queued submit never reached its client: it is refused and the passphrase dropped, and the earlier attempt still settles.
            if (root._queued) {
                root._queued = false
                root._secret = ""
                root.showRefusal(Unlock.unlockFailedNote(root.driveLabel))
                return
            }
            if (root._started || root._stage !== "unlock")
                return
            unlockProcess.running = false
            root.refuse(Unlock.unlockFailedNote(root.driveLabel))
        }
    }

    Flea.DialogScrim {
        onDismissed: root.close()
    }

    Rectangle {
        id: card
        x: Theme.cardOrigin(root.width, width)
        y: Theme.cardOrigin(root.height, height)
        width: Theme.cardSpan(Theme.card.form, root.width - 2 * Theme.card.inset)
        height: Theme.cardSpan(body.wanted + card.cardPadding + card.cardBottomPadding, root.height - 2 * Theme.card.inset)
        color: Theme.color.surface
        border.color: Theme.color.mutedFrame
        border.width: Theme.spacing.hairline
        radius: Style.cornerRadius
        // The shared card padding on every side; the gap over the buttons is CardFrame's.
        readonly property real cardPadding: Theme.card.padding
        readonly property real cardBottomPadding: Theme.card.padding
        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            // Without this sink a press on the card padding falls to the ground, which closes the dialog.
            onClicked: {}
            onWheel: function(wheel) { wheel.accepted = true }
        }
        Flea.CardFrame {
            id: body
            anchors.fill: parent
            anchors.leftMargin: card.cardPadding
            anchors.rightMargin: card.cardPadding
            anchors.topMargin: card.cardPadding
            anchors.bottomMargin: card.cardBottomPadding
            title: Flea.DialogHeader {
                width: parent.width
                text: Unlock.title(root.driveLabel)
            }
            Column {
                width: parent.width
                spacing: Theme.spacing.gap
                Row {
                    width: parent.width
                    spacing: Theme.spacing.gap
                    Flea.Glyph {
                        width: Theme.markSize
                        height: Theme.fileRowHeight
                        name: "drive"
                        color: Theme.color.foreground
                    }
                    Text {
                        width: parent.width - Theme.markSize - parent.spacing
                        anchors.verticalCenter: parent.verticalCenter
                        text: root.subtitleText
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        color: Theme.color.foreground
                        font { family: Theme.font.family; pixelSize: Theme.font.body }
                    }
                }
                Flea.DialogField {
                    id: field
                    width: parent.width
                    label: "Passphrase"
                    secret: true
                    enabled: root.phase !== "unlocking"
                    opacity: root.phase === "unlocking" ? Buttons.DISABLED_OPACITY : 1
                    onAccepted: root.submit()
                    onTabbed: function (from, back) { root.stepTab(back) }
                    // A retry starts clean, as the archive card does.
                    onEdited: {
                        if (field.error || root.noteIsError) {
                            field.error = false
                            root.noteText = ""
                            root.noteIsError = false
                        }
                    }
                }
                Text {
                    width: parent.width
                    visible: root.noteText.length > 0
                    text: root.noteText
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: root.noteIsError ? Theme.color.errorOnSurface : Theme.color.muted
                    font { family: Theme.font.family; pixelSize: Theme.font.caption }
                }
            }
            buttons: Item {
                width: parent.width
                height: buttonRow.height
                Row {
                    id: buttonRow
                    anchors.right: parent.right
                    spacing: Theme.spacing.gap
                    Flea.DialogButton {
                        id: cancelButton
                        label: "Cancel"
                        focused: activeFocus
                        onActivated: root.close()
                    }
                    Flea.DialogButton {
                        id: unlockButton
                        label: "Unlock"
                        primary: true
                        // Disabled with the field, so a keyboard that Tabbed here drops to the card and Cancel keeps the one ring.
                        enabled: root.phase !== "unlocking"
                        opacity: root.phase === "unlocking" ? Buttons.DISABLED_OPACITY : 1
                        onActivated: root.submit()
                    }
                }
            }
        }
    }

    Process {
        id: unlockProcess
        environment: ({ "LC_ALL": "C" })
        stdinEnabled: true
        stdout: StdioCollector { id: unlockOut; waitForEnd: true; onStreamFinished: root._unlockOutput = text }
        stderr: StdioCollector { id: unlockErr; waitForEnd: true; onStreamFinished: root._unlockErr = text }
        onStarted: {
            root._started = true
            if (root._stage !== "unlock") {
                // A Cancel won the race: close stdin so the waiting --key-file /dev/stdin reads EOF and exits.
                unlockProcess.stdinEnabled = false
                return
            }
            var secret = root._secret
            root._secret = ""
            // --key-file /dev/stdin reads every byte to EOF as the key, so no newline and stdin closes after it.
            unlockProcess.write(secret)
            unlockProcess.stdinEnabled = false
            secret = ""
        }
        onExited: function(exitCode) {
            unlockTimeout.stop()
            // A Cancelled unlock that still succeeded left the container open with nobody waiting: lock it back quietly.
            if (root._stage === "abandon") {
                if (exitCode === 0) {
                    var late = Unlock.parseUnlocked(String(unlockOut.text || root._unlockOutput || ""))
                    if (late.length > 0) {
                        root._quietLock = true
                        root.lockContainer(true, root._abandonDevice, root._abandonLabel)
                        return
                    }
                }
                root._stage = "idle"
                root._startQueued()
                return
            }
            // Nothing waits on this exit, so a late one does nothing.
            if (root._stage !== "unlock")
                return
            if (exitCode !== 0) {
                // Wrong passphrase versus any other refusal is read off the D-Bus error name, never off udisksctl's own sentences.
                root.refuse(Unlock.refusalNote(String(unlockErr.text || root._unlockErr || ""), root.driveLabel))
                return
            }
            var mapped = Unlock.parseUnlocked(String(unlockOut.text || root._unlockOutput || ""))
            if (mapped.length === 0) {
                root.refuse(Unlock.wrongNote(root.driveLabel))
                return
            }
            root._mountOutput = ""
            root._stage = "mount"
            mountProcess.command = ["udisksctl", "mount", "--block-device", mapped]
            mountProcess.running = true
        }
    }

    Process {
        id: mountProcess
        environment: ({ "LC_ALL": "C" })
        stdout: StdioCollector { id: mountOut; waitForEnd: true; onStreamFinished: root._mountOutput = text }
        stderr: StdioCollector { waitForEnd: true }
        onExited: function(exitCode) {
            if (root._stage === "abandon") {
                root.lockContainer(false, root._abandonDevice, root._abandonLabel)
                return
            }
            if (root._stage !== "mount")
                return
            var folder = exitCode === 0 ? Unlock.parseMounted(String(mountOut.text || root._mountOutput || "")) : ""
            if (folder.length === 0) {
                root.lockContainer(true)
                return
            }
            var label = root.driveLabel
            var note = Unlock.unlockedNote(label)
            root._stage = "idle"
            field.text = ""
            field.heldMask = 0
            root.opened = false
            root.closed()
            root.unlocked(folder, label, note)
        }
    }

    Process {
        id: lockProcess
        environment: ({ "LC_ALL": "C" })
        // Every lock exit frees the drive for a submit that queued behind it.
        onExited: function(exitCode) {
            root._lockExited(exitCode)
            root._startQueued()
        }
    }

    function _lockExited(exitCode) {
        var quiet = root._quietLock
        root._quietLock = false
        if (root._stage === "relock") {
            // Open on a relock, the mount failure is what the user reads; a quiet lock or one nobody waits on takes no focus.
            if (root.opened && !quiet)
                root.refuse(Unlock.mountNote(root._lockLabel))
            else
                root._stage = "idle"
        }
        // Every refused lock is told, beside the mount failure or alone.
        if (exitCode !== 0)
            root.lockFailed(Unlock.lockNote(root._lockLabel))
    }
}
