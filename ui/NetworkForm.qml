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
    readonly property string password: passwordField.text
    // The TLS box flips dav/davs and ftp/ftps, and the canvas draws it ticked.
    property bool tls: true

    signal submitted()

    readonly property var spec: Protocols.fieldsFor(root.protocol)
    readonly property string uri: Protocols.uri({
        protocol: root.protocol, host: root.host, port: root.port, path: root.path,
        domain: root.domain, user: root.user, tls: root.tls
    })
    readonly property bool complete: Protocols.complete({ host: root.host, port: root.port })

    spacing: Style.space(12)
    // Every row's natural height whether or not the protocol shows it: the form keeps this height
    // through every chip, so the chips above and the buttons below never move under the pointer,
    // and a shorter protocol leaves its air under Mounts as rather than a hole between fields.
    // The three protocol-bound rows count once, as the tallest of them: they never show together.
    readonly property real tallest: {
        var swapped = [domainField, nfsNote, tlsRow]
        var sum = 0
        var shown = 0
        for (var i = 0; i < root.children.length; i++) {
            if (swapped.indexOf(root.children[i]) >= 0)
                continue
            sum += root.children[i].implicitHeight
            shown++
        }
        var tallestSwapped = 0
        for (var j = 0; j < swapped.length; j++)
            tallestSwapped = Math.max(tallestSwapped, swapped[j].implicitHeight)
        return sum + tallestSwapped + root.spacing * shown
    }
    height: root.tallest

    function pick(name) {
        root.protocol = name
        // The port follows the scheme, because it is that scheme's own default and not a memory.
        portField.text = String(Protocols.defaultPort(name, root.tls))
        root.rehome()
    }

    // The box picks half of the scheme, so it picks half of the port: dav is 80 where davs is 443,
    // and a number left behind by the tick offers a port the scheme the form built does not use.
    onTlsChanged: portField.text = String(Protocols.defaultPort(root.protocol, root.tls))

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
        ring.push(labelField, hostField, portField, pathField, domainField, userField, passwordField, tlsRow)
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
        passwordField.text = ""
        root.tls = true
        root.pick("SMB")
    }

    function load(values) {
        root.reset()
        root.pick(values.protocol || "SMB")
        // The box before the port: onTlsChanged above prefills the scheme's own default, and after
        // the port it would overwrite the one this saved location actually carries.
        root.tls = values.tls !== false
        labelField.text = values.label || ""
        hostField.text = values.host || ""
        portField.text = values.port || String(Protocols.defaultPort(root.protocol, root.tls))
        pathField.text = values.path || ""
        domainField.text = values.domain || ""
        userField.text = values.user || ""
        passwordField.text = values.password || ""
    }

    function takePassword() {
        var value = passwordField.text
        passwordField.text = ""
        return value
    }

    // Non-secret form observables: tests may see shape, focus and masking, never field contents.
    function visibleFields() {
        var out = ["Label", "Host", "Port", root.spec.pathLabel]
        if (root.spec.domain) out.push("Domain")
        if (root.spec.credentials) out.push("Username", "Password")
        if (root.spec.tls) out.push("TLS")
        return out.join("|")
    }

    function focusName() {
        var fields = [labelField, hostField, portField, pathField, domainField, userField, passwordField]
        for (var i = 0; i < fields.length; i++) {
            if (fields[i].focused) return fields[i].label
        }
        return tlsRow.focused ? "TLS" : ""
    }

    function hostPortWidths() { return Math.round(hostField.width) + "|" + Math.round(portField.width) }
    function passwordState() {
        return (passwordField.input.echoMode === TextInput.Password ? "masked" : "visible")
            + "|" + (passwordField.text.length > 0 ? "set" : "empty")
    }
    function passwordEyeCentre() {
        var input = passwordField.input
        var hit = Math.max(Theme.hitMin, Theme.font.bodySmall)
        var point = input.mapToItem(null, input.width + Theme.spacing.gap + hit / 2, input.height / 2)
        return Math.round(point.x) + " " + Math.round(point.y)
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
        spacing: Style.space(10)

        Flea.DialogField {
            id: hostField
            width: (parent.width - parent.spacing) / 2
            label: "Host"
            onAccepted: root.submitted()
            onTabbed: function (from, back) { root.step(from, back ? -1 : 1) }
        }

        Flea.DialogField {
            width: hostField.width
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

    Flea.DialogField {
        id: userField
        width: parent.width
        visible: root.spec.credentials
        height: visible ? implicitHeight : 0
        label: "Username"
        onAccepted: root.submitted()
        onTabbed: function (from, back) { root.step(from, back ? -1 : 1) }
    }

    Flea.DialogField {
        id: passwordField
        width: parent.width
        visible: root.spec.credentials
        height: visible ? implicitHeight : 0
        label: "Password"
        secret: true
        onAccepted: root.submitted()
        onTabbed: function (from, back) { root.step(from, back ? -1 : 1) }
    }

    Text {
        id: nfsNote
        visible: root.protocol === "NFS"
        width: parent.width
        height: visible ? implicitHeight : 0
        text: "No credentials: NFS trusts the client host"
        color: Theme.color.muted
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
    }

    Item {
        id: tlsRow
        width: parent.width
        visible: root.spec.tls
        implicitHeight: Theme.rowHeight - Theme.spacing.rowPaddingY
        height: visible ? implicitHeight : 0

        readonly property bool focused: tlsRow.activeFocus
        signal tabbed(var from, bool back)
        function takeFocus() { tlsRow.forceActiveFocus() }
        onTabbed: function (from, back) { root.step(from, back ? -1 : 1) }
        Keys.onTabPressed: function(event) { tlsRow.tabbed(tlsRow, (event.modifiers & Qt.ShiftModifier) !== 0) }
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
            font.pixelSize: Theme.font.body
            textFormat: Text.PlainText
        }

        TapHandler {
            acceptedButtons: Qt.LeftButton
            gesturePolicy: TapHandler.ReleaseWithinBounds
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
