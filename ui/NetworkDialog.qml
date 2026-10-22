import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "." as Flea
import "js/Mounts.js" as Mounts
import "js/Protocols.js" as Protocols

// A new favourite is saved only after its identified mount succeeds; credentials stay in this session.
Item {
    id: root

    property bool opened: false
    property string statusText: ""
    property bool dropboxInstalled: false
    property bool retrying: false
    property bool failedConnect: false
    property int requestSerial: 0
    property string requestId: ""
    property string mountedUri: ""
    property string pendingUri: ""
    property string pendingLabel: ""
    property bool connecting: false
    property bool saving: false
    property bool saveCommitted: false
    property bool saveNewPlace: true
    readonly property bool busy: root.connecting || root.saving
    readonly property string dialogTitle: root.failedConnect ? "Could not connect to the network share" : root.baseTitle()
    // Downstream (driveway-fm) test seam: the Browse button's centre, so the mouse lane can press it.
    readonly property alias browseButtonCentre: browseButton
    // The card keeps this much window above and below it when the window is shorter than the card.
    readonly property int clampMargin: 8
    // var, not Item: BorderSurface is a qs.Ui type qmllint cannot resolve, and Item would read as incompatible.
    readonly property var cardItem: card
    readonly property var bodyItem: body

    signal closed()
    signal mountRequested(string requestId, string uri, string label, string password)
    signal cancelRequested(string requestId)

    // The hosts leg of the network browser: avahi's answer replaces the form with the share
    // browser's host rows, so a user who does not know an address can still find one.
    signal browseRequested()

    // A plain overlay, not a QQC Popup, the same call ui/ContextMenu.qml already made.
    anchors.fill: parent
    // opened flips at once so no caller races the close fade; visible holds until the fade ends, and a closed dialog takes no input.
    visible: root.opened || root.opacity > 0
    enabled: root.opened
    opacity: 0

    function open() {
        if (root.opened || root.busy) return
        root.unacknowledgedUri = ""
        root.unacknowledgedReason = ""
        root.statusText = ""
        root.retrying = false
        root.failedConnect = false
        root.mountedUri = ""
        root.saveNewPlace = true
        root.saveCommitted = false
        form.reset()
        root.present()
    }

    // The reason a mount actually failed with, kept across a close so reopening the same location
    // cannot answer for it. 0.1.6 never completed a missing-credential request, so its dialog kept
    // the real cause; here the row is reopened for the same uri a second later and the generic
    // "Enter the password" prompt was landing on top of "authentication helper is unavailable".
    property string unacknowledgedUri: ""
    property string unacknowledgedReason: ""

    function openLocation(uri, label, password, reason, failed) {
        if (root.opened || root.busy) return
        form.load(root.valuesFor(uri, label, password))
        var known = root.unacknowledgedUri === Mounts.normalize(uri) && root.unacknowledgedReason.length > 0
        root.statusText = known && failed !== true ? root.unacknowledgedReason : (reason || "")
        root.retrying = true
        root.failedConnect = known && failed !== true ? true : failed === true
        // Showing it is what acknowledges it. Left standing it answered the next genuine credential
        // prompt for the same location with a connect failure from minutes earlier.
        if (known) { root.unacknowledgedUri = ""; root.unacknowledgedReason = "" }
        root.mountedUri = ""
        root.saveNewPlace = false
        root.saveCommitted = false
        root.present()
    }

    // The card is kept between opens, so a body scrolled last time would open scrolled.
    function present() {
        body.contentY = 0
        root.checkDropbox()
        root.opened = true
        // The host is the one field the form actually needs, so it takes the caret on open.
        form.focusHost()
    }

    // The form's fields hold the caret whenever the dialog is open, and a key travels from the
    // focused item up that item's own parents, so neither of the handlers below can see Escape
    // from a field: pendingFocus is a sibling of the card, and the card's focus ring wraps the
    // fields without being their parent. The dialog root is every field's ancestor, so Escape
    // lands here from wherever the caret is; close() itself still refuses mid-save.
    Keys.onEscapePressed: root.close()

    function close() {
        if (root.saving) return
        if (root.connecting) root.cancelRequested(root.requestId)
        root.connecting = false
        root.requestId = ""
        root.mountedUri = ""
        form.takePassword()
        root.opened = false
        root.closed()
    }

    // The URI the form built, which is the same one the Mounts-as line showed.
    function submitLocation() {
        if (!root.opened || root.busy) return
        if (root.saveCommitted) { root.close(); return }
        if (!form.complete) {
            root.statusText = "Enter a valid host and port."
            return
        }
        if (form.spec.credentials && form.user.trim().length > 0 && form.password.length === 0) {
            root.statusText = "Enter the password to mount this location."
            root.retrying = true
            root.failedConnect = false
            return
        }
        root.pendingUri = Mounts.normalize(form.uri)
        root.pendingLabel = form.labelText()
        root.unacknowledgedUri = ""
        root.unacknowledgedReason = ""
        root.statusText = ""
        root.requestId = "network-" + (++root.requestSerial)
        if (root.mountedUri === root.pendingUri) { root.saveFavourite(); return }
        pendingFocus.forceActiveFocus()
        root.connecting = true
        root.mountRequested(root.requestId, root.pendingUri, root.pendingLabel, form.password)
    }

    function mountFinished(requestId, uri, success, reason, connectFailed) {
        if (!root.opened || !root.connecting || requestId !== root.requestId || uri !== root.pendingUri) return
        root.connecting = false
        if (!success) { root.saveFailed(reason, connectFailed === true); return }
        // The location answered, so nothing about it is left unacknowledged.
        root.unacknowledgedUri = ""
        root.unacknowledgedReason = ""
        root.mountedUri = uri
        root.saveFavourite()
    }

    function saveFavourite() {
        if (!root.saveNewPlace) { root.close(); return }
        pendingFocus.forceActiveFocus()
        root.saving = true
        if (!Favourites.add(root.pendingUri, root.pendingLabel, root.requestId))
            root.saveFailed("A Favorites change is still being saved. Retry to save this location.", false)
    }

    // connect is true when the connection itself failed, which the mount service says and this dialog never works out from the sentence.
    function saveFailed(message, connect) {
        root.saving = false
        root.statusText = message
        if (connect) {
            root.unacknowledgedUri = Mounts.normalize(root.pendingUri)
            root.unacknowledgedReason = message
        }
        root.retrying = true
        root.failedConnect = connect
        form.focusHost()
    }

    Connections {
        target: Favourites
        function onCompleted(requestId, success, message) {
            if (!root.opened || !root.saving || requestId !== root.requestId) return
            if (!success) { root.saveFailed(message, false); return }
            root.saving = false
            if (message) {
                root.saveCommitted = true
                root.statusText = message
                pendingFocus.forceActiveFocus()
                return
            }
            root.close()
        }
    }

    Item {
        id: pendingFocus
        Keys.onTabPressed: function(event) { event.accepted = true }
        Keys.onBacktabPressed: function(event) { event.accepted = true }
        Keys.onEscapePressed: root.close()
        Keys.onReturnPressed: if (root.saveCommitted) root.close()
        Keys.onEnterPressed: if (root.saveCommitted) root.close()
    }

    // Read back by shell.qml's IPC so a test asserts the protocol swap without OCR.
    function formProtocol() { return form.protocol }
    function formPort() { return form.port }
    function formUri() { return form.uri }
    function formPathLabel() { return form.spec.pathLabel }
    function formChip(name) { return form.chipFor(name) }
    function formFields() { return form.visibleFields() }
    function formFocus() { return form.focusName() }
    function formHostPortWidths() { return form.hostPortWidths() }
    function formPasswordState() { return form.passwordState() }
    function formPasswordEyeCentre() { return form.passwordEyeCentre() }
    function formPasswordHint() { return form.passwordHintText }
    function formNote() { return form.protocol === "NFS" ? "No password needed: the NFS server trusts this computer" : "" }
    function formAction() {
        if (root.saveCommitted) return "Close"
        if (root.saving) return "Saving…"
        if (root.connecting) return "Connecting…"
        return root.retrying ? "Retry" : "Connect and save"
    }
    function formMetrics() { return Math.round(card.padding) + "|" + Math.round(body.spacing) }
    // "contentY|contentHeight|height" of the scroller between the pinned title and buttons, so a test sees the clamp and the scroll.
    function bodyScroll() { return Math.round(body.contentY) + "|" + Math.round(body.contentHeight) + "|" + Math.round(body.scrollItem.height) }
    function formMetricTargets() { return Style.space(16) + "|" + Style.space(12) }

    // Dialogs rule 6: the title says what you are doing, so it survives a chip swap; the two entry
    // points are two tasks, open() adding a place and openLocation() connecting to one already saved.
    function baseTitle() {
        return root.retrying ? "Connect to a network share" : "Add a network share"
    }

    function decoded(text) {
        try { return decodeURIComponent(text) } catch (e) { return text }
    }

    // Sample input: ftps://user@host:2121/path, decomposed only to repopulate the approved form.
    function valuesFor(uri, label, password) {
        var match = String(uri || "").match(/^([a-z][a-z0-9+.-]*):\/\/(.*)$/i)
        if (!match) return { protocol: "SMB", label: label, password: password }
        var schemes = { smb: "SMB", sftp: "SFTP", ftp: "FTPS", ftps: "FTPS",
                        dav: "WebDAV", davs: "WebDAV", nfs: "NFS" }
        var scheme = match[1].toLowerCase()
        var protocol = schemes[scheme] || "SMB"
        var tls = scheme !== "ftp" && scheme !== "dav"
        var rest = match[2]
        var slash = rest.indexOf("/")
        var authority = slash < 0 ? rest : rest.substring(0, slash)
        var path = slash < 0 ? "" : root.decoded(rest.substring(slash + 1))
        var at = authority.lastIndexOf("@")
        var userInfo = at < 0 ? "" : authority.substring(0, at)
        var hostPort = at < 0 ? authority : authority.substring(at + 1)
        var host = hostPort
        var port = String(Protocols.defaultPort(protocol, tls))
        if (hostPort.charAt(0) === "[") {
            var bracket = hostPort.indexOf("]")
            if (bracket >= 0 && hostPort.charAt(bracket + 1) === ":") {
                host = hostPort.substring(0, bracket + 1)
                port = hostPort.substring(bracket + 2)
            }
        } else if (hostPort.lastIndexOf(":") >= 0) {
            var colon = hostPort.lastIndexOf(":")
            host = hostPort.substring(0, colon)
            port = hostPort.substring(colon + 1)
        }
        var domain = ""
        var user = root.decoded(userInfo)
        if (protocol === "SMB" && userInfo.indexOf(";") >= 0) {
            var semi = userInfo.indexOf(";")
            domain = root.decoded(userInfo.substring(0, semi))
            user = root.decoded(userInfo.substring(semi + 1))
        }
        return { protocol: protocol, label: label || "", host: host, port: port, path: path,
                 domain: domain, user: user, password: password || "",
                 tls: tls }
    }

    function checkDropbox() {
        if (dropboxCheck.running) return
        dropboxCheck.command = ["which", "dropbox-cli"]
        dropboxCheck.running = true
    }

    // omarchy-launch-terminal runs its argv directly in a real terminal, so the sudo prompt
    // omarchy-pkg-add may need and the install's own progress are both visible to the user.
    function installDropbox() {
        Quickshell.execDetached(["omarchy-launch-terminal", "omarchy", "install", "service", "dropbox"])
        root.close()
    }


    Process {
        id: dropboxCheck
        onExited: function (exitCode) { root.dropboxInstalled = exitCode === 0 }
    }

    // The fade and the rise are the shared reveal of ui/Reveal.qml.
    Flea.Reveal { target: root; shown: root.opened; lift: root.cardItem }

    Rectangle {
        id: scrim
        anchors.fill: parent
        // Util.alpha bakes the 0.7 scrim strength into the colour, the house idiom ConfirmDialog uses.
        color: Util.alpha(Theme.color.background, 0.7)

        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            hoverEnabled: true
            onClicked: root.close()
            onWheel: function (wheel) { wheel.accepted = true }
        }
    }

    // The house dialog shape read off ConfirmDialog.qml: BorderSurface, an accent border, cornerRadius.
    BorderSurface {
        id: card
        width: Theme.cardSpan(Theme.space(380) * Theme.dialogWidthRatio, root.width - 2 * root.clampMargin)
        // Clamped to the window; the body scrolls whatever the clamp cut, see ui/CardScroll.qml.
        height: Theme.cardSpan(body.wanted + contentTopInset + contentBottomInset, root.height - 2 * root.clampMargin)
        x: Theme.cardOrigin(root.width, width)
        y: Theme.cardOrigin(root.height, height)
        color: Theme.color.surface
        borderSpec: Border.flat(Theme.color.accent, Style.normalBorderWidth)
        radius: Style.cornerRadius
        padding: Style.space(16)

        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onClicked: {}
            onWheel: function (wheel) { wheel.accepted = true }
        }

        Flea.CardFrame {
            id: body
            anchors.fill: parent
            anchors.topMargin: card.contentTopInset - Theme.focusClearance
            anchors.bottomMargin: card.contentBottomInset - Theme.focusClearance
            bleed: Theme.focusClearance
            anchors.leftMargin: card.contentLeftInset - Theme.focusClearance
            anchors.rightMargin: card.contentRightInset - Theme.focusClearance
            Keys.onEscapePressed: root.close()
            // Outer rhythm and each section's own header-to-content gap read exact off tailscale/dropbox Panel.qml.
            spacing: Style.space(12)

            title: Flea.DialogTitle {
                width: parent.width
                text: root.dialogTitle
            }

            // Network.html places Cancel before the primary action at the card's right edge; the Dropbox install route stays pinned beneath.
            buttons: Column {
                width: parent.width
                spacing: Style.space(12)

                Row {
                    anchors.right: parent.right
                    spacing: Theme.spacing.gap

                    Flea.DialogButton {
                        id: browseButton
                        // An alternative entry, not an answer: it steps out of the form into
                        // discovery, the way Cancel steps out of the dialog entirely.
                        label: "Browse the network"
                        available: !root.busy
                        tabHandle: true
                        onTabbed: function (from, back) { form.step(from, back ? -1 : 1) }
                        onActivated: {
                            root.browseRequested()
                            root.close()
                        }
                    }

                    Flea.DialogButton {
                        id: cancelButton
                        label: "Cancel"
                        available: !root.saving
                        tabHandle: true
                        onTabbed: function (from, back) { form.step(from, back ? -1 : 1) }
                        onActivated: root.close()
                    }

                    Flea.DialogButton {
                        id: actionButton
                        label: root.formAction()
                        primary: true
                        available: !root.busy
                        tabHandle: true
                        onTabbed: function (from, back) { form.step(from, back ? -1 : 1) }
                        onActivated: root.submitLocation()
                    }
                }

                PanelSeparator { visible: !root.dropboxInstalled }

                // Dialogs rule 4: the section is a live install route while Dropbox is absent, and it is
                // held here until providers have a home of their own; installed, it has nothing to say
                // and is not drawn, where it used to draw a button that could never be pressed.
                Column {
                    width: parent.width
                    visible: !root.dropboxInstalled
                    spacing: Style.space(10)

                    PanelSectionHeader {
                        text: "DROPBOX"
                    }

                    Flea.DialogButton {
                        id: dropboxButton
                        label: "Install Dropbox"
                        available: !root.busy
                        tabHandle: true
                        onTabbed: function (from, back) { form.step(from, back ? -1 : 1) }
                        onActivated: root.installDropbox()
                    }
                }
            }

            Column {
                width: parent.width
                spacing: Style.space(12)

                NetworkForm {
                    id: form
                    width: parent.width
                    enabled: !root.busy && !root.saveCommitted
                    trailing: [cancelButton, actionButton, dropboxButton]
                    onSubmitted: root.submitLocation()

                    function labelText() {
                        return Protocols.label({ label: form.label, host: form.host, path: form.path })
                    }
                }

                // The slot is always there, two caption lines tall, so an error appearing never moves the buttons.
                Row {
                    width: parent.width
                    height: Theme.rowHeight
                    spacing: Theme.spacing.gap

                    Flea.Glyph {
                        visible: root.statusText.length > 0
                        width: Theme.font.caption
                        height: Theme.font.caption
                        name: "alert"
                        color: Theme.color.errorOnSurface
                    }

                    Text {
                        width: parent.width - Theme.font.caption - parent.spacing
                        text: root.statusText
                        color: Theme.color.errorOnSurface
                        font.family: Theme.font.family
                        font.pixelSize: Theme.font.caption
                        wrapMode: Text.Wrap
                        maximumLineCount: 2
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                    }
                }
            }
        }
    }
}
