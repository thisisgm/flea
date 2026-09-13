import QtQuick
import qs.Commons
import "." as Flea
import "js/Permissions.js" as Permissions

// One item, one mode; the backend owns its reviewed descriptor for this dialog's lifetime.
FocusScope {
    id: root
    anchors.fill: parent
    visible: opened
    property bool opened: false
    property int requestId: 0
    property var facts: ({})
    property string path: ""
    property string modeText: ""
    property string errorText: ""
    property bool busy: false
    property bool transportFailed: false
    property Item focusHolder: null
    readonly property bool editable: facts.ok === true && !facts.reason && !busy && !transportFailed
    readonly property int modeValue: Permissions.parse(modeText)
    readonly property bool applying: busy && facts.ok === true
    readonly property real labelWidth: Math.round(96 * Theme.font.bodySmall / 13)
    readonly property int controlHeight: Math.max(Theme.rowHeight, Math.ceil(Theme.font.body * Theme.lineBoxRatio) + 2 * Theme.spacing.rowPaddingY)
    readonly property real bodyInset: 16 * Theme.font.bodySmall / 13 + Theme.spacing.hairline
    readonly property int headingHeight: Math.round(26 * Theme.font.bodySmall / 13)
    readonly property var cardItem: card
    readonly property var bodyItem: body
    readonly property string displayedError: errorLabel.text
    readonly property string displayedSummary: changeSummary.text + "\n" + scopeLabel.text
    function controls() {
        var result = [{name: "Close", item: closeMark, enabled: !applying}, {name: "Octal", item: octal, enabled: editable},
            {name: "Cancel", item: cancelFocus, enabled: !applying}, {name: "Apply", item: applyFocus, enabled: editable && modeValue >= 0}]
        for (var row = 0; row < permissionRows.count; row++) {
            var group = permissionRows.itemAt(row)
            for (var column = 0; column < group.checks.count; column++) {
                var checkbox = group.checks.itemAt(column)
                result.push({name: checkbox.Accessible.name, item: checkbox, checked: checkbox.checked, bit: checkbox.bit, enabled: editable})
            }
        }
        return result
    }
    readonly property string scopeText: facts.directory
        ? "Scope this directory only · enclosed items unchanged · ownership unchanged"
        : "Scope this item only · ownership unchanged"
    signal requested(var message)
    signal changed()
    signal closed()

    function open(itemPath, holder) {
        requestId += 1
        path = itemPath
        focusHolder = holder
        facts = ({})
        modeText = ""
        errorText = ""
        transportFailed = false
        busy = true
        opened = true
        body.contentY = 0
        cancelFocus.forceActiveFocus()
        requested({ c: "permissions", op: "inspect", id: requestId, path: path })
    }
    function receive(message) {
        if (!opened || transportFailed || message.id !== requestId || message.op === "close") return
        busy = false
        cancelFocus.forceActiveFocus()
        if (!message.ok) { errorText = message.error || "Could not change permissions."; return }
        if (message.op === "apply") { changed(); close(); return }
        facts = message
        modeText = message.mode
    }
    function backendFailed(message) {
        if (!opened || transportFailed) return
        var applying = busy && facts.ok === true
        transportFailed = true
        busy = false
        cancelFocus.forceActiveFocus()
        var reason = message || "the backend stopped"
        errorText = applying
            ? "Permission change outcome is unknown because " + reason + "; restart Flea and reopen Permissions to check the current mode."
            : "Permissions is unavailable because " + reason + "; restart Flea and reopen Permissions."
    }
    function close() {
        // An issued fchmod cannot be cancelled; retain its result before allowing dismissal.
        if (!opened || applying) return
        requested({ c: "permissions", op: "close", id: requestId })
        opened = false
        closed()
        if (focusHolder) focusHolder.forceActiveFocus()
    }
    function apply() {
        if (!editable || modeValue < 0) return
        root.forceActiveFocus()
        busy = true
        errorText = ""
        requested({ c: "permissions", op: "apply", id: requestId, mode: modeText })
    }
    function focusItems(item, result) {
        if (!item.visible || !item.enabled) return
        if (item.activeFocusOnTab) result.push(item)
        for (var i = 0; i < item.children.length; i++) focusItems(item.children[i], result)
    }
    function stepFocus(back) {
        var items = []
        focusItems(card, items)
        if (!items.length) return
        var current = -1
        for (var i = 0; i < items.length; i++) if (items[i].activeFocus) current = i
        var next = current < 0 ? (back ? items.length - 1 : 0)
            : (current + (back ? -1 : 1) + items.length) % items.length
        items[next].forceActiveFocus()
        body.reveal(items[next])
    }
    Keys.onTabPressed: function(event) { root.stepFocus((event.modifiers & Qt.ShiftModifier) !== 0); event.accepted = true }
    Keys.onBacktabPressed: function(event) { root.stepFocus(true); event.accepted = true }
    Keys.onPressed: function(event) { event.accepted = true }
    Keys.onEscapePressed: function(event) { root.close(); event.accepted = true }
    Rectangle {
        anchors.fill: parent
        color: Theme.color.background
        opacity: 0.5
        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onClicked: root.close()
            onWheel: function(wheel) { wheel.accepted = true }
        }
    }
    Rectangle {
        id: card
        anchors.centerIn: parent
        // The board's 420 content width shares the Settings board's 560 scale.
        width: Math.max(0, Math.min(Theme.settings.panelWidth * 3 / 4 + 2 * Theme.spacing.hairline, root.width - 2 * Theme.spacing.gap))
        height: Math.max(0, Math.min(chrome.height + body.wanted + Theme.spacing.rowPaddingX + root.bodyInset, root.height - 2 * Theme.spacing.gap))
        color: Theme.color.surface
        border.color: Theme.color.muted
        border.width: Theme.spacing.hairline
        radius: Style.cornerRadius
        clip: true
        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onWheel: function(wheel) { wheel.accepted = true }
        }
        Item {
            id: chrome
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            height: Theme.chromeHeight
            Row {
                anchors.left: parent.left
                anchors.leftMargin: Theme.spacing.rowPaddingX
                anchors.verticalCenter: parent.verticalCenter
                spacing: Theme.spacing.gap
                Flea.Glyph { width: Theme.chromeMarkSize; height: title.height; name: "lock"; color: Theme.color.accent }
                Text {
                    id: title
                    text: "Permissions"
                    color: Theme.color.foreground
                    font { family: Theme.font.family; pixelSize: Theme.font.caption; bold: true }
                    textFormat: Text.PlainText
                }
            }
            Flea.ChromeButton {
                id: closeMark
                anchors.right: parent.right
                anchors.rightMargin: Theme.spacing.rowPaddingX
                anchors.verticalCenter: parent.verticalCenter
                glyph: "x"
                enabled: !root.applying
                accessName: "Close permissions"
                activeFocusOnTab: true
                keyboardFocused: activeFocus
                Keys.onTabPressed: function(event) { root.stepFocus((event.modifiers & Qt.ShiftModifier) !== 0) }
                Keys.onBacktabPressed: root.stepFocus(true)
                Keys.onReturnPressed: root.close()
                Keys.onSpacePressed: root.close()
                onActivated: root.close()
            }
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: Theme.spacing.hairline; color: Theme.color.muted; opacity: 0.4 }
        }
        Flea.CardScroll {
            id: body
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: chrome.bottom
            anchors.bottom: parent.bottom
            anchors.leftMargin: root.bodyInset
            anchors.rightMargin: root.bodyInset
            anchors.topMargin: Theme.spacing.rowPaddingX
            anchors.bottomMargin: root.bodyInset
            Column {
                width: body.width
                spacing: 0
                Row {
                    width: parent.width
                    height: root.controlHeight
                    spacing: Theme.spacing.gap
                    Flea.Glyph { width: Theme.markSize; height: nameLabel.height; anchors.verticalCenter: parent.verticalCenter; name: root.facts.directory ? "folder" : "file"; color: Theme.color.muted }
                    Text { id: nameLabel; width: parent.width - Theme.markSize - kindLabel.width - 2 * parent.spacing; anchors.verticalCenter: parent.verticalCenter; text: root.path.split("/").pop(); elide: Text.ElideMiddle; textFormat: Text.PlainText; color: Theme.color.foreground; font { family: Theme.font.family; pixelSize: Theme.font.body } }
                    Text { id: kindLabel; anchors.verticalCenter: parent.verticalCenter; text: root.facts.ok ? (root.facts.directory ? "directory" : "file") : ""; textFormat: Text.PlainText; color: Theme.color.foreground; font { family: Theme.font.family; pixelSize: Theme.font.caption } }
                }
                Rectangle { width: parent.width; height: Theme.spacing.hairline; color: Theme.color.muted; opacity: 0.4 }
                Row {
                    width: parent.width
                    height: root.headingHeight
                    Item { width: root.labelWidth; height: parent.height }
                    Repeater {
                        model: ["READ", "WRITE", root.facts.directory ? "ENTER" : "EXEC"]
                        Text { required property string modelData; width: (body.width - root.labelWidth) / 3; height: parent.height; horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter; text: modelData; textFormat: Text.PlainText; color: Theme.color.foreground; font { family: Theme.font.family; pixelSize: Theme.font.caption; letterSpacing: Theme.font.caption / 10 } }
                    }
                }
                Repeater {
                    id: permissionRows
                    model: ["Owner", "Group", "Everyone"]
                    Row {
                        id: permissionRow
                        required property string modelData
                        required property int index
                        readonly property alias checks: checks
                        width: body.width
                        height: root.controlHeight
                        Text { width: root.labelWidth; anchors.verticalCenter: parent.verticalCenter; text: permissionRow.modelData; textFormat: Text.PlainText; color: Theme.color.foreground; font { family: Theme.font.family; pixelSize: Theme.font.body } }
                        Repeater {
                            id: checks
                            model: 3
                            FocusScope {
                                id: checkbox
                                required property int index
                                readonly property int bit: 1 << (8 - permissionRow.index * 3 - index)
                                readonly property bool checked: (root.modeValue >= 0 ? root.modeValue : parseInt(root.facts.mode || "0", 8)) & bit
                                width: (body.width - root.labelWidth) / 3
                                height: permissionRow.height
                                activeFocusOnTab: true
                                enabled: root.editable
                                opacity: root.editable ? 1 : 0.45
                                Accessible.role: Accessible.CheckBox
                                Accessible.name: permissionRow.modelData + " " + ["read", "write", root.facts.directory ? "enter" : "execute"][index]
                                Accessible.checked: checked
                                Accessible.onPressAction: toggle()
                                Accessible.onToggleAction: toggle()
                                function toggle() { if (root.editable) { root.modeText = Permissions.toggle(root.modeText, bit); forceActiveFocus() } }
                                Keys.onSpacePressed: toggle()
                                Keys.onTabPressed: function(event) { root.stepFocus((event.modifiers & Qt.ShiftModifier) !== 0) }
                                Keys.onBacktabPressed: root.stepFocus(true)
                                Rectangle {
                                    anchors.centerIn: parent
                                    width: Theme.font.bodySmall * 16 / 13
                                    height: width
                                    color: "transparent"
                                    border.width: Theme.spacing.hairline * 2
                                    border.color: checkbox.checked || checkbox.activeFocus ? Theme.color.accent : Theme.color.muted
                                    Flea.Glyph { anchors.centerIn: parent; width: Theme.font.bodySmall * 10 / 13; height: width; strokeWidth: 3; name: "check"; visible: checkbox.checked; color: Theme.color.accent }
                                }
                                TapHandler { onTapped: checkbox.toggle() }
                            }
                        }
                    }
                }
                Item {
                    width: parent.width
                    height: Theme.spacing.rowPaddingY + Theme.settings.railPaddingY / 2 + Theme.spacing.hairline
                    Rectangle { y: Theme.settings.railPaddingY / 2; width: parent.width; height: Theme.spacing.hairline; color: Theme.color.muted; opacity: 0.4 }
                }
                Row {
                    width: parent.width
                    height: root.controlHeight
                    spacing: Theme.spacing.gap
                    Text { width: root.labelWidth; anchors.verticalCenter: parent.verticalCenter; text: "Octal"; textFormat: Text.PlainText; color: Theme.color.foreground; font { family: Theme.font.family; pixelSize: Theme.font.body } }
                    Rectangle {
                        width: body.width - root.labelWidth - parent.spacing
                        height: parent.height
                        color: Theme.color.background
                        border.color: octal.activeFocus ? Theme.color.accent : Theme.color.muted
                        TextInput {
                            id: octal
                            anchors.fill: parent
                            anchors.leftMargin: Theme.spacing.gap
                            anchors.rightMargin: Theme.spacing.gap
                            verticalAlignment: TextInput.AlignVCenter
                            text: root.modeText
                            readOnly: !root.editable
                            activeFocusOnTab: true
                            enabled: root.editable
                            Accessible.name: "Octal mode"
                            color: root.editable ? Theme.color.foreground : Theme.color.muted
                            font { family: Theme.font.family; pixelSize: Theme.font.body }
                            clip: true
                            selectByMouse: true
                            Keys.onTabPressed: function(event) { root.stepFocus((event.modifiers & Qt.ShiftModifier) !== 0) }
                            Keys.onBacktabPressed: root.stepFocus(true)
                            onTextEdited: root.modeText = text
                            onAccepted: if (root.editable && root.modeValue >= 0) applyFocus.forceActiveFocus()
                        }
                    }
                }
                Repeater {
                    model: ["Owner", "Group"]
                    Row {
                        required property string modelData
                        required property int index
                        width: body.width
                        height: root.controlHeight
                        spacing: Theme.spacing.gap
                        Text { width: root.labelWidth; anchors.verticalCenter: parent.verticalCenter; text: parent.modelData; textFormat: Text.PlainText; color: Theme.color.foreground; font { family: Theme.font.family; pixelSize: Theme.font.body } }
                        Text { width: body.width - root.labelWidth - identity.width - 2 * parent.spacing; anchors.verticalCenter: parent.verticalCenter; text: root.facts.ok ? ((parent.index === 0 ? root.facts.owner : root.facts.group) || "Unknown") + " · read-only" : ""; textFormat: Text.PlainText; elide: Text.ElideRight; color: Theme.color.foreground; font { family: Theme.font.family; pixelSize: Theme.font.body } }
                        Text { id: identity; anchors.verticalCenter: parent.verticalCenter; text: root.facts.ok ? (parent.index === 0 ? "uid " + root.facts.uid : "gid " + root.facts.gid) : ""; textFormat: Text.PlainText; color: Theme.color.foreground; font { family: Theme.font.family; pixelSize: Theme.font.caption } }
                    }
                }
                Rectangle {
                    width: parent.width
                    height: directoryScope.implicitHeight + 2 * Theme.spacing.gap
                    visible: root.facts.directory === true
                    color: Theme.color.background
                    border.width: Theme.spacing.hairline
                    border.color: Theme.color.muted
                    Flea.Glyph { id: scopeMark; x: Theme.spacing.gap; y: Theme.spacing.gap; width: Theme.font.bodySmall; height: width; name: "check"; color: Theme.color.accent }
                    Text {
                        id: directoryScope
                        x: scopeMark.x + scopeMark.width + Theme.spacing.gap
                        y: Theme.spacing.gap
                        width: parent.width - x - Theme.spacing.gap
                        text: "Scope: this directory only. Enclosed files and directories keep every bit."
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        color: Theme.color.foreground
                        font { family: Theme.font.family; pixelSize: Theme.font.caption }
                    }
                }
                Text {
                    id: errorLabel
                    width: parent.width
                    visible: text.length > 0
                    topPadding: Theme.spacing.gap
                    text: root.errorText || root.facts.reason || (root.busy
                        ? (root.facts.ok ? "Applying permissions…" : "Reading permissions…")
                        : root.facts.ok && root.modeValue < 0 ? "Enter three octal digits or a leading-zero four-digit mode." : "")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: root.busy ? Theme.color.muted : Theme.color.error
                    font { family: Theme.font.family; pixelSize: Theme.font.caption }
                }
                Item {
                    width: parent.width
                    height: Theme.settings.railPaddingY + Theme.spacing.gap + Theme.spacing.hairline
                    Rectangle { y: Theme.settings.railPaddingY; width: parent.width; height: Theme.spacing.hairline; color: Theme.color.muted; opacity: 0.4 }
                }
                Text {
                    text: "WILL CHANGE"
                    bottomPadding: Theme.spacing.rowPaddingY / 2
                    textFormat: Text.PlainText
                    color: Theme.color.foreground
                    font { family: Theme.font.family; pixelSize: Theme.font.caption; letterSpacing: Theme.font.caption / 10 }
                }
                Text {
                    id: changeSummary
                    width: parent.width
                    text: (root.facts.ok && !root.facts.reason && root.modeValue >= 0
                        ? "Requested mode " + Permissions.octal(root.modeValue) : "No changes available")
                        + "\nPath " + root.path.replace(/\//g, "/\u200b")
                        + (root.facts.reason ? "\nCurrent mode " + root.facts.mode : "")
                    textFormat: Text.PlainText
                    wrapMode: Text.WordWrap
                    color: Theme.color.accent
                    font { family: Theme.font.family; pixelSize: Theme.font.caption }
                }
                Text {
                    id: scopeLabel
                    width: parent.width
                    text: root.scopeText
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Theme.color.foreground
                    font { family: Theme.font.family; pixelSize: Theme.font.caption }
                }
                Item { width: parent.width; height: Theme.settings.railPaddingY + Theme.spacing.hairline }
                Row {
                    anchors.right: parent.right
                    spacing: Theme.spacing.gap
                    FocusScope {
                        id: cancelFocus
                        width: cancelButton.implicitWidth; height: cancelButton.implicitHeight
                        activeFocusOnTab: true
                        enabled: !root.applying
                        Keys.onTabPressed: function(event) { root.stepFocus((event.modifiers & Qt.ShiftModifier) !== 0) }
                        Keys.onBacktabPressed: root.stepFocus(true)
                        Keys.onReturnPressed: root.close()
                        Keys.onSpacePressed: root.close()
                        Flea.DialogButton {
                            id: cancelButton
                            label: "Cancel"
                            primary: parent.activeFocus
                            horizontalPadding: Theme.spacing.rowPaddingX
                            verticalPadding: 6 * Theme.font.bodySmall / 13
                            available: !root.applying
                            onActivated: root.close()
                        }
                    }
                    FocusScope {
                        id: applyFocus
                        width: applyButton.implicitWidth; height: applyButton.implicitHeight
                        activeFocusOnTab: true
                        enabled: root.editable && root.modeValue >= 0
                        Keys.onTabPressed: function(event) { root.stepFocus((event.modifiers & Qt.ShiftModifier) !== 0) }
                        Keys.onBacktabPressed: root.stepFocus(true)
                        Keys.onReturnPressed: root.apply()
                        Keys.onSpacePressed: root.apply()
                        Flea.DialogButton {
                            id: applyButton
                            label: "Apply"
                            primary: parent.activeFocus
                            horizontalPadding: Theme.spacing.rowPaddingX
                            verticalPadding: 6 * Theme.font.bodySmall / 13
                            available: root.editable && root.modeValue >= 0
                            onActivated: root.apply()
                        }
                    }
                }
            }
        }
    }
}
