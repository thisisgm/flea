import QtQuick
import qs.Commons
import "js/Keymap.js" as Keymap

// The keymap sheet ? opens, drawn as the Keys panel on Operations.dc.html draws it. Every row comes
// from keys.toml through Keymap.SHEET, so a key that loses its binding cannot go on being advertised.
Item {
    id: root

    property bool opened: false
    property Item focusHolder: null

    // The canvas draws this panel at 300 design pixels wide, the same as the convert popup.
    readonly property int sheetWidth: 300
    // Two columns, which is what the canvas draws and what keeps the whole map on one panel.
    readonly property int columns: 2
    // A cap is sized from the type scale, never from the text inside it, so every cap is one height.
    readonly property int capSize: Theme.markSize
    readonly property real groundOpacity: 0.5

    anchors.fill: parent
    visible: root.opened
    z: 2

    function open(holder) {
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

    // What a test reads instead of running OCR over the panel, the same idiom ui/Pane.qml's
    // menuEntries() uses: one row per line, the cap and the wording it is drawn beside.
    function rows() {
        var out = []
        for (var i = 0; i < Keymap.SHEET.length; i++)
            out.push(Keymap.SHEET[i].keys + " " + Keymap.SHEET[i].label)
        return out.join("\n")
    }

    // A dimmed ground, and a click on it closes, the same shape ui/ConvertDialog.qml uses.
    Rectangle {
        anchors.fill: parent
        color: Theme.color.background
        opacity: root.groundOpacity

        MouseArea {
            anchors.fill: parent
            onClicked: root.close()
        }
    }

    Rectangle {
        id: card
        anchors.centerIn: parent
        width: Theme.space(root.sheetWidth)
        height: body.implicitHeight + 2 * Theme.spacing.rowPaddingX
        color: Theme.color.surface
        border.width: Theme.spacing.hairline
        border.color: Theme.color.muted
        // Mirrors hyprland decoration:rounding, same as ui/ConvertDialog.qml; 0 on a stock box stays square.
        radius: Style.cornerRadius

        Column {
            id: body
            x: Theme.spacing.rowPaddingX
            y: Theme.spacing.rowPaddingX
            width: parent.width - 2 * Theme.spacing.rowPaddingX
            spacing: Theme.spacing.gap

            Text {
                text: "Keys"
                color: Theme.color.foreground
                font.family: Theme.font.family
                font.pixelSize: Theme.font.bodySmall
                font.bold: true
                textFormat: Text.PlainText
            }

            Grid {
                columns: root.columns
                rowSpacing: Theme.spacing.rowPaddingY
                columnSpacing: Theme.spacing.rowPaddingX

                Repeater {
                    model: Keymap.SHEET

                    delegate: Item {
                        id: entry
                        required property var modelData
                        // Equal halves, so the second column starts on one x the whole way down.
                        width: (body.width - Theme.spacing.rowPaddingX) / root.columns
                        height: root.capSize

                        Rectangle {
                            id: capBox
                            anchors.left: parent.left
                            anchors.verticalCenter: parent.verticalCenter
                            // A two-key cap like "j k" is wider than one square, never taller.
                            width: Math.max(root.capSize, cap.implicitWidth + Theme.spacing.gap)
                            height: root.capSize
                            color: "transparent"
                            border.width: Theme.spacing.hairline
                            border.color: Theme.color.muted

                            Text {
                                id: cap
                                anchors.centerIn: parent
                                text: entry.modelData.keys
                                color: Theme.color.foreground
                                font.family: Theme.font.family
                                font.pixelSize: Theme.font.caption
                                textFormat: Text.PlainText
                            }
                        }

                        Text {
                            anchors.left: capBox.right
                            anchors.leftMargin: Theme.spacing.gap
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            text: entry.modelData.label
                            color: Theme.color.muted
                            font.family: Theme.font.family
                            font.pixelSize: Theme.font.caption
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                        }
                    }
                }
            }

            // The canvas's own footer. One table, keys.toml, so the TUI's map cannot drift from this one.
            Text {
                width: parent.width
                text: "esc closes, ^ is ctrl, no TUI yet, generated by flea-keymap-gen"
                color: Theme.color.muted
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }
        }
    }

    Item {
        id: keys
        anchors.fill: parent
        focus: true

        // Any key closes it: the sheet is a reference and not a mode, and ? is how it comes back.
        Keys.onPressed: function (event) {
            root.close()
            event.accepted = true
        }
    }
}
