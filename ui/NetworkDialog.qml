import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "." as Flea
import "js/Mounts.js" as Mounts
import "js/Protocols.js" as Protocols
import "js/Motion.js" as Motion

// The Network group's add popup: a form over a gvfs URI, written to the same GTK bookmarks file
// nautilus writes, plus a separate Add Dropbox action that runs the stock installer.
//
// The form records a bookmark and mounts nothing, so it asks for a username (which the URI carries)
// and never for a password. gio's own prompt is what asks for a secret at mount time. The canvas
// draws a password row; it comes back when credentials actually reach a mount attempt, because a
// field that collects a secret and drops it is worse than no field at all.
Item {
    id: root

    property bool opened: false
    property string statusText: ""
    property bool dropboxInstalled: false

    signal closed()
    // Sidebar's own bookmarksFile FileView never watched a directory absent at its own
    // construction, so a plain watch is not enough the first run; the caller reloads on this.
    signal saved()

    // A plain overlay, not a QQC Popup, the same call ui/ContextMenu.qml already made.
    anchors.fill: parent
    // opened flips instantly (open()/close() above), so a caller reading it never races the close
    // fade; visible only stays true a little longer, until card's own opacity finishes it.
    visible: root.opened || card.opacity > 0

    function open() {
        root.statusText = ""
        form.reset()
        root.checkDropbox()
        root.opened = true
        // The host is the one field the form actually needs, so it takes the caret on open.
        form.focusHost()
    }

    function close() {
        root.opened = false
        root.closed()
    }

    // The URI the form built, which is the same one the Mounts-as line showed.
    function submitLocation() {
        if (!form.complete) {
            root.statusText = "A host is the one thing this needs; the rest is optional."
            return
        }
        root.appendBookmark(form.uri, form.labelText())
        root.close()
    }

    function appendBookmark(uri, label) {
        // Canonical form, so this line dedups against a later gio-reported mount of the same share.
        var canonical = Mounts.normalize(uri)
        // This view is not watched, so what it last read is not what the file holds: after the rail
        // has removed a place in the same session it still carries that line, and appending to it
        // wrote the removed place straight back. Re-read first, blocking, so the append is an append.
        bookmarksWrite.reload()
        bookmarksWrite.waitForJob()
        var body = bookmarksWrite.text()
        if (body.length > 0 && body.charAt(body.length - 1) !== "\n")
            body += "\n"
        bookmarksWrite.setText(body + canonical + " " + label + "\n")
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
        padding: Style.spacing.panelPadding

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
                spacing: Style.space(10)

                PanelSectionHeader {
                    text: "ADD NETWORK LOCATION"
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

                Text {
                    visible: root.statusText.length > 0
                    width: parent.width
                    text: root.statusText
                    color: Theme.color.error
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
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
                        label: "Save"
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
