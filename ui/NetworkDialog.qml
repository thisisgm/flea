import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "." as Flea
import "js/Mounts.js" as Mounts
import "js/Protocols.js" as Protocols
import "js/Motion.js" as Motion

// The Network group's popup: the approved form writes a secret-free GTK bookmark, then hands its
// session-only password to NetworkMounts for one helper stdin write.
Item {
    id: root

    property bool opened: false
    property string statusText: ""
    property bool dropboxInstalled: false
    property bool retrying: false
    property bool failedConnect: false
    readonly property string dialogTitle: root.baseTitle() + (root.failedConnect ? ", failed connect" : "")

    signal closed()
    // Sidebar's own bookmarksFile FileView never watched a directory absent at its own
    // construction, so a plain watch is not enough the first run; the caller reloads on this.
    signal saved()
    signal mountRequested(string uri, string label, string password)

    // A plain overlay, not a QQC Popup, the same call ui/ContextMenu.qml already made.
    anchors.fill: parent
    // opened flips instantly (open()/close() above), so a caller reading it never races the close
    // fade; visible only stays true a little longer, until card's own opacity finishes it.
    visible: root.opened || card.opacity > 0

    function open() {
        root.statusText = ""
        root.retrying = false
        root.failedConnect = false
        form.reset()
        root.checkDropbox()
        root.opened = true
        // The host is the one field the form actually needs, so it takes the caret on open.
        form.focusHost()
    }

    function openLocation(uri, label, password, reason, failed) {
        form.load(root.valuesFor(uri, label, password))
        root.statusText = reason || ""
        root.retrying = true
        root.failedConnect = failed === true
        root.checkDropbox()
        root.opened = true
        form.focusHost()
    }

    function close() {
        form.takePassword()
        root.opened = false
        root.closed()
    }

    // The URI the form built, which is the same one the Mounts-as line showed.
    function submitLocation() {
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
        var uri = Mounts.normalize(form.uri)
        var label = form.labelText()
        var password = form.takePassword()
        root.appendBookmark(uri, label)
        root.mountRequested(uri, label, password)
        password = ""
        root.close()
    }

    function appendBookmark(uri, label) {
        var canonical = Mounts.normalize(uri)
        var lines = String(bookmarksWrite.text() || "").split("\n")
        var out = []
        var found = false
        for (var i = 0; i < lines.length; i++) {
            var line = lines[i].trim()
            if (line.length === 0) continue
            var space = line.indexOf(" ")
            var oldUri = space < 0 ? line : line.substring(0, space)
            if (Mounts.normalize(oldUri) === canonical) {
                if (!found) out.push(canonical + " " + label)
                found = true
            } else {
                out.push(line)
            }
        }
        if (!found) out.push(canonical + " " + label)
        bookmarksWrite.setText(out.join("\n") + "\n")
        // Blocks until this write actually lands, not just until it is queued: without it,
        // Sidebar's reload() (fired by saved(), below) can race the write and read stale content.
        bookmarksWrite.waitForJob()
        root.saved()
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
    function formAction() { return root.retrying ? "Retry" : "Save" }
    function formMetrics() { return Math.round(card.padding) + "|" + Math.round(content.spacing) }
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

    FileView {
        id: bookmarksWrite
        path: Quickshell.env("HOME") + "/.config/gtk-3.0/bookmarks"
        printErrors: false
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
            onClicked: root.close()
        }
    }

    // The house dialog shape read off ConfirmDialog.qml: BorderSurface, an accent border, cornerRadius.
    BorderSurface {
        id: card
        width: Theme.space(380)
        height: content.implicitHeight + contentTopInset + contentBottomInset
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
            onClicked: {}
        }

        Column {
            id: content
            anchors.top: parent.top
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.topMargin: card.contentTopInset
            anchors.leftMargin: card.contentLeftInset
            anchors.rightMargin: card.contentRightInset
            // Outer rhythm and each section's own header-to-content gap read exact off tailscale/dropbox Panel.qml.
            spacing: Style.space(12)

            Column {
                width: parent.width
                spacing: Style.space(12)

                Text {
                    text: root.dialogTitle
                    color: Theme.color.foreground
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.bodySmall
                    font.weight: Font.Bold
                    textFormat: Text.PlainText
                }

                NetworkForm {
                    id: form
                    width: parent.width
                    onSubmitted: root.submitLocation()
                    Keys.onEscapePressed: root.close()

                    function labelText() {
                        return Protocols.label({ label: form.label, host: form.host, path: form.path })
                    }
                }

                Row {
                    visible: root.statusText.length > 0
                    width: parent.width
                    spacing: Theme.spacing.gap

                    Flea.Glyph {
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
                        textFormat: Text.PlainText
                    }
                }

                // Network.dc.html draws the accept pair right-aligned and reading Cancel then Save,
                // in the same hairline treatment ui/ConvertDialog.qml already uses; the stock Button
                // that stood here painted OEM chrome instead of the palette Flea reads from colors.toml.
                Row {
                    anchors.right: parent.right
                    spacing: Theme.spacing.gap

                    Flea.DialogButton {
                        label: "Cancel"
                        onActivated: root.close()
                    }

                    Flea.DialogButton {
                        label: root.formAction()
                        primary: true
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

                Flea.DialogButton {
                    label: root.dropboxInstalled ? "Dropbox is already installed" : "Install Dropbox"
                    onActivated: if (!root.dropboxInstalled) root.installDropbox()
                }
            }
        }
    }
}
