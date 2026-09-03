import QtQuick
import qs.Commons
import "." as Flea
import "js/Protocols.js" as Protocols

// The Add-network-place form: a protocol picks the scheme, prefills the port and swaps the field set,
// and the Mounts-as line always shows the exact URI this will hand to gio mount. A form over a URI,
// never a second mount system.
Column {
    id: root

    property string protocol: "SMB"
    // Every value is read out of the field that holds it, so nothing here can go stale against what
    // is on screen and a programmatic set reaches the field rather than a shadow copy.
    readonly property string label: labelField.text
    readonly property string host: hostField.text
    readonly property string port: portField.text
    readonly property string path: pathField.text
    readonly property string domain: domainField.text
    readonly property string user: userField.text
    // The TLS box flips dav/davs and ftp/ftps, and the canvas draws it ticked.
    property bool tls: true

    signal submitted()

    readonly property var spec: Protocols.fieldsFor(root.protocol)
    readonly property string uri: Protocols.uri({
        protocol: root.protocol, host: root.host, port: root.port, path: root.path,
        domain: root.domain, user: root.user, tls: root.tls
    })
    readonly property bool complete: Protocols.complete({ host: root.host })

    spacing: Theme.spacing.gap

    function pick(name) {
        root.protocol = name
        // The port follows the protocol, because it is that protocol's own default and not a memory.
        portField.text = String(Protocols.defaultPort(name))
        root.rehome()
    }

    // Qt keeps active focus on an item that has just gone invisible, so a chip click that hides the
    // focused field would leave the caret where nobody can see it and Enter still submits from.
    // The caret steps on to the next visible ring member instead; step already skips hidden ones.
    function rehome() {
        var ring = root.focusRing()
        for (var i = 0; i < ring.length; i++) {
            if (ring[i].focused && !ring[i].visible) {
                root.step(ring[i], 1)
                return
            }
        }
    }

    function focusHost() {
        hostField.input.forceActiveFocus()
    }

    // Visual order: the protocol chips, then the fields down the form, then the TLS box. Three of
    // these are visibility-bound and come and go with the protocol, so the walk skips what is not on
    // screen rather than parking the caret where nobody can see it.
    function focusRing() {
        var ring = []
        var kids = chips.children
        for (var i = 0; i < kids.length; i++) {
            // A Repeater is itself a child of the Row it fills, alongside its delegates, so the ring
            // takes only what can hold a caret; chipFor below dodges the same object by testing a label.
            if (typeof kids[i].takeFocus === "function") {
                ring.push(kids[i])
            }
        }
        ring.push(labelField, hostField, portField, pathField, domainField, userField, tlsRow)
        return ring
    }

    function step(from, delta) {
        var ring = root.focusRing()
        var len = ring.length
        var at = ring.indexOf(from)
        if (at < 0) {
            return
        }
        for (var n = 1; n <= len; n++) {
            var next = ring[((at + delta * n) % len + len) % len]
            if (next.visible) {
                next.takeFocus()
                return
            }
        }
    }

    // A chip carries a label and no accessibility tree, so a test reaches one by name and clicks it.
    function chipFor(name) {
        var kids = chips.children
        for (var i = 0; i < kids.length; i++) {
            if (kids[i].label === name)
                return kids[i]
        }
        return null
    }

    function reset() {
        labelField.text = ""
        hostField.text = ""
        pathField.text = ""
        domainField.text = ""
        userField.text = ""
        root.tls = true
        root.pick("SMB")
    }

    // Fill from Protocols.parse, without pick(), so a non-default port survives rather than
    // snapping back to the protocol's own default the way a chip click does.
    function load(parsed, name) {
        if (!parsed) {
            reset()
            return
        }
        root.protocol = parsed.protocol
        root.tls = !!parsed.tls
        portField.text = String(parsed.port || Protocols.defaultPort(parsed.protocol))
        labelField.text = name || ""
        hostField.text = parsed.host || ""
        pathField.text = parsed.path || ""
        domainField.text = parsed.domain || ""
        userField.text = parsed.user || ""
    }

    Row {
        id: chips
        spacing: Theme.spacing.hairline * 6

        Repeater {
            model: Protocols.PROTOCOLS
            delegate: Flea.ProtocolChip {
                required property string modelData
                label: modelData
                picked: root.protocol === modelData
                onActivated: root.pick(modelData)
                onTabbed: function (from, back) { root.step(from, back ? -1 : 1) }
            }
        }
    }

    Flea.DialogField {
        id: labelField
        width: parent.width
        label: "Label"
        placeholder: Protocols.label({ label: "", host: root.host, path: root.path })
        onAccepted: root.submitted()
        onTabbed: function (from, back) { root.step(from, back ? -1 : 1) }
    }

    Row {
        width: parent.width
        spacing: Theme.spacing.gap

        Flea.DialogField {
            id: hostField
            width: (parent.width - Theme.spacing.gap) * 0.72
            label: "Host"
            onAccepted: root.submitted()
            onTabbed: function (from, back) { root.step(from, back ? -1 : 1) }
        }

        Flea.DialogField {
            width: parent.width - hostField.width - Theme.spacing.gap
            id: portField
            label: "Port"
            onAccepted: root.submitted()
            onTabbed: function (from, back) { root.step(from, back ? -1 : 1) }
        }
    }

    // Share, Path or Export: the row is the same, the thing it names is not.
    Flea.DialogField {
        id: pathField
        width: parent.width
        label: root.spec.pathLabel
        onAccepted: root.submitted()
        onTabbed: function (from, back) { root.step(from, back ? -1 : 1) }
    }

    Flea.DialogField {
        id: domainField
        width: parent.width
        visible: root.spec.domain
        height: visible ? implicitHeight : 0
        label: "Domain"
        onAccepted: root.submitted()
        onTabbed: function (from, back) { root.step(from, back ? -1 : 1) }
    }

    // Username only. A password field was here and nothing consumed it: the form writes a bookmark,
    // and gio's own prompt is what asks for a secret at mount time. Collecting one to drop it is
    // worse than not collecting it, so the field comes back when credentials reach the mount attempt.
    Flea.DialogField {
        id: userField
        width: parent.width
        visible: root.spec.credentials
        height: visible ? implicitHeight : 0
        label: "Username"
        onAccepted: root.submitted()
        onTabbed: function (from, back) { root.step(from, back ? -1 : 1) }
    }

    Item {
        id: tlsRow
        width: parent.width
        visible: root.spec.tls
        height: visible ? Theme.rowHeight - Theme.spacing.rowPaddingY : 0

        readonly property bool focused: tlsRow.activeFocus
        signal tabbed(var from, bool back)
        function takeFocus() { tlsRow.forceActiveFocus() }
        onTabbed: function (from, back) { root.step(from, back ? -1 : 1) }
        Keys.onTabPressed: tlsRow.tabbed(tlsRow, false)
        Keys.onBacktabPressed: tlsRow.tabbed(tlsRow, true)
        Keys.onReturnPressed: root.tls = !root.tls
        Keys.onEnterPressed: root.tls = !root.tls
        Keys.onSpacePressed: root.tls = !root.tls

        Rectangle {
            id: tlsBox
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            width: Theme.font.caption
            height: Theme.font.caption
            color: "transparent"
            border.width: Theme.spacing.hairline * 2
            border.color: root.tls ? Theme.color.accent
                                   : (tlsRow.focused ? Theme.color.foreground : Theme.color.muted)

            Flea.Glyph {
                anchors.fill: parent
                visible: root.tls
                name: "check"
                color: Theme.color.accent
            }
        }

        Text {
            anchors.left: tlsBox.right
            anchors.leftMargin: Theme.spacing.gap
            anchors.verticalCenter: parent.verticalCenter
            text: "Encrypted (TLS)"
            color: Theme.color.foreground
            font.family: Theme.font.family
            font.pixelSize: Theme.font.bodySmall
            textFormat: Text.PlainText
        }

        TapHandler {
            acceptedButtons: Qt.LeftButton
            onTapped: root.tls = !root.tls
        }
    }

    // The exact URI, on one line that never wraps and never clips: it pans, and the edge fade says
    // there is more. A Flickable is what makes that a pan rather than an elide.
    Column {
        width: parent.width
        spacing: Theme.spacing.hairline * 2

        Text {
            text: "MOUNTS AS"
            color: Theme.color.muted
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            font.letterSpacing: Theme.font.caption * 0.1
            textFormat: Text.PlainText
        }

        Item {
            width: parent.width
            height: uriText.implicitHeight
            clip: true

            Flickable {
                anchors.fill: parent
                contentWidth: uriText.implicitWidth
                contentHeight: uriText.implicitHeight
                flickableDirection: Flickable.HorizontalFlick
                boundsBehavior: Flickable.StopAtBounds

                Text {
                    id: uriText
                    text: root.uri
                    color: Theme.color.muted
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    textFormat: Text.PlainText
                }
            }

            Rectangle {
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                visible: uriText.implicitWidth > parent.width
                width: Theme.space(36)
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0; color: "transparent" }
                    GradientStop { position: 1; color: Theme.color.surface }
                }
            }
        }
    }
}
