import QtQuick
import qs.Commons
import "." as Flea
import "js/Convert.js" as Convert
import "js/Format.js" as Format

// The one popup in the whole design. Every other operation answers in the status bar; this one asks two questions first, so it is the exception the operations design names rather than a pattern.
Item {
    id: root

    property bool opened: false
    // The card's title, for ui/Ipc.qml: a driven click on it proves the card swallows what its controls do not.
    readonly property alias titleItem: title
    property string name: ""
    property Item focusHolder: null
    property var owner: null
    property var source: null
    property int serial: 0
    property int requestId: 0
    property int operationId: 0
    property bool checking: false
    property bool checkAgain: false
    property bool busy: false
    property bool unavailable: false
    property bool collision: false
    property string errorText: ""
    property int focusPart: 0

    // The format row that starts picked is never the one the file already is.
    property string format: ""
    property bool strip: false
    property int cursor: 0
    // Where a row or the ground last saw the pointer, -1 since opening, so a row built under a resting pointer never takes the cursor.
    property point pointerGlobal: Qt.point(-1, -1)

    signal accepted(var source, string format, bool strip, int requestId)

    // The card title, filled by Format.fill.
    readonly property string titleWord: "Convert %1"
    readonly property var formats: Convert.FORMATS
    readonly property string outputPath: root.source ? Convert.destination(root.source, root.format) : ""
    readonly property bool editable: !root.busy && !root.unavailable
    readonly property bool canConvert: root.opened && root.editable && !root.checking && !root.collision && !root.errorText
    readonly property var cancelItem: cancelButton
    readonly property var submitItem: convertButton
    readonly property var metadataItem: metadata
    readonly property var outputItem: output
    readonly property var bodyItem: body
    function formatItem(index) { return formatRows.itemAt(index) }
    // What each row prints. The canvas draws JPEG, WebP and AVIF, which is neither the extension
    // nor a plain upper-casing of it, so the wording is a table and not a rule.
    readonly property var formatLabels: ({
        jpg: "JPEG", png: "PNG", webp: "WebP", avif: "AVIF", heic: "HEIC", tiff: "TIFF", bmp: "BMP"
    })
    // The canvas draws this popup at 300 design pixels wide.
    readonly property int dialogWidth: 300
    readonly property int clampMargin: Theme.card.inset
    // var, not Item: BorderSurface is a qs.Ui type qmllint cannot resolve, and Item would read as incompatible.
    readonly property var cardItem: card

    anchors.fill: parent
    visible: root.opened || root.opacity > 0
    // Closed means no input, even while the fade-out is still drawn.
    enabled: root.opened
    opacity: 0
    z: 2

    Flea.Reveal { target: root; shown: root.opened; lift: card }

    // A codec this box converts to but the canvas does not name still needs a cap, so upper-casing
    // the extension is the fallback rather than an empty row.
    function formatLabel(id) {
        var text = root.formatLabels[id]
        return text !== undefined ? text : String(id).toUpperCase()
    }

    function open(rowName, holder) {
        if (root.opened || !holder || !holder.convertSource) return
        root.owner = holder
        root.source = Object.assign({}, holder.convertSource)
        root.name = root.source.name
        root.format = Convert.defaultFormat(root.name)
        root.strip = false
        root.cursor = root.formats.indexOf(root.format)
        root.pointerGlobal = Qt.point(-1, -1)
        root.focusHolder = holder.listArea
        root.errorText = ""
        root.collision = false
        root.busy = false
        root.unavailable = false
        root.checking = false
        root.checkAgain = false
        root.focusPart = 0
        root.opened = true
        body.contentY = 0
        root.formatItem(root.cursor).forceActiveFocus()
        root.probe()
    }

    function close() {
        if (!root.opened || root.busy)
            return
        root.opened = false
        if (root.focusHolder)
            root.focusHolder.forceActiveFocus()
    }

    function commit() {
        if (!root.canConvert) return
        keys.forceActiveFocus()
        root.busy = true
        root.operationId = 0
        root.requestId = ++root.serial
        root.accepted(root.source, root.format, root.strip, root.requestId)
    }

    function probe() {
        if (!root.opened || !root.editable) return
        if (root.checking) { root.checkAgain = true; return }
        root.checking = true
        root.errorText = ""
        root.collision = false
        root.requestId = ++root.serial
        root.owner.backend.convertImage(root.source.path, root.outputPath, root.strip, root.source.menuId, root.requestId, true)
    }

    function chooseFormat(index) {
        if (!root.editable) return
        root.focusPart = 0
        root.cursor = Math.max(0, Math.min(root.formats.length - 1, index))
        var next = root.formats[root.cursor]
        if (root.format !== next || root.errorText) {
            root.format = next
            root.probe()
        }
        body.reveal(root.formatItem(root.cursor))
        root.formatItem(root.cursor).forceActiveFocus()
    }

    function stepFocus(back) {
        if (root.busy) { keys.forceActiveFocus(); return }
        var parts = root.unavailable ? [2] : root.canConvert ? [0, 1, 2, 3] : [0, 1, 2]
        var at = parts.indexOf(root.focusPart)
        root.focusPart = parts[(at + (back ? parts.length - 1 : 1)) % parts.length]
        var item = [root.formatItem(root.cursor), metadata, cancelButton, convertButton][root.focusPart]
        item.forceActiveFocus()
        body.reveal(item)
    }

    Connections {
        target: root.owner ? root.owner.backend : null
        function onConvertChecked(message) {
            if (!root.opened || root.unavailable || !Convert.matchesReply(root.source, root.requestId, message)) return
            root.checking = false
            if (root.checkAgain || message.path !== root.outputPath) { root.checkAgain = false; root.probe(); return }
            root.collision = message.collision === true
            root.errorText = message.ok && !root.collision ? "" : message.error
        }
        function onConvertStarted(id, requestId, source) {
            if (root.busy && Convert.matchesReply(root.source, root.requestId, {requestId: requestId, source: source}))
                root.operationId = id
        }
        function onConvertDone(id, ok, path, err, requestId, source, collision) {
            if (!root.opened || !root.busy || !Convert.matchesReply(root.source, root.requestId, {requestId: requestId, source: source})
                    || (root.operationId ? id !== root.operationId : ok)) return
            root.busy = false
            root.collision = collision
            root.errorText = err
            if (ok) root.close()
            else root.formatItem(root.cursor).forceActiveFocus()
        }
        function onFailed(where, input, message, mode) {
            if (!root.opened || root.unavailable || (where !== "backend" && where !== "read")) return
            var pending = root.busy
            root.unavailable = true
            root.busy = false
            root.checking = false
            root.checkAgain = false
            root.focusPart = 2
            cancelButton.forceActiveFocus()
            root.errorText = pending
                ? "Flea stopped responding, so the conversion may not have finished. Check the converted file."
                : "Flea stopped responding. Reopen Flea, then convert again."
        }
    }

    // A dimmed ground, and a click on it is a cancel, the same shape the network dialog already uses.
    Rectangle {
        anchors.fill: parent
        color: Theme.color.background
        opacity: 0.5

        MouseArea {
            id: ground
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            hoverEnabled: true
            onEntered: root.pointerGlobal = ground.mapToItem(null, ground.mouseX, ground.mouseY)
            onPositionChanged: function (mouse) { root.pointerGlobal = ground.mapToItem(null, mouse.x, mouse.y) }
            onClicked: root.close()
            onWheel: function (wheel) { wheel.accepted = true }
        }
    }

    Rectangle {
        id: card
        x: Theme.cardOrigin(root.width, width)
        y: Theme.cardOrigin(root.height, height)
        width: Theme.cardSpan(Theme.space(root.dialogWidth) * Theme.dialogWidthRatio, root.width - 2 * root.clampMargin)
        // Clamped to the window; the body scrolls whatever the clamp cut, see ui/CardScroll.qml.
        height: Theme.cardSpan(body.wanted + 2 * Theme.spacing.rowPaddingX, root.height - 2 * root.clampMargin)
        color: Theme.color.surface
        border.width: Theme.spacing.hairline
        border.color: Theme.color.mutedFrame
        radius: Style.cornerRadius

        // The strip toggle's handler takes a passive grab, so without this sink its press fell
        // through the card to the ground below, which closed the dialog on release.
        MouseArea {
            id: cardSink
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            hoverEnabled: true
            onEntered: root.pointerGlobal = cardSink.mapToItem(null, cardSink.mouseX, cardSink.mouseY)
            onPositionChanged: function (mouse) { root.pointerGlobal = cardSink.mapToItem(null, mouse.x, mouse.y) }
            onClicked: {}
            onWheel: function (wheel) { wheel.accepted = true }
        }

        Flea.CardFrame {
            id: body
            bleedY: Theme.focusClearance
            anchors.fill: parent
            anchors.topMargin: Theme.spacing.rowPaddingX - Theme.focusClearance
            anchors.bottomMargin: Theme.spacing.rowPaddingX - Theme.focusClearance
            spacing: Theme.spacing.gap / 2

            title: Item {
                id: title
                width: parent.width
                height: header.implicitHeight + Theme.spacing.gap

                Flea.DialogTitle {
                    id: header
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.margins: Theme.spacing.rowPaddingX
                    text: Format.fill(root.titleWord, root.name)
                    elide: Text.ElideMiddle
                }

                Rectangle {
                    anchors.bottom: parent.bottom
                    width: parent.width
                    height: Theme.spacing.hairline
                    color: Theme.color.mutedFrame
                    opacity: 0.4
                }
            }

            buttons: Item {
                width: parent.width
                height: Theme.spacing.gap + Math.max(cancelButton.implicitHeight, convertButton.implicitHeight)
                Row {
                    anchors.bottom: parent.bottom
                    anchors.right: parent.right
                    anchors.rightMargin: Theme.spacing.rowPaddingX
                    spacing: Theme.spacing.gap

                    Flea.DialogButton {
                        id: cancelButton
                        label: "Cancel"
                        focused: root.focusPart === 2
                        available: !root.busy
                        enabled: available
                        Keys.forwardTo: [keys]
                        onActivated: root.close()
                    }

                    Flea.DialogButton {
                        id: convertButton
                        label: "Convert"
                        primary: true
                        focused: root.focusPart === 3
                        available: root.canConvert
                        enabled: available
                        Keys.forwardTo: [keys]
                        onActivated: root.commit()
                    }
                }
            }

            Column {
                width: parent.width
                spacing: Theme.spacing.gap / 2

                Repeater {
                    id: formatRows
                    model: root.formats
                    delegate: Flea.MenuRow {
                        required property string modelData
                        required property int index
                        width: body.holderWidth
                        enabled: root.editable
                        entry: ({ label: root.formatLabel(modelData), action: modelData, glyph: "image",
                            labelColor: Theme.color.foreground, disabled: !root.editable })
                        picked: root.format === modelData
                        current: root.focusPart === 0 && root.cursor === index
                        Accessible.role: Accessible.RadioButton
                        Accessible.name: root.formatLabel(modelData)
                        Accessible.checkable: true
                        Accessible.checked: root.format === modelData
                        Keys.forwardTo: [keys]
                        Accessible.onPressAction: root.chooseFormat(index)
                        lastPointerGlobal: root.pointerGlobal
                        onPointerSeen: function (at) { root.pointerGlobal = at }
                        onPointerMoved: { root.focusPart = 0; root.cursor = index }
                        onActivated: root.chooseFormat(index)
                    }
                }

                Rectangle {
                    width: parent.width
                    height: Theme.spacing.hairline
                    color: Theme.color.mutedFrame
                    opacity: 0.4
                }

                // The rule's checkbox row; Space and a tap toggle it.
                Item {
                    id: metadata
                    width: parent.width
                    height: Theme.rowHeight
                    enabled: root.editable
                    Accessible.role: Accessible.CheckBox
                    Accessible.name: "Remove metadata"
                    Accessible.checkable: true
                    Accessible.checked: root.strip
                    Keys.forwardTo: [keys]
                    Accessible.onPressAction: if (root.editable) root.strip = !root.strip

                    Item {
                        id: box
                        anchors.left: parent.left
                        anchors.leftMargin: Theme.spacing.rowPaddingX
                        anchors.verticalCenter: parent.verticalCenter
                        width: ruleBox.implicitWidth
                        height: ruleBox.implicitHeight

                        Flea.CheckBox {
                            id: ruleBox
                            value: root.strip ? "on" : "off"
                            focused: root.focusPart === 1
                            available: root.editable
                        }
                    }

                    Text {
                        anchors.left: box.right
                        anchors.leftMargin: Theme.spacing.gap
                        anchors.verticalCenter: parent.verticalCenter
                        // Unchecked by default: a user converting a photo does not expect metadata
                        // silently dropped unless they asked for it.
                        text: "Remove metadata"
                        color: Theme.color.foreground
                        font.family: Theme.font.family
                        font.pixelSize: Theme.font.body
                        textFormat: Text.PlainText
                    }

                    TapHandler {
                        acceptedButtons: Qt.LeftButton
                        gesturePolicy: TapHandler.ReleaseWithinBounds
                        onTapped: if (root.editable) { root.focusPart = 1; root.strip = !root.strip; metadata.forceActiveFocus() }
                    }
                }

                Text {
                    id: output
                    x: Theme.spacing.rowPaddingX
                    width: parent.width - 2 * Theme.spacing.rowPaddingX
                    topPadding: 2 * Theme.spacing.hairline
                    bottomPadding: 2 * Theme.spacing.hairline
                    text: "Output: " + root.outputPath
                    color: Theme.color.foreground
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    textFormat: Text.PlainText
                    wrapMode: Text.WrapAnywhere
                }

                // The status slot is always laid out, two caption lines tall (the network slot's rowHeight) inside its padding, so rest, busy and a short error keep one card height; only a longer error grows it.
                Text {
                    x: Theme.spacing.rowPaddingX
                    width: parent.width - 2 * Theme.spacing.rowPaddingX
                    height: Math.max(Theme.rowHeight + topPadding + bottomPadding, implicitHeight)
                    topPadding: 2 * Theme.spacing.hairline
                    bottomPadding: 2 * Theme.spacing.hairline
                    text: root.busy ? "Converting…" : root.errorText
                    color: root.busy ? Theme.color.muted : Theme.color.errorOnSurface
                    font { family: Theme.font.family; pixelSize: Theme.font.caption }
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }

                Text {
                    x: Theme.spacing.rowPaddingX
                    width: parent.width - 2 * Theme.spacing.rowPaddingX
                    topPadding: 2 * Theme.spacing.hairline
                    bottomPadding: 2 * Theme.spacing.hairline
                    visible: root.collision
                    text: "Choose another format."
                    color: Theme.color.foreground
                    font { family: Theme.font.family; pixelSize: Theme.font.caption }
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }
            }
        }
    }

    Item {
        id: keys
        anchors.fill: parent

        Keys.onPressed: function (event) {
            event.accepted = true
            if (event.key === Qt.Key_Escape) { root.close(); event.accepted = true; return }
            if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) {
                root.stepFocus(event.key === Qt.Key_Backtab || (event.modifiers & Qt.ShiftModifier)); return
            }
            if (root.busy) return
            if (root.unavailable) {
                if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) root.close()
                return
            }
            if (event.key === Qt.Key_Down) {
                root.chooseFormat(root.cursor + 1)
                event.accepted = true
                return
            }
            if (event.key === Qt.Key_Up) {
                root.chooseFormat(root.cursor - 1)
                event.accepted = true
                return
            }
            // Space toggles the one checkbox, which is the only other thing this popup asks.
            if (event.key === Qt.Key_Space && root.focusPart < 2) { root.strip = !root.strip; return }
            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
                if (root.focusPart === 2) root.close()
                else if (root.focusPart === 1) root.strip = !root.strip
                else root.commit()
            }
        }
    }
}
