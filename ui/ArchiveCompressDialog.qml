import QtQuick
import qs.Commons
import "." as Flea
import "js/Archive.js" as Archive
import "js/ArchiveCard.js" as ArchiveCard

// The With-password compress card: format, name, the password pair and the header switch, on the shipped dialog controls.
Item {
    id: root

    property bool opened: false
    property var owner: null
    property var source: null
    property int count: 0
    property string parentLeaf: ""
    property var formats: ["7z", "zip"]
    property string format: "7z"
    property string stem: ""
    property bool encryptNames: true
    property string errorText: ""
    property int focusPart: 0

    signal accepted(var source, string format, string name, string password, bool encryptNames)

    readonly property string password: passwordField.text
    readonly property string confirm: confirmField.text
    readonly property bool matched: root.password === root.confirm
    // A password card compresses only with a password typed twice, so an empty pair never reaches the wire.
    readonly property bool canCompress: root.opened && root.matched && root.password.length > 0 && root.name.length > 0
    readonly property string name: nameField.text
    readonly property var cancelItem: cancelButton
    readonly property var submitItem: compressButton
    readonly property var bodyItem: body
    readonly property var cardItem: card

    // What a capture reads instead of the fields: the eye, which field wears the error frame, never a typed character.
    function captureState() {
        return { encrypt: root.encryptNames, encryptShown: encryptRow.visible, revealed: passwordField.revealed, eye: passwordField.eyeCentre(),
                 passwordError: passwordField.error, confirmError: confirmField.error }
    }

    anchors.fill: parent
    visible: root.opened || root.opacity > 0
    // Closed means no input, even while the fade-out is still drawn.
    enabled: root.opened
    opacity: 0
    z: 2

    Flea.Reveal { target: root; shown: root.opened; lift: card }
    // The name clears once the fade ended, so the card stays still while it fades out; open() sets it again.
    onOpacityChanged: if (root.opacity === 0 && !root.opened) nameField.text = ""

    // The tab order; a disabled stop is skipped, so Tab never lands on a button that does nothing.
    function tabStops() {
        var stops = [formatRow, nameField.input, passwordField.input, confirmField.input]
        if (root.format === "7z") stops.push(encryptRow)
        return stops.concat([cancelButton, compressButton]).filter(function (s) { return s.enabled })
    }

    function stepTab(back) {
        var stops = root.tabStops()
        var at = -1
        for (var i = 0; i < stops.length; i++) {
            if (stops[i].activeFocus) { at = i; break }
        }
        var next = at < 0 ? (back ? stops.length - 1 : 0)
                          : (at + (back ? stops.length - 1 : 1)) % stops.length
        // The rings read focusPart, so a ring follows the tab stop.
        if (stops[next] === cancelButton) root.focusPart = 5
        else if (stops[next] === compressButton) root.focusPart = 6
        else if (stops[next] === encryptRow) root.focusPart = 4
        else if (stops[next] === formatRow) root.focusPart = 1
        else root.focusPart = 0
        stops[next].forceActiveFocus()
    }

    Keys.onEscapePressed: function (event) { root.close(); event.accepted = true }
    Keys.onTabPressed: function (event) { root.stepTab((event.modifiers & Qt.ShiftModifier) !== 0); event.accepted = true }
    Keys.onBacktabPressed: function (event) { root.stepTab(true); event.accepted = true }
    Keys.onPressed: function (event) {
        if (formatRow.activeFocus
                && (event.key === Qt.Key_Left || event.key === Qt.Key_Right)) {
            var at = root.formats.indexOf(root.format)
            root.chooseFormat(at + (event.key === Qt.Key_Right ? 1 : -1))
            event.accepted = true
            return
        }
        if (encryptRow.activeFocus && event.key === Qt.Key_Space) {
            root.encryptNames = !root.encryptNames
            event.accepted = true
            return
        }
        if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
                && (formatRow.activeFocus || encryptRow.activeFocus)) {
            root.commit()
            event.accepted = true
        }
    }

    // Test seam for the threat walk: types into the pair the way keys would, the confirm apart when asked.
    function takeSecretForTest(secret, confirm) {
        passwordField.text = secret
        confirmField.text = confirm === undefined ? secret : confirm
    }

    // The source is { paths, menuId }, kept whole so the menu id reaches the wire with the submit.
    function open(source, leaf, holder) {
        if (root.opened || !holder || !source || !source.paths || source.paths.length === 0) return
        var paths = source.paths
        root.owner = holder
        var leaves = ArchiveCard.pathLeaves(paths)
        root.source = source
        root.count = paths.length
        root.parentLeaf = leaf || ""
        // The board's order, 7z first, whatever order the backend reports its formats in.
        var have = holder.backend.archiveFormats || []
        var offered = ["7z", "zip"].filter(function (f) { return have.indexOf(f) >= 0 })
        root.formats = offered.length > 0 ? offered : ["7z"]
        root.format = root.formats.indexOf("7z") >= 0 ? "7z" : root.formats[0]
        root.stem = Archive.archiveStem(leaves, root.parentLeaf)
        root.encryptNames = true
        root.errorText = ""
        root.focusPart = 2
        root.opened = true
        body.contentY = 0
        nameField.text = root.stem + "." + root.format
        passwordField.text = ""
        confirmField.text = ""
        passwordField.takeFocus()
    }

    // The fields clear on close, so a secret never outlives the card that took it.
    function close() {
        if (!root.opened)
            return
        passwordField.text = ""
        confirmField.text = ""
        root.stem = ""
        root.source = null
        root.opened = false
        if (root.owner && root.owner.listArea)
            root.owner.listArea.forceActiveFocus()
    }

    function chooseFormat(index) {
        var next = root.formats[Math.max(0, Math.min(root.formats.length - 1, index))]
        if (next === root.format) return
        root.format = next
        nameField.text = root.stem + "." + next
    }

    function commit() {
        if (!root.opened) return
        if (!root.canCompress) {
            if (!root.matched) root.errorText = "Passwords do not match."
            else if (root.password.length === 0) root.errorText = "Type a password first."
            // Only an empty name is left: the old sentence goes and the focus moves to that field.
            else { root.errorText = ""; root.focusPart = 0; nameField.takeFocus() }
            return
        }
        root.errorText = ""
        // Accepted before close, so the owner's submit still sees the source; close clears the fields.
        root.accepted(root.source, root.format, root.name, root.password,
                      root.encryptNames && root.format === "7z")
        root.close()
    }

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
                text: ArchiveCard.compressTitle(root.count)
                elide: Text.ElideMiddle
            }

            buttons: Item {
                width: parent.width
                height: Math.max(cancelButton.implicitHeight, compressButton.implicitHeight)
                Row {
                    anchors.bottom: parent.bottom
                    anchors.right: parent.right
                    anchors.rightMargin: Theme.card.padding
                    spacing: Theme.spacing.gap

                    Flea.DialogButton {
                        id: cancelButton
                        label: "Cancel"
                        focused: root.focusPart === 5
                        onActivated: root.close()
                        onTabbed: function (from, back) { root.stepTab(back) }
                    }

                    Flea.DialogButton {
                        id: compressButton
                        label: "Compress"
                        primary: true
                        focused: root.focusPart === 6
                        onActivated: root.commit()
                        onTabbed: function (from, back) { root.stepTab(back) }
                    }
                }
            }

            Column {
                width: parent.width
                spacing: Theme.card.gap

                // The segment's own caption, in the same capitals and at the same distance DialogField draws above its box.
                Column {
                    x: Theme.card.padding
                    width: parent.width - 2 * Theme.card.padding
                    spacing: Theme.card.labelGap

                    Text {
                        width: parent.width
                        text: "FORMAT"
                        color: Theme.color.muted
                        font.family: Theme.font.family
                        font.pixelSize: Theme.font.caption
                        font.letterSpacing: Theme.font.caption * nameField.labelSpacing
                        textFormat: Text.PlainText
                    }

                    Flea.SettingsSegment {
                        id: formatRow
                        tabStop: true
                        options: root.formats
                        value: root.format
                        onPicked: function (index) { root.chooseFormat(index) }
                    }
                }

                Flea.DialogField {
                    id: nameField
                    x: Theme.card.padding
                    width: parent.width - 2 * Theme.card.padding
                    label: "Name"
                    placeholder: "Archive.7z"
                    onAccepted: root.commit()
                    onTabbed: function (from, back) { root.stepTab(back) }
                }

                Flea.DialogField {
                    id: passwordField
                    x: Theme.card.padding
                    width: parent.width - 2 * Theme.card.padding
                    label: "Password"
                    secret: true
                    error: root.errorText.length > 0 && root.password.length === 0
                    onEdited: root.errorText = ""
                    onAccepted: root.commit()
                    onTabbed: function (from, back) { root.stepTab(back) }
                }

                // The extract card's recipe: the refused field in the error frame, its bare sentence directly under the pair.
                Column {
                    x: Theme.card.padding
                    width: parent.width - 2 * Theme.card.padding
                    spacing: Theme.spacing.gap

                    Flea.DialogField {
                        id: confirmField
                        width: parent.width
                        label: "Confirm"
                        secret: true
                        error: root.errorText.length > 0 && !root.matched
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

                Item {
                    id: encryptRow
                    width: parent.width
                    height: Theme.rowHeight
                    visible: root.format === "7z"
                    Accessible.role: Accessible.CheckBox
                    Accessible.name: "Encrypt file names"
                    Accessible.checkable: true
                    Accessible.checked: root.encryptNames

                    // An Item, not a Rectangle: a Rectangle with no color paints white behind the unchecked box.
                    Item {
                        id: box
                        anchors.left: parent.left
                        anchors.leftMargin: Theme.card.padding
                        anchors.verticalCenter: parent.verticalCenter
                        width: ruleBox.implicitWidth
                        height: ruleBox.implicitHeight

                        Flea.CheckBox {
                            id: ruleBox
                            value: root.encryptNames ? "on" : "off"
                            focused: root.focusPart === 4
                        }
                    }

                    Text {
                        anchors.left: box.right
                        anchors.leftMargin: Theme.spacing.gap
                        anchors.verticalCenter: parent.verticalCenter
                        text: "Encrypt file names"
                        color: Theme.color.foreground
                        font.family: Theme.font.family
                        font.pixelSize: Theme.font.body
                        textFormat: Text.PlainText
                    }

                    TapHandler {
                        acceptedButtons: Qt.LeftButton
                        gesturePolicy: TapHandler.ReleaseWithinBounds
                        onTapped: root.encryptNames = !root.encryptNames
                    }
                }
            }
        }
    }
}
