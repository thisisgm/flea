import QtQuick
import qs.Commons
import "." as Flea
import "js/Picker.js" as Picker

// The picker's two chrome strips: what was asked for and the two answers on top, where the list is
// standing and what it is narrowed to underneath. SendPicker.html draws both.
Item {
    id: root

    property var picker: null
    property var submissionFocus: null

    function submissionChanged() {
        var window = root.Window.window
        if (root.picker.submitting) {
            var before = window ? window.activeFocusItem : null
            root.submissionFocus = null
            root.picker.stepFocus(null, false)
            root.submissionFocus = {before: before, stepped: window ? window.activeFocusItem : null}
            return
        }
        var pending = root.submissionFocus
        // Replies clear submitting before they answer and before disabled controls re-enable.
        Qt.callLater(function() {
            if (!pending || root.submissionFocus !== pending) return
            root.submissionFocus = null
            if (root.picker.answered || root.picker.submitting) return
            var before = pending.before
            if (before && before.visible && before.enabled && pending.stepped && pending.stepped.activeFocus)
                before.forceActiveFocus()
        })
    }

    Connections {
        target: root.Window.window
        function onActiveFocusItemChanged() {
            if (root.submissionFocus && root.Window.window.activeFocusItem !== root.submissionFocus.stepped)
                root.submissionFocus = null
        }
    }

    signal cancelRequested()
    signal acceptRequested()
    signal backRequested()
    signal upRequested()
    signal chipChosen(int index)
    signal viewChosen(string mode)

    readonly property var req: root.picker.req
    readonly property var chips: Picker.chips(root.req)

    readonly property color edge: root.picker.edge

    implicitHeight: ask.height + where.height

    // The picker's answers are ButtonSystem040 A's one control; the picker owns Tab.
    component Answer: Flea.DialogButton {
        id: answer
        property string name: answer.label
        tabHandle: true
        enabled: answer.available
        activeFocusOnTab: answer.available
        onTabbed: function(from, back) { root.picker.stepFocus(answer, back) }
    }

    // Back, Up and the view marks are Tier A chrome marks, frameless: muted at rest, the keyboard lifts one to the foreground, a lit view stays in it.
    component Mark: Flea.ChromeButton {
        id: mark
        property bool lit: false
        readonly property bool available: mark.enabled
        property string name: mark.accessName
        gesturePolicy: TapHandler.ReleaseWithinBounds
        restingColor: mark.lit ? Theme.color.foreground : Theme.color.muted
        activeFocusOnTab: mark.enabled
        keyboardFocused: mark.activeFocus
        Keys.onTabPressed: function(event) { root.picker.stepFocus(mark, (event.modifiers & Qt.ShiftModifier) !== 0) }
        Keys.onBacktabPressed: root.picker.stepFocus(mark, true)
        Keys.onReturnPressed: if (mark.enabled) mark.activated()
        Keys.onEnterPressed: if (mark.enabled) mark.activated()
        Keys.onSpacePressed: if (mark.enabled) mark.activated()
    }

    // A filter chip (Picker040's dropdown replaces it in 0.3.10): a word in a muted frame, the chosen one an accent frame and wash.
    component Framed: Item {
        id: control

        property string label: ""
        property string name: control.label
        property bool primary: false
        property bool available: true
        opacity: available ? 1 : Theme.disabledOpacity
        enabled: available
        activeFocusOnTab: available
        Keys.onTabPressed: function(event) { root.picker.stepFocus(control, (event.modifiers & Qt.ShiftModifier) !== 0) }
        Keys.onBacktabPressed: root.picker.stepFocus(control, true)
        Keys.onReturnPressed: if (control.available) control.pressed()
        Keys.onEnterPressed: if (control.available) control.pressed()
        Keys.onSpacePressed: if (control.available) control.pressed()

        signal pressed()

        readonly property color ink: !control.available ? Theme.color.muted : Theme.color.foreground

        implicitWidth: caption.implicitWidth + 2 * Theme.spacing.gap
        implicitHeight: Theme.hitMin
        scale: press.pressed && control.available && !Theme.reducedMotion ? 0.96 : 1

        Accessible.role: Accessible.Button
        Accessible.name: control.name
        Accessible.onPressAction: if (control.available) control.pressed()

        Behavior on scale {
            enabled: !Theme.reducedMotion
            NumberAnimation { duration: 150; easing.type: Easing.OutQuad }
        }

        readonly property color frame: control.primary
            ? Theme.color.accentFrame : Theme.color.muted

        // The chosen chip carries its wash at rest; every other earns one under the pointer or the keyboard.
        readonly property real wash: control.primary ? Theme.washActive : !control.available ? 0
            : (control.activeFocus || press.pressed) ? Theme.washActive
            : hover.hovered ? Theme.washHover : 0
        readonly property color washInk: control.primary ? Theme.color.accent : Theme.color.foreground

        Rectangle {
            anchors.fill: parent
            color: Qt.alpha(control.washInk, control.wash)
            border.width: Theme.spacing.hairline
            border.color: control.frame
        }

        Text {
            id: caption
            anchors.centerIn: parent
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

        // Above the strip's rule: the answers' 2 px ring reaches the strip's own edges, and the rule must not cut it.
        Row {
            id: buttons
            z: 1
            anchors.right: parent.right
            anchors.rightMargin: Theme.spacing.rowPaddingX
            anchors.verticalCenter: parent.verticalCenter
            spacing: Theme.spacing.gap

            Answer {
                id: cancelButton
                label: "Cancel"
                onActivated: root.cancelRequested()
            }

            Answer {
                id: acceptButton
                label: Picker.acceptLabel(root.req, root.picker.marks.length)
                primary: true
                available: root.picker.canAccept
                onActivated: root.acceptRequested()
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

            Mark {
                id: backButton
                glyph: "arrow-left"
                enabled: !root.picker.backendUnavailable && root.picker.history.length > 0 && !root.picker.submitting
                onActivated: root.backRequested()
            }

            Mark {
                id: upButton
                glyph: "arrow-up"
                accessName: root.picker.recent ? "Parent folder unavailable in Recent" : "Parent folder"
                // The board's own rule, drawn as its disabled Up: a history has no directory above it.
                enabled: !root.picker.backendUnavailable && !root.picker.submitting && !root.picker.recent && Picker.parentOf(root.picker.path) !== root.picker.path
                onActivated: root.upRequested()
            }
        }

        // The path slot: the location the strip stands in, and, on Ctrl+L or ":", the field that
        // types a new one. It fills the room between the nav marks and the filter chips; its own
        // internals live in ui/PickerLocationField.qml so this strip stays under its hard cap.
        Flea.PickerLocationField {
            anchors.left: moves.right
            anchors.leftMargin: Theme.spacing.gap
            anchors.right: types.left
            anchors.rightMargin: Theme.spacing.gap
            anchors.verticalCenter: parent.verticalCenter
            height: Theme.hitMin
            picker: root.picker
        }

        // The caller's filters, and All files beside them; a request with no filters draws no chips.
        // The view marks keep the far right, so the chips give way through their anchor.
        Flickable {
            id: types
            anchors.right: parent.right
            anchors.rightMargin: views.width + Theme.spacing.rowPaddingX + Theme.spacing.gap
            anchors.verticalCenter: parent.verticalCenter
            // The chips take room first and the path gives way through its anchor, keeping its minimum.
            width: Picker.chipStripWidth(where.width - moves.width - views.width - 2 * Theme.spacing.rowPaddingX - 3 * Theme.spacing.gap, chipRow.width, Picker.CHIP_PATH_MIN)
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
                        // The chosen chip is the picker's own primary: a foreground label in an accent frame.
                        onPressed: root.chipChosen(modelData.index)
                    }
                }
            }
        }
    }

    // Pointer and ctrl-1/ctrl-3 targets only; callout 12 keeps them out of the Tab walk.
    Row {
        id: views
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: where.verticalCenter
        spacing: Theme.spacing.gap

        Mark {
            id: listButton
            glyph: "list"
            lit: root.picker.viewMode === "list"
            activeFocusOnTab: false
            onActivated: root.viewChosen("list")
        }

        Mark {
            id: gridButton
            glyph: "grid"
            lit: root.picker.viewMode === "grid"
            activeFocusOnTab: false
            onActivated: root.viewChosen("grid")
        }
    }

    function focusItems() {
        var items = [cancelButton, acceptButton, backButton, upButton]
        for (var i = 0; i < chipRepeater.count; i++) items.push(chipRepeater.itemAt(i))
        return items
    }
    function controls() {
        return root.focusItems().concat([listButton, gridButton]).map(function(item) { return root.picker.control(item.name, item, item.available) })
    }
}
