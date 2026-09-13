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

    // The picker's chrome control: a mark, or a word. GM's 2026-09-11 ruling moved its frame off the
    // divider's ink, which ui/picker.qml's own comment says the board drew both in: a frame and a
    // rule in one ink are one line, and the controls dissolved into the chrome as the scale dropped.
    // The frame carries the role, muted or accent, and the wash inside it carries the state.
    component Framed: Item {
        id: control

        property string glyph: ""
        property string label: ""
        property string name: control.label
        property bool primary: false
        property bool available: true
        enabled: available
        activeFocusOnTab: available
        Keys.onTabPressed: function(event) { root.picker.stepFocus(control, (event.modifiers & Qt.ShiftModifier) !== 0) }
        Keys.onBacktabPressed: root.picker.stepFocus(control, true)
        Keys.onReturnPressed: if (control.available) control.pressed()
        Keys.onEnterPressed: if (control.available) control.pressed()
        Keys.onSpacePressed: if (control.available) control.pressed()

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

        // Muted is the resting frame of a neutral control, the role ThemeRoles.html gives an inactive
        // one, and an unavailable control stays there: a frame may recede only when the control is
        // inert. ui/DialogButton.qml has drawn its own frames this way all along.
        readonly property color frame: control.available && control.primary
            ? Theme.color.accent : Theme.color.muted

        // The primary control carries its wash at rest, because it is the one action the request is
        // asking for; every other control earns one under the pointer or the keyboard.
        readonly property real wash: !control.available ? 0
            : (control.activeFocus || press.pressed) ? Theme.washActive
            : hover.hovered ? Theme.washHover
            : control.primary ? Theme.washActive : 0

        Rectangle {
            anchors.fill: parent
            color: Qt.alpha(control.ink, control.wash)
            border.width: Theme.spacing.hairline
            border.color: control.frame
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

        HoverHandler {
            id: hover
            cursorShape: control.available ? Qt.PointingHandCursor : Qt.ArrowCursor
        }

        TapHandler {
            id: press
            acceptedButtons: Qt.LeftButton
            onTapped: if (control.available) { control.forceActiveFocus(Qt.MouseFocusReason); control.pressed() }
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
                font.pixelSize: Theme.font.body
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
                id: cancelButton
                height: Theme.hitMin + 2 * Theme.spacing.hairline
                label: "Cancel"
                onPressed: root.cancelRequested()
            }

            Framed {
                id: acceptButton
                height: Theme.hitMin + 2 * Theme.spacing.hairline
                label: Picker.acceptLabel(root.req, root.picker.marks.length)
                primary: true
                available: root.picker.canAccept
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
                id: backButton
                glyph: "arrow-left"
                name: "Back"
                available: !root.picker.backendUnavailable && root.picker.history.length > 0 && !root.picker.submitting
                onPressed: root.backRequested()
            }

            Framed {
                id: upButton
                glyph: "arrow-up"
                name: root.picker.recent ? "Parent folder unavailable in Recent" : "Parent folder"
                // The board's own rule, drawn as its disabled Up: a history has no directory above it.
                available: !root.picker.backendUnavailable && !root.picker.submitting && !root.picker.recent && Picker.parentOf(root.picker.path) !== root.picker.path
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
        Flickable {
            id: types
            anchors.right: parent.right
            anchors.rightMargin: Theme.spacing.rowPaddingX
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(chipRow.width, Math.max(0, (where.width - moves.width - 3 * Theme.spacing.rowPaddingX) / 2))
            height: Theme.hitMin
            contentWidth: chipRow.width
            contentHeight: height
            boundsBehavior: Flickable.StopAtBounds
            flickableDirection: Flickable.HorizontalFlick
            clip: true
            Flea.FastScrollHandler { flickable: types }

            function reveal(item) {
                if (item.x < contentX) contentX = item.x
                else if (item.x + item.width > contentX + width) contentX = item.x + item.width - width
            }

            Row {
                id: chipRow
                spacing: Theme.spacing.hairline * 4

                Repeater {
                    id: chipRepeater
                    model: root.chips

                    Framed {
                        required property var modelData
                        label: modelData.label
                        primary: modelData.index === root.picker.filterIndex
                        available: !root.picker.backendUnavailable && !root.picker.submitting
                        onActiveFocusChanged: if (activeFocus) types.reveal(this)
                        // The chosen chip is accent ink over the accent wash, which makes it the same
                        // control the Settings segmented chooser already draws; its recessed plane
                        // went with the frames.
                        onPressed: root.chipChosen(modelData.index)
                    }
                }
            }
        }
    }

    function focusItems() {
        var items = [cancelButton, acceptButton, backButton, upButton]
        for (var i = 0; i < chipRepeater.count; i++) items.push(chipRepeater.itemAt(i))
        return items
    }
    function controls() {
        return root.focusItems().map(function(item) { return root.picker.control(item.name, item, item.available) })
    }
}
