import QtQuick
import QtQuick.Layouts
import "." as Flea
import "js/Format.js" as Format
import "js/Picker.js" as Picker

// Collision approval reviews a URI; the requesting application alone writes the chosen file.
// Folder creation is a separate, explicit picker operation.
Item {
    id: root
    required property var picker
    signal nameEdited(string text)
    signal accepted()
    readonly property alias fieldItem: field
    readonly property alias scrollItem: body
    readonly property alias uriItem: outputUri
    readonly property alias statusItem: statusLine
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
        bleed: Theme.focusClearance
        anchors.fill: parent
        anchors.margins: Theme.spacing.rowPaddingX - Theme.focusClearance
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
                    // Muted at rest, accent on focus: the frame DialogField, MenuActionDialog, OpenWithDialog and PermissionsDialog draw.
                    border.color: field.activeFocus ? Theme.color.accent : Theme.color.mutedFrame
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
                    text: "Saves to"
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
                    Accessible.name: "Saves to"
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
                id: statusLine
                width: parent.width
                visible: text.length > 0
                text: root.refused ? Format.fill(Picker.SAVE_REFUSED, root.askedName, Picker.NAME_REFUSED)
                    : root.picker.saveError || (root.picker.saveCollision ? Format.fill(Picker.SAVE_EXISTS, Picker.jointName(root.picker.saveName)) : "")
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                color: Theme.color.errorOnSurface
                font { family: Theme.font.family; pixelSize: Theme.font.caption }
            }
            Row {
                anchors.right: parent.right
                visible: root.picker.saveCollision
                spacing: Theme.spacing.gap
                Flea.DialogButton {
                    id: cancelButton
                    label: "Cancel"
                    tabStop: true
                    enabled: !root.picker.submitting
                    tabHandle: true
                    onTabbed: function(from, back) { root.picker.stepFocus(from, back) }
                    onActivated: root.picker.cancel()
                }
                Flea.DialogButton {
                    id: useButton
                    label: "Use this location"
                    destructive: true
                    tabStop: true
                    enabled: !root.picker.submitting
                    tabHandle: true
                    onTabbed: function(from, back) { root.picker.stepFocus(from, back) }
                    onActivated: root.picker.accept(true)
                }
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
    function focusCancel() { cancelButton.forceActiveFocus(Qt.TabFocusReason) }
    function focusItems() { return root.visible ? [field, outputUri].concat(root.picker.saveCollision ? [cancelButton, useButton] : []) : [] }
    function controls() {
        return root.focusItems().map(function(item) {
            return root.picker.control(item === field ? "Filename" : item === outputUri ? "Saves to" : item.label, item, item.enabled)
        })
    }
}
