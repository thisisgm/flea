import QtQuick
import qs.Commons
import "." as Flea
import "js/Format.js" as Format
import "js/Picker.js" as Picker

// The picker's two chrome strips: what was asked for and the two answers on top, where the list is
// standing and what it is narrowed to underneath. SendPicker.html draws both.
Item {
    id: root

    property var picker: null

    signal cancelRequested()
    signal acceptRequested()
    signal backRequested()
    signal upRequested()
    signal chipChosen(int index)

    readonly property var req: root.picker.req
    readonly property var chips: Picker.chips(root.req)

    readonly property color edge: root.picker.edge

    implicitHeight: ask.height + where.height

    // The board's framed chrome control: a hairline square around a mark, or a hairline box around
    // a word. Only the accent frame, the recessed ground and which of the two it holds ever differ.
    component Framed: Item {
        id: control

        property string glyph: ""
        property string label: ""
        property string name: control.label
        property bool primary: false
        property bool recessed: false
        property bool available: true

        signal pressed()

        readonly property color ink: !control.available ? Theme.color.muted
            : control.primary ? Theme.color.accent : Theme.color.foreground

        implicitWidth: control.glyph.length > 0 ? Theme.hitMin : caption.implicitWidth + 2 * Theme.spacing.gap
        implicitHeight: Theme.hitMin
        scale: press.pressed && control.available && !Theme.reducedMotion ? 0.96 : 1

        Accessible.role: Accessible.Button
        Accessible.name: control.name
        Accessible.onPressAction: if (control.available) control.pressed()

        Behavior on scale {
            enabled: !Theme.reducedMotion
            NumberAnimation { duration: 150; easing.type: Easing.OutQuad }
        }

        Rectangle {
            anchors.fill: parent
            color: control.recessed ? Theme.color.surface : "transparent"
            border.width: Theme.spacing.hairline
            border.color: control.primary ? Theme.color.accent : root.edge
        }

        Flea.Glyph {
            anchors.centerIn: parent
            visible: control.glyph.length > 0
            width: Theme.chromeMarkSize
            height: Theme.chromeMarkSize
            name: control.glyph
            color: control.ink
        }

        Text {
            id: caption
            anchors.centerIn: parent
            visible: control.glyph.length === 0
            text: control.label
            color: control.ink
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            textFormat: Text.PlainText
        }

        HoverHandler { cursorShape: Qt.PointingHandCursor }

        TapHandler {
            id: press
            acceptedButtons: Qt.LeftButton
            onTapped: if (control.available) control.pressed()
        }
    }

    Item {
        id: ask
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: Math.max(Theme.chromeHeight, titles.implicitHeight + 2 * Theme.spacing.rowPaddingY)

        Rectangle {
            anchors.fill: parent
            color: Theme.color.surface
        }

        Column {
            id: titles
            anchors.left: parent.left
            anchors.leftMargin: Theme.spacing.rowPaddingX
            anchors.right: buttons.left
            anchors.rightMargin: Theme.spacing.gap
            anchors.verticalCenter: parent.verticalCenter

            // The caller's own words, and a filename is arbitrary text, so PlainText here too.
            Text {
                width: parent.width
                text: Picker.title(root.req)
                color: Theme.color.foreground
                font.family: Theme.font.family
                font.pixelSize: Theme.font.bodySmall
                elide: Text.ElideRight
                textFormat: Text.PlainText
            }

            // Only a portal identity is drawn, so an application the desktop cannot name gets no line.
            Text {
                width: parent.width
                visible: text.length > 0
                text: Picker.subtitle(root.req)
                color: Theme.color.foreground
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                elide: Text.ElideRight
                textFormat: Text.PlainText
            }
        }

        Row {
            id: buttons
            anchors.right: parent.right
            anchors.rightMargin: Theme.spacing.rowPaddingX
            anchors.verticalCenter: parent.verticalCenter
            spacing: Theme.spacing.gap

            // The board's chrome button is its hit box plus the hairline frame around it, 26 at base-size 14.
            Framed {
                height: Theme.hitMin + 2 * Theme.spacing.hairline
                label: "Cancel"
                onPressed: root.cancelRequested()
            }

            Framed {
                height: Theme.hitMin + 2 * Theme.spacing.hairline
                label: Picker.acceptLabel(root.req, root.picker.marks.length)
                primary: true
                onPressed: root.acceptRequested()
            }
        }

        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: Theme.spacing.hairline
            color: root.edge
        }
    }

    Item {
        id: where
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: ask.bottom
        // The strip carries 24 px hit boxes, so it takes a list row's height less the two hairline
        // rules that bracket it: 35 at base-size 14, which is the board's own nav strip.
        height: Theme.rowHeight - 2 * Theme.spacing.hairline

        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: Theme.spacing.hairline
            color: root.edge
        }

        Row {
            id: moves
            anchors.left: parent.left
            anchors.leftMargin: Theme.spacing.rowPaddingX
            anchors.verticalCenter: parent.verticalCenter
            spacing: Theme.spacing.gap

            Framed {
                glyph: "arrow-left"
                name: "Back"
                available: root.picker.history.length > 0
                onPressed: root.backRequested()
            }

            Framed {
                glyph: "arrow-up"
                name: root.picker.recent ? "Parent folder unavailable in Recent" : "Parent folder"
                // The board's own rule, drawn as its disabled Up: a history has no directory above it.
                available: !root.picker.recent && Picker.parentOf(root.picker.path) !== root.picker.path
                onPressed: root.upRequested()
            }
        }

        Text {
            anchors.left: moves.right
            anchors.leftMargin: Theme.spacing.gap
            anchors.right: types.left
            anchors.rightMargin: Theme.spacing.gap
            anchors.verticalCenter: parent.verticalCenter
            // Recent is a location and not a path, so the strip says the location's own name; a
            // tilde form of the token would be a path the window is not standing in.
            text: root.picker.recent ? Picker.RECENT_LABEL : Format.tilde(root.picker.path, root.picker.home)
            color: Theme.color.foreground
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            elide: Text.ElideLeft
            textFormat: Text.PlainText
        }

        // The caller's filters, and All files beside them; a request with no filters draws no chips.
        Row {
            id: types
            anchors.right: parent.right
            anchors.rightMargin: Theme.spacing.rowPaddingX
            anchors.verticalCenter: parent.verticalCenter
            spacing: Theme.spacing.hairline * 4

            Repeater {
                model: root.chips

                Framed {
                    required property var modelData
                    label: modelData.label
                    primary: modelData.index === root.picker.filterIndex
                    // The board sets the active chip on the recessed plane, so it reads as pressed in.
                    recessed: modelData.index === root.picker.filterIndex
                    onPressed: root.chipChosen(modelData.index)
                }
            }
        }
    }
}
