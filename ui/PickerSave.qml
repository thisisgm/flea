import QtQuick
import QtQuick.Layouts
import "." as Flea
import "js/Format.js" as Format
import "js/Picker.js" as Picker

// Collision approval reviews a URI; the requesting application remains the only writer.
Item {
    id: root
    required property var picker
    signal nameEdited(string text)
    signal accepted()
    readonly property alias fieldItem: field
    readonly property alias scrollItem: body
    readonly property alias uriItem: outputUri
    readonly property string outPath: Picker.join(root.picker.path, root.picker.saveName)
    readonly property string uri: Format.fileUri(root.outPath)
    readonly property string askedName: root.picker.saveName || root.picker.req.name
    readonly property bool refused: root.askedName.length > 0 && !Picker.validName(root.askedName)
    visible: root.picker.saving
    property real maximumHeight: parent.height
    implicitHeight: visible ? Math.min(maximumHeight, body.wanted + 2 * Theme.spacing.rowPaddingX) : 0

    Rectangle { anchors.fill: parent; color: Theme.color.surface }
    Flea.CardScroll {
        id: body
        anchors.fill: parent
        anchors.margins: Theme.spacing.rowPaddingX
        Column {
            id: column
            width: parent.width
            spacing: Theme.spacing.gap

            GridLayout {
                width: parent.width
                columns: 2
                columnSpacing: Theme.spacing.gap
                rowSpacing: Theme.spacing.hairline * 4
                Text {
                    text: "Filename"
                    textFormat: Text.PlainText
                    color: Theme.color.foreground
                    font { family: Theme.font.family; pixelSize: Theme.font.caption }
                }
                Rectangle {
                    Layout.fillWidth: true
                    implicitHeight: Math.max(Theme.hitMin, field.implicitHeight + 2 * Theme.spacing.rowPaddingY)
                    color: Theme.color.background
                    border.width: Theme.spacing.hairline
                    // Muted at rest, accent on focus: what DialogField, MenuActionDialog, OpenWithDialog
                    // and PermissionsDialog all draw. This was the last control in the product still
                    // framed in the divider's own ink.
                    border.color: field.activeFocus ? Theme.color.accent : Theme.color.muted
                    TextInput {
                        id: field
                        anchors.fill: parent
                        anchors.leftMargin: Theme.spacing.gap
                        anchors.rightMargin: Theme.spacing.gap
                        verticalAlignment: TextInput.AlignVCenter
                        text: root.picker.saveName
                        color: Theme.color.foreground
                        selectionColor: Theme.color.accent
                        selectedTextColor: Theme.color.background
                        font { family: Theme.font.family; pixelSize: Theme.font.body }
                        clip: true
                        activeFocusOnTab: true
                        enabled: !root.picker.submitting
                        Accessible.name: "Filename"
                        onTextEdited: root.nameEdited(text)
                        onAccepted: root.accepted()
                        Keys.onTabPressed: function(event) { root.picker.stepFocus(field, (event.modifiers & Qt.ShiftModifier) !== 0) }
                        Keys.onBacktabPressed: root.picker.stepFocus(field, true)
                    }
                }
                Text {
                    visible: !root.refused
                    text: "Output URI"
                    textFormat: Text.PlainText
                    color: Theme.color.foreground
                    font { family: Theme.font.family; pixelSize: Theme.font.caption }
                }
                Flickable {
                    id: outputUri
                    Layout.fillWidth: true
                    visible: !root.refused
                    implicitHeight: uriText.implicitHeight
                    contentWidth: uriText.implicitWidth
                    contentHeight: height
                    flickableDirection: Flickable.HorizontalFlick
                    boundsBehavior: Flickable.StopAtBounds
                    clip: true
                    activeFocusOnTab: true
                    Accessible.role: Accessible.StaticText
                    Accessible.name: "Output URI"
                    Accessible.description: uriText.text
                    Keys.onPressed: function(event) {
                        if (event.key === Qt.Key_Home) contentX = 0
                        else if (event.key === Qt.Key_End) contentX = Math.max(0, contentWidth - width)
                        else { event.accepted = false; return }
                        event.accepted = true
                    }
                    Keys.onLeftPressed: contentX = Math.max(0, contentX - Theme.font.caption)
                    Keys.onRightPressed: contentX = Math.min(Math.max(0, contentWidth - width), contentX + Theme.font.caption)
                    Keys.onTabPressed: function(event) { root.picker.stepFocus(outputUri, (event.modifiers & Qt.ShiftModifier) !== 0) }
                    Keys.onBacktabPressed: root.picker.stepFocus(outputUri, true)
                    Flea.FastScrollHandler { flickable: outputUri }
                    TapHandler { onTapped: outputUri.forceActiveFocus(Qt.MouseFocusReason) }
                    Text {
                        id: uriText
                        text: root.uri
                        textFormat: Text.PlainText
                        color: outputUri.activeFocus ? Theme.color.accent : Theme.color.foreground
                        font { family: Theme.font.family; pixelSize: Theme.font.caption }
                        onTextChanged: outputUri.contentX = 0
                    }
                }
            }
            Text {
                width: parent.width
                visible: text.length > 0
                text: root.refused ? "Refused · " + root.askedName + " · " + Picker.NAME_REFUSED
                    : root.picker.saveError || (root.picker.saveCollision ? root.picker.saveName + " already exists here · review before continuing" : "")
                textFormat: Text.PlainText
                wrapMode: Text.WrapAnywhere
                color: Theme.color.error
                font { family: Theme.font.family; pixelSize: Theme.font.caption }
            }
            Row {
                anchors.right: parent.right
                visible: root.picker.saveCollision
                spacing: Theme.spacing.gap
                ReviewButton { id: cancelButton; label: "Cancel"; onPressed: root.picker.cancel() }
                ReviewButton { id: useButton; label: "Use this location"; danger: true; onPressed: root.picker.accept(true) }
            }
            Text {
                width: parent.width
                visible: root.picker.saveCollision
                text: "This confirms the shown name and folder. Cancel leaves the existing file untouched."
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                color: Theme.color.foreground
                font { family: Theme.font.family; pixelSize: Theme.font.caption }
            }
        }
    }
    component ReviewButton: FocusScope {
        id: control
        required property string label
        property bool danger: false
        signal pressed()
        width: Math.max(Theme.hitMin, caption.implicitWidth + 2 * Theme.spacing.gap)
        height: Math.max(Theme.hitMin, caption.implicitHeight + Theme.spacing.gap)
        activeFocusOnTab: true
        enabled: !root.picker.submitting
        Accessible.role: Accessible.Button
        Accessible.name: label
        Accessible.onPressAction: if (enabled) pressed()
        Keys.onReturnPressed: control.pressed()
        Keys.onEnterPressed: control.pressed()
        Keys.onSpacePressed: control.pressed()
        Keys.onTabPressed: function(event) { root.picker.stepFocus(control, (event.modifiers & Qt.ShiftModifier) !== 0) }
        Keys.onBacktabPressed: root.picker.stepFocus(control, true)
        // GM's 2026-09-11 ruling, the same one ui/PickerChrome.qml's controls carry: the frame is the
        // control's own role and never the divider's ink, and the wash inside it is the state.
        readonly property color ink: control.danger ? Theme.color.error : Theme.color.foreground
        readonly property color frame: control.danger ? Theme.color.error : Theme.color.muted
        readonly property real wash: (control.activeFocus || collisionPress.pressed) ? Theme.washActive
            : collisionHover.hovered ? Theme.washHover : 0

        Rectangle {
            anchors.fill: parent
            color: Qt.alpha(control.ink, control.wash)
            border.width: Theme.spacing.hairline
            border.color: control.frame
        }
        Text {
            id: caption
            anchors.centerIn: parent
            text: control.label
            textFormat: Text.PlainText
            color: control.ink
            font { family: Theme.font.family; pixelSize: Theme.font.caption }
        }
        HoverHandler { id: collisionHover; cursorShape: Qt.PointingHandCursor }
        TapHandler {
            id: collisionPress
            onTapped: { control.forceActiveFocus(Qt.MouseFocusReason); control.pressed() }
        }
    }
    function focusCancel() { cancelButton.forceActiveFocus(Qt.TabFocusReason) }
    function focusItems() { return root.visible ? [field, outputUri].concat(root.picker.saveCollision ? [cancelButton, useButton] : []) : [] }
    function controls() {
        return root.focusItems().map(function(item) {
            return root.picker.control(item === field ? "Filename" : item === outputUri ? "Output URI" : item.label, item, item.enabled)
        })
    }
}
