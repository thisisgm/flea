import QtQuick
import qs.Commons
import "." as Flea
import "js/Convert.js" as Convert

// The one popup in the whole design. Every other operation answers in the status bar; this one asks
// two questions first, so it is the exception the operations design names rather than a pattern.
Item {
    id: root

    property bool opened: false
    // The card's title, for ui/Ipc.qml: a driven click on it proves the card swallows what its controls do not.
    readonly property alias titleItem: title
    property string name: ""
    property Item focusHolder: null

    // The format row that starts picked is never the one the file already is.
    property string format: ""
    property bool strip: false
    property int cursor: 0

    signal accepted(string format, bool strip)

    readonly property var formats: Convert.FORMATS
    // What each row prints. The canvas draws JPEG, WebP and AVIF, which is neither the extension
    // nor a plain upper-casing of it, so the wording is a table and not a rule.
    readonly property var formatLabels: ({
        jpg: "JPEG", png: "PNG", webp: "WebP", avif: "AVIF", heic: "HEIC", tiff: "TIFF", bmp: "BMP"
    })
    // The canvas draws this popup at 300 design pixels wide.
    readonly property int dialogWidth: 300
    readonly property int clampMargin: 8
    // var, not Item: BorderSurface is a qs.Ui type qmllint cannot resolve, and Item would read as incompatible.
    readonly property var cardItem: card

    anchors.fill: parent
    visible: root.opened
    z: 2

    // A codec this box converts to but the canvas does not name still needs a cap, so upper-casing
    // the extension is the fallback rather than an empty row.
    function formatLabel(id) {
        var text = root.formatLabels[id]
        return text !== undefined ? text : String(id).toUpperCase()
    }

    function open(rowName, holder) {
        root.name = rowName
        root.format = Convert.defaultFormat(rowName)
        root.strip = false
        root.cursor = root.formats.indexOf(root.format)
        root.focusHolder = holder
        root.opened = true
        keys.forceActiveFocus()
    }

    function close() {
        if (!root.opened)
            return
        root.opened = false
        if (root.focusHolder)
            root.focusHolder.forceActiveFocus()
    }

    function commit() {
        var chosen = root.format
        var stripping = root.strip
        root.close()
        root.accepted(chosen, stripping)
    }

    // A dimmed ground, and a click on it is a cancel, the same shape the network dialog already uses.
    Rectangle {
        anchors.fill: parent
        color: Theme.color.background
        opacity: 0.5

        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            hoverEnabled: true
            onClicked: root.close()
            onWheel: function (wheel) { wheel.accepted = true }
        }
    }

    Rectangle {
        id: card
        anchors.centerIn: parent
        width: Theme.space(root.dialogWidth)
        // Clamped to the window; the body scrolls whatever the clamp cut, see ui/CardScroll.qml.
        height: Math.min(body.wanted + 2 * Theme.spacing.rowPaddingX, root.height - 2 * root.clampMargin)
        color: Theme.color.surface
        border.width: Theme.spacing.hairline
        border.color: Theme.color.muted
        radius: Style.cornerRadius

        // The strip toggle's handler takes a passive grab, so without this sink its press fell
        // through the card to the ground below, which closed the dialog on release.
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onClicked: {}
            onWheel: function (wheel) { wheel.accepted = true }
        }

        Flea.CardScroll {
            id: body
            anchors.fill: parent
            anchors.topMargin: Theme.spacing.rowPaddingX
            anchors.bottomMargin: Theme.spacing.rowPaddingX

        Column {
            width: parent.width
            spacing: 0

            Text {
                id: title
                x: Theme.spacing.rowPaddingX
                width: parent.width - 2 * Theme.spacing.rowPaddingX
                bottomPadding: Theme.spacing.gap
                text: "Convert " + root.name
                color: Theme.color.foreground
                font.family: Theme.font.family
                font.pixelSize: Theme.font.body
                font.bold: true
                textFormat: Text.PlainText
                elide: Text.ElideMiddle
            }

            Rectangle {
                width: parent.width
                height: Theme.spacing.hairline
                color: Theme.color.muted
                opacity: 0.4
            }

            Repeater {
                model: root.formats
                delegate: Flea.MenuRow {
                    required property string modelData
                    required property int index
                    width: body.width
                    entry: ({ label: root.formatLabel(modelData), action: modelData, glyph: "image" })
                    // The pick takes the canvas's accent treatment and the cursor keeps the plain
                    // lift, so the two facts stay separately readable. The pointer moves neither: j and k
                    // step cursor and format together, so hover moving one on its own desyncs them.
                    picked: root.format === modelData
                    current: root.cursor === index
                    onActivated: { root.format = modelData; root.cursor = index; root.commit() }
                }
            }

            Rectangle {
                width: parent.width
                height: Theme.spacing.hairline
                color: Theme.color.muted
                opacity: 0.4
            }

            // Drawn to the cut: a 24-grid square with the check glyph inside it when it is ticked.
            Item {
                width: parent.width
                height: Theme.rowHeight

                Rectangle {
                    id: box
                    anchors.left: parent.left
                    anchors.leftMargin: Theme.spacing.rowPaddingX
                    anchors.verticalCenter: parent.verticalCenter
                    width: Theme.font.caption
                    height: Theme.font.caption
                    color: "transparent"
                    border.width: Theme.spacing.hairline * 2
                    border.color: root.strip ? Theme.color.accent : Theme.color.muted

                    Flea.Glyph {
                        anchors.fill: parent
                        visible: root.strip
                        name: "check"
                        color: Theme.color.accent
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
                    onTapped: root.strip = !root.strip
                }
            }

            Text {
                x: Theme.spacing.rowPaddingX
                width: parent.width - 2 * Theme.spacing.rowPaddingX
                bottomPadding: Theme.spacing.gap
                text: "writes " + Convert.destName(root.name, root.format) + ", never in place"
                color: Theme.color.muted
                font.family: Theme.font.family
                font.pixelSize: Theme.font.body
                textFormat: Text.PlainText
                elide: Text.ElideMiddle
            }

            Row {
                anchors.right: parent.right
                anchors.rightMargin: Theme.spacing.rowPaddingX
                spacing: Theme.spacing.gap

                Flea.DialogButton {
                    label: "Cancel"
                    onActivated: root.close()
                }

                Flea.DialogButton {
                    label: "Convert"
                    primary: true
                    onActivated: root.commit()
                }
            }
        }
        }
    }

    Item {
        id: keys
        anchors.fill: parent
        focus: true

        Keys.onPressed: function (event) {
            if (event.key === Qt.Key_Escape) { root.close(); event.accepted = true; return }
            if (event.key === Qt.Key_Down) {
                root.cursor = Math.min(root.formats.length - 1, root.cursor + 1)
                root.format = root.formats[root.cursor]
                event.accepted = true
                return
            }
            if (event.key === Qt.Key_Up) {
                root.cursor = Math.max(0, root.cursor - 1)
                root.format = root.formats[root.cursor]
                event.accepted = true
                return
            }
            // Space toggles the one checkbox, which is the only other thing this popup asks.
            if (event.key === Qt.Key_Space) { root.strip = !root.strip; event.accepted = true; return }
            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) { root.commit(); event.accepted = true }
        }
    }
}
