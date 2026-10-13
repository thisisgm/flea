import QtQuick
import qs.Commons
import "." as Flea
import "js/Buttons.js" as Buttons
import "js/Format.js" as Format
import "js/Picker.js" as Picker

// The picker's two chrome strips: what was asked for and the two answers on top, where the list is
// standing and what it is narrowed to underneath. SendPicker.html draws both.
Item {
    id: root
    // Its Jump dropdown hangs below the strip, so the chrome rises over the places and the listing while it shows.
    z: entry.jumpShown || folderButton.hovered ? 1 : 0

    property var picker: null
    property var submissionFocus: null
    property var listing: null
    property var backend: null

    readonly property bool editing: entry.editing
    readonly property var jump: entry.jump
    function startEdit() { entry.startEdit() }

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
    signal newFolderRequested()

    readonly property var req: root.picker.req
    readonly property var chips: Picker.chips(root.req)

    readonly property color edge: root.picker.edge

    readonly property bool folderButtonWrapped: !!root.picker.saving && root.width < moves.width + views.width + folderButton.implicitWidth + Theme.space(280)
    implicitHeight: ask.height + where.height + (root.folderButtonWrapped ? Theme.chromeHeight : 0)

    // The picker's answers are ButtonSystem040 A's one control at body size, framed inside the ask strip's insets; the picker owns Tab.
    component Answer: Flea.DialogButton {
        id: answer
        property string name: answer.label
        ruledStripHeight: ask.height
        tabHandle: true
        tabStop: true
        onTabbed: function(from, back) { root.picker.stepFocus(answer, back) }
    }

    // Back and Up rest in the foreground like the main chrome; only an unlit view mark rests muted.
    component Mark: Flea.ChromeButton {
        id: mark
        ruleRows: Theme.spacing.hairline
        property bool lit: false
        readonly property bool available: mark.enabled
        property string name: mark.accessName
        gesturePolicy: TapHandler.ReleaseWithinBounds
        restingColor: Theme.color.foreground
        activeFocusOnTab: mark.enabled
        keyboardFocused: mark.activeFocus
        Keys.onTabPressed: function(event) { root.picker.stepFocus(mark, (event.modifiers & Qt.ShiftModifier) !== 0) }
        Keys.onBacktabPressed: root.picker.stepFocus(mark, true)
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
        scale: press.pressed && control.available && !Theme.reducedMotion ? Buttons.PRESS_SCALE : 1

        Accessible.role: Accessible.Button
        Accessible.name: control.name
        Accessible.onPressAction: if (control.available) control.pressed()

        Behavior on scale {
            enabled: !Theme.reducedMotion
            NumberAnimation { duration: Buttons.PRESS_MS; easing.type: Easing.OutCubic }
        }

        readonly property color frame: control.primary
            ? Theme.color.accentFrame : Theme.color.mutedFrame

        // The chosen chip carries its wash at rest; every other earns one under the pointer or the keyboard.
        readonly property real wash: control.primary ? Theme.washActive : !control.available ? 0
            : (control.activeFocus || press.pressed) ? Theme.washActive
            : hover.hovered ? Theme.washHover : 0
        readonly property color washInk: control.primary ? Theme.color.accent : Theme.color.foreground

        // A framed control in a ruled strip keeps the control inset from the strip top and its rule; the host gives the strip top in this item's coordinates so the frame lands on whole pixels.
        property real stripTop: 0
        readonly property real frameHeight: Math.min(control.height, Theme.chromeControlHeight)
        readonly property real frameY: control.stripTop + Math.round((Theme.chromeHeight - Theme.spacing.hairline - control.frameHeight) / 2)

        Item {
            y: control.frameY
            width: parent.width
            height: control.frameHeight

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

        // The answers' frames start at the strip inset, so they keep it from the rule too and the rule never crosses one.
        Row {
            id: buttons
            anchors.right: parent.right
            anchors.rightMargin: Theme.spacing.rowPaddingX
            y: Theme.chromeControlInset - cancelButton.frameY
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
        objectName: "pickerWhere"
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: ask.bottom
        // The board's 27 px strip at base-size 14: the chrome's own height, so the field and its dropdown sit as in the main window.
        height: Theme.chromeHeight

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
                objectName: "pickerBack"
                glyph: "arrow-left"
                enabled: !root.picker.backendUnavailable && root.picker.history.length > 0 && !root.picker.submitting
                onActivated: root.backRequested()
            }

            Mark {
                id: upButton
                objectName: "pickerUp"
                glyph: "arrow-up"
                accessName: root.picker.recent ? "Parent folder unavailable in Recent" : "Parent folder"
                // The board's own rule, drawn as its disabled Up: a history has no directory above it.
                enabled: !root.picker.backendUnavailable && !root.picker.submitting && !root.picker.recent && Picker.parentOf(root.picker.path) !== root.picker.path
                onActivated: root.upRequested()
            }
        }

        // Parents muted, the leaf lit inline after them; a long path elides its parents from the left and keeps the leaf whole.
        Row {
            id: strip
            objectName: "pickerStrip"
            anchors.left: moves.right
            anchors.leftMargin: Theme.spacing.gap
            anchors.right: types.left
            anchors.rightMargin: Theme.spacing.gap
            anchors.verticalCenter: parent.verticalCenter
            // The marks centre in the band above the rule, so the path does too.
            anchors.verticalCenterOffset: -Theme.spacing.hairline / 2
            visible: !entry.editing
            // Recent is a location and not a path, so the strip says its name, never a tilde form of the token.
            readonly property string display: root.picker.recent ? Picker.RECENT_LABEL : Format.tilde(root.picker.path, root.picker.home)
            readonly property string leaf: root.picker.recent ? strip.display : Format.leafPart(strip.display)
            readonly property string parents: root.picker.recent ? "" : strip.display.substring(0, strip.display.length - strip.leaf.length)
            Text {
                id: stripParents
                width: Math.min(implicitWidth, Math.max(0, strip.width - stripLeaf.width))
                visible: strip.parents.length > 0
                text: strip.parents
                color: Theme.color.muted
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                elide: Text.ElideLeft
                textFormat: Text.PlainText
            }
            Text {
                id: stripLeaf
                width: Math.min(implicitWidth, strip.width)
                text: strip.leaf
                color: Theme.color.foreground
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                elide: Text.ElideMiddle
                textFormat: Text.PlainText
            }
        }

        // Picker040 path entry: the shared field, whose line pathLogic below answers.
        Flea.PathEntry {
            id: entry
            anchors.left: moves.right
            anchors.leftMargin: Theme.spacing.gap
            anchors.right: types.left
            anchors.rightMargin: Theme.spacing.gap
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            anchors.bottomMargin: Theme.spacing.hairline
            path: pathLogic.basePath
            home: root.picker.home
            showHidden: root.picker.showHidden
            onCommitLine: function (text) { pathLogic.commitTyped(text) }
            onJumpChosen: function (path) { pathLogic.ask(path) }
            onEditClosed: pathLogic.editClosed()
            onCompleteRequested: function (dir, hidden) { root.listing.peek(dir, root.picker.windowSize, hidden) }
            onSaid: function (text) { root.picker.say(text, false) }
            onJumpRequested: function (id, ranking, favourites, recent) { root.listing.jump(id, ranking, favourites, recent) }
        }

        Flea.PickerPathBar {
            id: pathLogic
            picker: root.picker
            listing: root.listing
            backend: root.backend
            entry: entry
        }

        // The caller's filters and All files; no filters, no chips. The view marks keep the far right.
        Flickable {
            id: types
            anchors.right: parent.right
            anchors.rightMargin: views.width + Theme.spacing.rowPaddingX + Theme.spacing.gap
                + (root.picker.saving && !root.folderButtonWrapped ? folderButton.width + Theme.spacing.gap : 0)
            anchors.verticalCenter: parent.verticalCenter
            // The chips take room first and the path gives way through its anchor, keeping its minimum.
            width: Picker.chipStripWidth(where.width - moves.width - views.width - (root.picker.saving && !root.folderButtonWrapped ? folderButton.width + Theme.spacing.gap : 0) - 2 * Theme.spacing.rowPaddingX - 3 * Theme.spacing.gap, chipRow.width, Picker.CHIP_PATH_MIN)
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
                spacing: Theme.spacing.hairline * Buttons.SET_GAP

                Repeater {
                    id: chipRepeater
                    model: root.chips

                    Framed {
                        required property var modelData
                        label: modelData.label
                        stripTop: -types.y
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
            objectName: "pickerListView"
            glyph: "list"
            lit: root.picker.viewMode === "list"
            restingColor: listButton.lit ? Theme.color.foreground : Theme.color.mutedFrame
            activeFocusOnTab: false
            onActivated: root.viewChosen("list")
        }

        Mark {
            id: gridButton
            objectName: "pickerGridView"
            glyph: "grid"
            lit: root.picker.viewMode === "grid"
            restingColor: gridButton.lit ? Theme.color.foreground : Theme.color.mutedFrame
            activeFocusOnTab: false
            onActivated: root.viewChosen("grid")
        }
    }

    Mark {
        id: folderButton
        objectName: "pickerNewFolder"
        accessName: "New folder"
        visible: !!root.picker.saving
        enabled: !!root.picker.canCreateFolder
        markItem.visible: false
        ringItem.visible: false
        anchors.right: root.folderButtonWrapped ? parent.right : views.left
        anchors.rightMargin: root.folderButtonWrapped ? Theme.spacing.rowPaddingX : Theme.spacing.gap
        anchors.top: root.folderButtonWrapped ? where.bottom : where.top
        onActivated: root.newFolderRequested()
        Text {
            objectName: "pickerFolderGlyph"
            text: "\ueec7" // Nerd Fonts: nf-fa-folder_plus (U+EEC7)
            font.family: "Symbols Nerd Font Mono"
            font.pixelSize: folderButton.glyphSize
            width: folderButton.glyphSize
            height: width
            anchors.horizontalCenter: parent.horizontalCenter
            y: Math.round((parent.height - Theme.spacing.hairline - height) / 2)
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            color: !folderButton.enabled ? Theme.color.mutedFrame : folderButton.activeFocus ? Theme.color.accent : Theme.color.foreground
            opacity: folderButton.enabled ? 1 : Theme.disabledOpacity
            textFormat: Text.PlainText
        }
    }
    Rectangle {
        visible: folderButton.hovered && folderButton.enabled
        x: Math.max(0, folderButton.x + folderButton.width - width)
        y: folderButton.y + folderButton.height
        width: folderHint.implicitWidth + 2 * Theme.spacing.gap
        height: folderHint.implicitHeight + 2 * Theme.spacing.rowPaddingY
        color: Theme.color.surface
        Text {
            id: folderHint
            anchors.centerIn: parent
            text: "New folder (Ctrl+Shift+N)"
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            color: Theme.color.foreground
            textFormat: Text.PlainText
        }
    }

    function focusItems() {
        var items = [cancelButton, acceptButton, backButton, upButton]
        if (root.picker.saving) items.push(folderButton)
        for (var i = 0; i < chipRepeater.count; i++) items.push(chipRepeater.itemAt(i))
        return items
    }
    function controls() {
        return root.focusItems().concat([listButton, gridButton]).map(function(item) { return root.picker.control(item.name, item, item.available) })
    }
}
