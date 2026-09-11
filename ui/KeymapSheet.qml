import QtQuick
import qs.Commons
import "." as Flea
import "js/Keymap.js" as Keymap

// The keymap sheet ? opens, drawn as the Keys panel on Operations.dc.html draws it. Every row comes
// from keys.toml through Keymap.SHEET, so a key that loses its binding cannot go on being advertised.
Item {
    id: root

    property bool opened: false
    property Item focusHolder: null
    readonly property var sheet: Keymap.sheetFor(ViewState.keysPreset, "gui", root.focusHolder ? root.focusHolder.dualMode : false)

    // The canvas drew this panel at 300, the convert popup's width, beside four illustrative rows.
    // The real sheet is sixty rows whose chords run to eighteen characters: at 300 a cap took the
    // whole half-cell, the wording beside it elided to a single ellipsis, and the two columns
    // overprinted each other. 480 is the Open with card's own anchor and leaves both room.
    readonly property int sheetWidth: 480
    readonly property int clampMargin: 8
    // var, not Item: BorderSurface is a qs.Ui type qmllint cannot resolve, and Item would read as incompatible.
    readonly property var cardItem: card
    // A cap is sized from the type scale, never from the text inside it, so every cap is one height.
    readonly property int capSize: Theme.markSize

    // One cap column for the whole sheet, measured off the widest chord this preset spells. Sizing
    // each cap to its own text left every wording starting on a different x, and a wide chord took
    // the cell whole and drew across the column beside it.
    readonly property string widestCap: {
        var out = ""
        for (var i = 0; i < root.sheet.length; i++)
            if (root.sheet[i].keys.length > out.length) out = root.sheet[i].keys
        return out
    }
    readonly property int capWidth: Math.max(root.capSize, Math.ceil(capMetrics.width) + Theme.spacing.gap)
    // The narrowest wording worth drawing beside a cap. A preset whose chords are wide enough to
    // leave less than this takes one column and scrolls, rather than two columns of elided stubs.
    readonly property int cellFloor: root.capWidth + Theme.spacing.gap + Math.ceil(labelFloor.width)
    // Two columns is what the canvas draws, and what keeps the whole map on one panel where it fits.
    readonly property int columns: body.width >= 2 * root.cellFloor + Theme.spacing.rowPaddingX ? 2 : 1

    TextMetrics {
        id: capMetrics
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        text: root.widestCap
    }

    TextMetrics {
        id: labelFloor
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        text: "extend down"
    }
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
        for (var i = 0; i < root.sheet.length; i++)
            out.push(root.sheet[i].keys + " " + root.sheet[i].label)
        return out.join("\n")
    }

    // A dimmed ground, and a click on it closes, the same shape ui/ConvertDialog.qml uses.
    Rectangle {
        anchors.fill: parent
        color: Theme.color.background
        opacity: root.groundOpacity

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
        width: Math.max(0, Math.min(Theme.space(root.sheetWidth) * Theme.dialogWidthRatio, root.width - 2 * root.clampMargin))
        // Clamped to the window; the body scrolls whatever the clamp cut, see ui/CardScroll.qml.
        height: Math.min(body.wanted + 2 * Theme.spacing.rowPaddingX, root.height - 2 * root.clampMargin)
        color: Theme.color.surface
        border.width: Theme.spacing.hairline
        border.color: Theme.color.muted
        // Mirrors hyprland decoration:rounding, same as ui/ConvertDialog.qml; 0 on a stock box stays square.
        radius: Style.cornerRadius

        Flea.CardScroll {
            id: body
            anchors.fill: parent
            anchors.margins: Theme.spacing.rowPaddingX

        Column {
            width: parent.width
            spacing: Theme.spacing.gap

            Text {
                text: "Keys"
                color: Theme.color.foreground
                font.family: Theme.font.family
                font.pixelSize: Theme.font.body
                font.bold: true
                textFormat: Text.PlainText
            }

            Grid {
                columns: root.columns
                rowSpacing: Theme.spacing.rowPaddingY
                columnSpacing: Theme.spacing.rowPaddingX

                Repeater {
                    model: root.sheet

                    delegate: Item {
                        id: entry
                        required property var modelData
                        // Equal halves, so the second column starts on one x the whole way down.
                        width: (body.width - (root.columns - 1) * Theme.spacing.rowPaddingX) / root.columns
                        height: root.capSize
                        // Nothing this cell draws may reach the cell beside it, whatever it holds.
                        clip: true

                        Rectangle {
                            id: capBox
                            anchors.left: parent.left
                            anchors.verticalCenter: parent.verticalCenter
                            // The sheet's own cap column, never this row's text: see root.capWidth.
                            width: root.capWidth
                            height: root.capSize
                            color: "transparent"
                            border.width: Theme.spacing.hairline
                            border.color: Theme.color.muted

                            Text {
                                id: cap
                                anchors.fill: parent
                                anchors.margins: Theme.spacing.hairline
                                horizontalAlignment: Text.AlignHCenter
                                verticalAlignment: Text.AlignVCenter
                                text: entry.modelData.keys
                                color: Theme.color.foreground
                                font.family: Theme.font.family
                                font.pixelSize: Theme.font.caption
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
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
