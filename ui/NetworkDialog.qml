import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "." as Flea
import "js/Mounts.js" as Mounts
import "js/Protocols.js" as Protocols
import "js/Motion.js" as Motion

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
    readonly property string dialogTitle: root.baseTitle() + (root.failedConnect ? ", failed connect" : "")
    // The card keeps this much window above and below it when the window is shorter than the card.
    readonly property int clampMargin: 8
    // var, not Item: BorderSurface is a qs.Ui type qmllint cannot resolve, and Item would read as incompatible.
    readonly property var cardItem: card
    readonly property var bodyItem: body

    signal closed()
    signal mountRequested(string requestId, string uri, string label, string password)
    signal cancelRequested(string requestId)

    // A plain overlay, not a QQC Popup, the same call ui/ContextMenu.qml already made.
    anchors.fill: parent
    // opened flips instantly (open()/close() above), so a caller reading it never races the close
    // fade; visible only stays true a little longer, until card's own opacity finishes it.
    visible: root.opened || card.opacity > 0

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

    function mountFinished(requestId, uri, success, reason) {
        if (!root.opened || !root.connecting || requestId !== root.requestId || uri !== root.pendingUri) return
        root.connecting = false
        if (!success) { root.saveFailed(reason); return }
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
            root.saveFailed("A Favorites change is still being saved. Retry to save this location.")
    }

    function saveFailed(message) {
        root.saving = false
        root.statusText = message
        if (message.indexOf("Connect failed:") === 0) {
            root.unacknowledgedUri = Mounts.normalize(root.pendingUri)
            root.unacknowledgedReason = message
        }
        root.retrying = true
        root.failedConnect = message.indexOf("Connect failed:") === 0
        form.focusHost()
    }

    Connections {
        target: Favourites
        function onCompleted(requestId, success, message) {
            if (!root.opened || !root.saving || requestId !== root.requestId) return
            if (!success) { root.saveFailed(message); return }
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
    function formNote() { return form.protocol === "NFS" ? "No credentials: NFS trusts the client host" : "" }
    function formAction() {
        if (root.saveCommitted) return "Close"
        if (root.saving) return "Saving..."
        if (root.connecting) return "Connecting..."
        return root.retrying ? "Retry" : "Connect and save"
    }
    function formMetrics() { return Math.round(card.padding) + "|" + Math.round(content.spacing) }
    // "contentY|contentHeight|height" of the scrolling body, so a test sees the clamp and the scroll.
    function bodyScroll() { return Math.round(body.contentY) + "|" + Math.round(body.contentHeight) + "|" + Math.round(body.height) }
    function formMetricTargets() { return Style.space(16) + "|" + Style.space(12) }

    function baseTitle() {
        var titles = { SMB: "SMB share", SFTP: "SFTP host", FTPS: "FTPS",
                       WebDAV: "WebDAV endpoint", NFS: "NFS export" }
        return titles[form.protocol] || titles.SMB
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

    Rectangle {
        id: scrim
        anchors.fill: parent
        // Util.alpha bakes the 0.7 scrim strength into the colour, the house idiom ConfirmDialog uses.
        color: Util.alpha(Theme.color.background, 0.7)
        // A backdrop only fades, no translate; same asymmetric open/close durations as card below.
        opacity: root.opened ? 1 : 0

        Behavior on opacity {
            enabled: !Theme.reducedMotion
            NumberAnimation {
                duration: root.opened ? Motion.durMs.open : Motion.durMs.close
                easing.type: Easing.BezierSpline
                easing.bezierCurve: Motion.bezierCurve
            }
        }

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
        width: Math.max(0, Math.min(Theme.space(380) * Theme.dialogWidthRatio, root.width - 2 * root.clampMargin))
        // Clamped to the window; the body scrolls whatever the clamp cut, see ui/CardScroll.qml.
        height: Math.min(body.wanted + contentTopInset + contentBottomInset, root.height - 2 * root.clampMargin)
        anchors.centerIn: parent
        // Open rises into place; close does not translate (enabled: root.opened only), only fades,
        // faster than the open animation. root.opened itself already flipped above, synchronously.
        anchors.verticalCenterOffset: root.opened ? 0 : Motion.translateUpPx
        opacity: root.opened ? 1 : 0
        color: Theme.color.surface
        borderSpec: Border.flat(Theme.color.accent, Style.normalBorderWidth)
        radius: Style.cornerRadius
        padding: Style.space(16)

        Behavior on anchors.verticalCenterOffset {
            enabled: root.opened && !Theme.reducedMotion
            NumberAnimation { duration: Motion.durMs.open; easing.type: Easing.BezierSpline; easing.bezierCurve: Motion.bezierCurve }
        }

        Behavior on opacity {
            enabled: !Theme.reducedMotion
            NumberAnimation {
                duration: root.opened ? Motion.durMs.open : Motion.durMs.close
                easing.type: Easing.BezierSpline
                easing.bezierCurve: Motion.bezierCurve
            }
        }

        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onClicked: {}
            onWheel: function (wheel) { wheel.accepted = true }
        }

        Flea.CardScroll {
            id: body
            anchors.fill: parent
            anchors.topMargin: card.contentTopInset
            anchors.bottomMargin: card.contentBottomInset
            anchors.leftMargin: card.contentLeftInset
            anchors.rightMargin: card.contentRightInset

        Column {
            id: content
            width: parent.width
            // Outer rhythm and each section's own header-to-content gap read exact off tailscale/dropbox Panel.qml.
            spacing: Style.space(12)

            Column {
                width: parent.width
                spacing: Style.space(12)

                Text {
                    text: root.dialogTitle
                    color: Theme.color.foreground
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.body
                    font.bold: true
                    textFormat: Text.PlainText
                }

                NetworkForm {
                    id: form
                    width: parent.width
                    enabled: !root.busy && !root.saveCommitted
                    onSubmitted: root.submitLocation()
                    Keys.onEscapePressed: root.close()

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
                        color: Theme.color.error
                    }

                    Text {
                        width: parent.width - Theme.font.caption - parent.spacing
                        text: root.statusText
                        color: Theme.color.error
                        font.family: Theme.font.family
                        font.pixelSize: Theme.font.caption
                        wrapMode: Text.Wrap
                        maximumLineCount: 2
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                    }
                }

                // Network.html places Cancel before the primary action at the card's right edge.
                Row {
                    anchors.right: parent.right
                    spacing: Theme.spacing.gap

                    Flea.DialogButton {
                        label: "Cancel"
                        available: !root.saving
                        onActivated: root.close()
                    }

                    Flea.DialogButton {
                        label: root.formAction()
                        primary: true
                        available: !root.busy
                        onActivated: root.submitLocation()
                    }
                }
            }

            PanelSeparator {}

            Column {
                width: parent.width
                spacing: Style.space(10)

                PanelSectionHeader {
                    text: "DROPBOX"
                }

                // Nothing left to install is an unavailable action, not a live control that answers
                // nothing: the label states the fact and the ink says the press will not be taken.
                Flea.DialogButton {
                    label: root.dropboxInstalled ? "Dropbox is already installed" : "Install Dropbox"
                    available: !root.dropboxInstalled && !root.busy
                    onActivated: root.installDropbox()
                }
            }
        }
        }
    }
}
