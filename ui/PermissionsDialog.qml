import QtQuick
import qs.Commons
import "." as Flea
import "js/Format.js" as Format
import "js/Permissions.js" as Permissions

// One item, one mode; the backend owns its reviewed descriptor for this dialog's lifetime.
FocusScope {
    id: root
    anchors.fill: parent
    visible: opened || opacity > 0
    // Closed means no input, even while the fade-out is still drawn.
    enabled: opened
    opacity: 0
    Flea.Reveal { target: root; shown: root.opened; lift: card }
    property bool opened: false
    property int requestId: 0
    // Inspect ids are request blocks of this width; a wider selection reserves whole blocks.
    readonly property int inspectStride: 1000
    property int multiBase: 0
    property var facts: ({})
    property string path: ""
    property string modeText: ""
    property string errorText: ""
    property bool busy: false
    property bool transportFailed: false
    property Item focusHolder: null
    // The selection's paths with modes in the same order, the first inspect refusal, and the explicit bits.
    property var multiPaths: []
    // The live inspect store noteMode owns; multiModes is its one last-reply snapshot.
    property var multiStore: ({ modes: [], reasons: [], skipped: [] })
    property var multiModes: []
    property int multiPending: 0
    property int explicitSet: 0
    property int explicitClear: 0
    property bool applyingMany: false
    // A failed batch's own error until its re-read lands, so the held note may replace it when it names that refusal.
    property string failedBatchError: ""
    // The Apply skips, carried to the applyMany reply for the final message.
    property var multiApplySkipped: []
    readonly property bool isMulti: multiPaths.length > 1
    readonly property var multiSummary: isMulti ? Permissions.summarize(multiModes, multiStore.reasons) : null
    // Several items: a selection where no file can change shows the files' bits and is not editable, as one unchangeable file is.
    readonly property bool editable: isMulti ? !busy && !transportFailed && !!multiSummary && multiSummary.changeable
                                              : facts.ok === true && !facts.reason && !busy && !transportFailed
    readonly property int modeValue: Permissions.parse(modeText)
    readonly property bool applying: busy && facts.ok === true
    // Permissions040: a multi Apply cannot be cancelled either.
    readonly property bool applyLocked: applying || applyingMany
    // One Apply condition for the button and the probe, which reads the button.
    readonly property bool canApply: isMulti ? editable : editable && modeValue >= 0
    readonly property real labelWidth: Math.round(96 * root.boardScale)
    // ButtonSystem040 A: a text field is DialogField's height (ui/DialogField.qml's box), centred in its row.
    readonly property int fieldHeight: Theme.rowHeight - Theme.spacing.rowPaddingY
    readonly property int controlHeight: Math.max(Theme.rowHeight, Math.ceil(Theme.font.body * Theme.lineBoxRatio) + 2 * Theme.spacing.rowPaddingY)
    // The board is drawn at base size 14, whose bodySmall is 13; every board pixel below scales from it and lands whole.
    readonly property real boardScale: Theme.font.bodySmall / 13
    readonly property int boardCardWidth: 480
    readonly property int bodyInset: Math.round(16 * root.boardScale) + Theme.spacing.hairline
    readonly property int headingHeight: Math.round(26 * root.boardScale)
    // Permissions040, several items: the surface's one 8 px gap, and the 6 px its button row adds above itself.
    readonly property int multiGap: Theme.spacing.rowPaddingY + Theme.spacing.hairline
    // The board's three flex:1 columns are exact thirds of this span; whole-number arithmetic rounds each start half up, as the board's paint does.
    readonly property real bitSpan: body.holderWidth - root.labelWidth
    function bitStart(column) { return Math.floor((2 * column * root.bitSpan + 3) / 6) }
    function bitWidth(column) { return root.bitStart(column + 1) - root.bitStart(column) }
    // The box sits at the rounded exact centre of its third, so it lands on whole pixels where the board's does.
    function boxLead(column, box) { return Math.floor((2 * column * root.bitSpan + root.bitSpan - 3 * box + 3) / 6) - root.bitStart(column) }
    // lib.py note(): a caption's line box is 1.5 x its size and its glyphs sit centred in it.
    readonly property int captionLead: Math.round((SettingsMetrics.hintLine - noteFont.height) / 2)
    // Permissions040's strip draws the title and "esc" glyphs one row above where Qt's line box seats them (board rows 7-16 and 10-16, the build's 8-17 and 11-17 once centred above the rule).
    readonly property int stripTextRise: Theme.spacing.hairline
    readonly property int railHalf: Math.round(Theme.settings.railPaddingY / 2)
    readonly property int buttonLead: root.railHalf + Theme.spacing.hairline
    readonly property var cardItem: card
    // The title strip's four marks, so a probe reads where each sits against the board's rows.
    readonly property var stripItems: ({ lock: lockMark, title: title, esc: escHint, close: closeMark })
    readonly property var noteItem: scopeLabel
    readonly property var octalFrame: octalBox
    readonly property var bodyItem: body
    readonly property string displayedError: errorLabel.text
    readonly property string displayedSummary: (isMulti ? "" : changeSummary.text + "\n") + scopeLabel.text
    // One section rule for the four drawn below; each site sets its own place and visibility.
    component Rule: Rectangle {
        width: parent.width
        height: Theme.spacing.hairline
        color: Theme.color.mutedFrame
        opacity: 0.4
    }
    // No enabled keys: each item.enabled already carries every mode's answer, a copy could disagree.
    function controls() {
        var result = [{name: "Close", item: closeMark}, {name: "Octal", item: octal},
            {name: "Cancel", item: cancelButton}, {name: "Apply", item: applyButton}]
        for (var row = 0; row < permissionRows.count; row++) {
            var group = permissionRows.itemAt(row)
            for (var column = 0; column < group.checks.count; column++) {
                var checkbox = group.checks.itemAt(column)
                result.push({name: checkbox.Accessible.name, item: checkbox, checked: checkbox.checked, bit: checkbox.bit})
            }
        }
        return result
    }
    readonly property string scopeText: facts.directory
        ? "Scope this folder only · enclosed items unchanged · ownership unchanged"
        : "Scope this item only · ownership unchanged"
    signal requested(var message)
    signal changed(string note)
    signal refreshNeeded()
    signal closed()

    // One reset for both openers, so a field added to one cannot be missed in the other.
    function reset(itemPath, paths, holder) {
        multiPaths = paths.slice()
        multiStore = ({ modes: [], reasons: [], skipped: [], pending: paths.length })
        multiModes = []
        multiPending = paths.length
        multiApplySkipped = []
        failedBatchError = ""
        explicitSet = 0
        explicitClear = 0
        applyingMany = false
        path = itemPath
        focusHolder = holder
        facts = ({})
        modeText = ""
        errorText = ""
        transportFailed = false
        busy = true
        opened = true
        body.contentY = 0
        cancelButton.forceActiveFocus()
    }
    // One inspect fan-out for openMany and the failed-batch re-read: the same requests.
    function inspectAll() {
        multiStore = ({ modes: [], reasons: [], skipped: [], pending: multiPaths.length })
        multiModes = []
        multiPending = multiPaths.length
        for (var i = 0; i < multiPaths.length; i++)
            requested({ c: "permissions", op: "inspect", id: multiBase * inspectStride + i, path: multiPaths[i] })
    }

    function open(itemPath, holder) {
        requestId += 1
        reset(itemPath, [], holder)
        requested({ c: "permissions", op: "inspect", id: requestId, path: path })
    }
    // One Entry holds every path the Apply changed, so one undo restores them all.
    function openMany(paths, holder) {
        requestId += 1
        multiBase = requestId
        reset("", paths, holder)
        inspectAll()
        // A wider selection reserves whole strides so the next open's block cannot overlap this one.
        requestId += Math.max(0, Math.floor((paths.length - 1) / inspectStride))
    }
    function receive(message) {
        if (isMulti) { receiveMany(message); return }
        if (!opened || transportFailed || message.id !== requestId || message.op === "close") return
        busy = false
        cancelButton.forceActiveFocus()
        if (!message.ok) { errorText = message.error || "Could not change permissions."; return }
        if (message.op === "apply") { changed(""); close(); return }
        facts = message
        modeText = message.mode
    }
    // The grid over several files; an untouched box takes the explicit opposite of what it shows.
    function multiBit(bit) {
        if (!multiSummary) return { on: false, mixed: false }
        for (var i = 0; i < multiSummary.bits.length; i++) {
            if (multiSummary.bits[i].mask === bit)
                return multiSummary.bits[i]
        }
        return { on: false, mixed: false }
    }
    function multiChecked(bit) {
        if ((explicitSet & bit) !== 0) return true
        if ((explicitClear & bit) !== 0) return false
        return multiBit(bit).on
    }
    function multiValue(bit) {
        if ((explicitSet & bit) !== 0) return "on"
        if ((explicitClear & bit) !== 0) return "off"
        var found = multiBit(bit)
        if (found.mixed) return "some"
        return found.on ? "on" : "off"
    }
    // A set box lets go unless mixed, so a uniform-off box reads off, on, off, on.
    function multiToggle(bit) {
        if ((explicitSet & bit) !== 0) {
            explicitSet &= ~bit
            if (multiBit(bit).mixed) explicitClear |= bit
        } else if ((explicitClear & bit) !== 0) {
            explicitClear &= ~bit
        } else if (multiBit(bit).mixed || !multiBit(bit).on) {
            explicitSet |= bit
            explicitClear &= ~bit
        } else {
            explicitClear |= bit
            explicitSet &= ~bit
        }
    }
    function receiveMany(message) {
        if (!opened || transportFailed || (message.op === "applyMany" && (!applyingMany || message.id !== requestId))) return
        if (message.op === "applyMany") {
            applyingMany = false
            if (message.ok === true) {
                changed(Permissions.multiResult(multiApplySkipped))
                close()
                return
            }
            errorText = message.error || "Could not change permissions."
            if (multiApplySkipped.length > 0)
                errorText += "\n" + Permissions.skipNote(multiApplySkipped)
            // A failed batch re-reads every mode so the grid and a retry start from disk.
            failedBatchError = message.error || ""
            inspectAll()
            busy = true
            cancelButton.forceActiveFocus()
            refreshNeeded()
            return
        }
        if (message.op !== "inspect") return
        var at = (message.id || 0) - multiBase * inspectStride
        if (at < 0 || at >= multiPaths.length) return
        // Accumulated in place through noteMode, which answers true once per selection.
        if (Permissions.noteMode(multiStore, at, multiPaths[at], message)) {
            multiPending = 0
            // The single assignment lands with the last reply, before busy clears, so editable never reads the previous selection's summary.
            multiModes = multiStore.modes.slice()
            busy = false
            var note = Permissions.inspectNote(multiStore, multiPaths)
            // A batch refused for a file the re-read cannot inspect is named by the held note, which quotes that refusal; any other batch error stays.
            var refused = failedBatchError.length > 0 && multiStore.skipped.some(function (skip) { return skip.why === failedBatchError })
            if (note.length > 0 && (errorText.length === 0 || refused)) errorText = note
            failedBatchError = ""
            cancelButton.forceActiveFocus()
        } else {
            multiPending = multiStore.pending
        }
    }
    function backendFailed() {
        if (!opened || transportFailed) return
        // A multi Apply counts as applying with an unknown outcome, and ends so Cancel and close work.
        var applying = applyLocked
        applyingMany = false
        transportFailed = true
        busy = false
        cancelButton.forceActiveFocus()
        errorText = applying
            ? "Flea stopped responding, so the permission change may not have finished. Restart Flea and reopen Permissions to check."
            : "Permissions is unavailable because Flea stopped responding. Restart Flea and reopen Permissions."
    }
    function close() {
        // An issued fchmod cannot be cancelled; retain its result before allowing dismissal.
        if (!opened || applyLocked) return
        if (isMulti) {
            for (var i = 0; i < multiPaths.length; i++)
                requested({ c: "permissions", op: "close", id: multiBase * inspectStride + i })
        } else {
            requested({ c: "permissions", op: "close", id: requestId })
        }
        opened = false
        closed()
        if (focusHolder) focusHolder.forceActiveFocus()
    }
    function apply() {
        if (isMulti) { applyMany(); return }
        if (!editable || modeValue < 0) return
        root.forceActiveFocus()
        busy = true
        errorText = ""
        requested({ c: "permissions", op: "apply", id: requestId, mode: modeText })
    }
    // Apply is one undo step; unchangeable items are counted and an empty batch answers at once.
    function applyMany() {
        if (!editable) return
        root.forceActiveFocus()
        busy = true
        applyingMany = true
        errorText = ""
        // Built fresh from the per-row reasons, so every skip is counted exactly once.
        var skipped = []
        var paths = []
        var modes = []
        for (var i = 0; i < multiPaths.length; i++) {
            var why = multiStore.reasons[i] || ""
            var base = Permissions.parse(multiStore.modes[i])
            if (why.length === 0 && base < 0)
                why = Permissions.specialReason(multiStore.modes[i])
            if (why.length > 0 || base < 0) {
                skipped.push({ path: multiPaths[i],
                    why: why.length > 0 ? why : "Could not change permissions." })
                continue
            }
            var target = (base & ~explicitClear) | explicitSet
            paths.push(multiPaths[i])
            modes.push(Permissions.octal(target))
        }
        if (paths.length === 0) {
            applyingMany = false
            busy = false
            errorText = Permissions.multiResult(skipped)
            cancelButton.forceActiveFocus()
            return
        }
        multiApplySkipped = skipped
        requested({ c: "permissionsBatch", paths: paths, modes: modes, id: requestId })
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
    FontMetrics { id: noteFont; font { family: Theme.font.family; pixelSize: Theme.font.caption } }
    Keys.onTabPressed: function(event) { root.stepFocus((event.modifiers & Qt.ShiftModifier) !== 0); event.accepted = true }
    Keys.onBacktabPressed: function(event) { root.stepFocus(true); event.accepted = true }
    Keys.onPressed: function(event) { event.accepted = true }
    Keys.onEscapePressed: function(event) { root.close(); event.accepted = true }
    Flea.DialogScrim { onDismissed: root.close() }
    Rectangle {
        id: card
        x: Theme.cardOrigin(root.width, width)
        y: Theme.cardOrigin(root.height, height)
        width: Theme.cardSpan(Math.round(root.boardCardWidth * root.boardScale), root.width - 2 * Theme.card.inset)
        height: Theme.cardSpan(Theme.spacing.hairline + chrome.height + body.wanted + Theme.spacing.rowPaddingX + root.bodyInset, root.height - 2 * Theme.card.inset)
        color: Theme.color.surface
        border.color: Theme.color.mutedFrame
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
            // The strip sits inside the card's border, as the board's title does, so its rule and marks start a hairline in.
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: Theme.spacing.hairline
            height: Theme.chromeHeight
            // The board centres the marks and title in the strip above its rule, so the rule's row is never part of the centring band.
            Item { id: titleBand; anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top; height: parent.height - Theme.spacing.hairline }
            Flea.Glyph {
                id: lockMark
                x: Theme.spacing.rowPaddingX
                y: Math.floor((titleBand.height - height) / 2)
                width: Theme.chromeMarkSize; height: title.height; name: "lock"; color: Theme.color.accent
            }
            Text {
                id: title
                x: lockMark.x + lockMark.width + Theme.spacing.gap
                y: Math.floor((titleBand.height - height) / 2) - root.stripTextRise
                text: root.isMulti ? Format.fill(Permissions.WORDS.titleMany, root.multiPaths.length) : "Permissions"
                color: Theme.color.foreground; textFormat: Text.PlainText; font { family: Theme.font.family; pixelSize: Theme.font.caption; bold: true }
            }
            // Dialogs rule 7: the way out is named beside the mark that performs it, the settings panel's own corner.
            Flea.EscapeHint {
                id: escHint
                anchors.right: closeMark.left
                anchors.rightMargin: Theme.spacing.gap
                y: Math.floor((titleBand.height - height) / 2) - root.stripTextRise
            }

            Flea.ChromeButton {
                id: closeMark
                ruleRows: Theme.spacing.hairline
                anchors.right: parent.right
                anchors.rightMargin: Theme.spacing.rowPaddingX
                anchors.verticalCenter: parent.verticalCenter
                glyph: "x"; gesturePolicy: TapHandler.ReleaseWithinBounds
                // Muted at rest as the board draws the strip; the keyboard adds ChromeButton's own frame.
                restingColor: Theme.color.mutedFrame
                enabled: !root.applyLocked
                accessName: "Close permissions"
                activeFocusOnTab: true
                keyboardFocused: activeFocus
                Keys.onTabPressed: function(event) { root.stepFocus((event.modifiers & Qt.ShiftModifier) !== 0) }
                Keys.onBacktabPressed: root.stepFocus(true)
                onActivated: root.close()
            }
            Rule { anchors.bottom: parent.bottom }
        }
        Flea.CardFrame {
            id: body
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: chrome.bottom
            anchors.bottom: parent.bottom
            bleed: Theme.focusClearance
            anchors.leftMargin: root.bodyInset - Theme.focusClearance
            anchors.rightMargin: root.bodyInset - Theme.focusClearance
            anchors.topMargin: Theme.spacing.rowPaddingX - Theme.focusClearance
            anchors.bottomMargin: root.bodyInset - Theme.focusClearance
            spacing: 0
            Column {
                width: body.holderWidth
                spacing: 0
                Row {
                    width: parent.width
                    height: root.controlHeight
                    spacing: Theme.spacing.gap
                    // For several items the grid and Apply are the whole card, so the name row drops out.
                    visible: !root.isMulti
                    Flea.Glyph { width: Theme.markSize; height: nameLabel.height; anchors.verticalCenter: parent.verticalCenter; name: root.facts.directory ? "folder" : "file"; color: Theme.color.mutedFrame }
                    Text { id: nameLabel; width: parent.width - Theme.markSize - kindLabel.width - 2 * parent.spacing; anchors.verticalCenter: parent.verticalCenter; text: root.path.split("/").pop(); elide: Text.ElideMiddle; textFormat: Text.PlainText; color: Theme.color.foreground; font { family: Theme.font.family; pixelSize: Theme.font.body } }
                    Text { id: kindLabel; anchors.verticalCenter: parent.verticalCenter; text: root.facts.ok ? (root.facts.directory ? "directory" : "file") : ""; textFormat: Text.PlainText; color: Theme.color.foreground; font { family: Theme.font.family; pixelSize: Theme.font.caption } }
                }
                Rule { visible: !root.isMulti }
                Flea.PermissionsHeadings { width: parent.width; card: root; enter: !root.isMulti && root.facts.directory === true }
                Repeater {
                    id: permissionRows
                    model: ["Owner", "Group", "Everyone"]
                    Row {
                        id: permissionRow
                        required property string modelData
                        required property int index
                        readonly property alias checks: checks
                        width: body.holderWidth
                        height: root.controlHeight
                        Text { width: root.labelWidth; anchors.verticalCenter: parent.verticalCenter; text: permissionRow.modelData; textFormat: Text.PlainText; color: Theme.color.foreground; font { family: Theme.font.family; pixelSize: Theme.font.body } }
                        Repeater {
                            id: checks
                            model: 3
                            FocusScope {
                                id: checkbox
                                required property int index
                                readonly property int bit: 1 << (8 - permissionRow.index * 3 - index)
                                readonly property bool checked: root.isMulti ? root.multiChecked(bit)
                                    : (root.modeValue >= 0 ? root.modeValue : parseInt(root.facts.mode || "0", 8)) & bit
                                width: root.bitWidth(index)
                                height: permissionRow.height
                                activeFocusOnTab: true
                                enabled: root.editable
                                // The pointer's own state, read by the native capture harness before it shoots a hover or a press.
                                readonly property bool hovered: boxHover.hovered
                                readonly property bool pressed: boxTap.pressed
                                Accessible.role: Accessible.CheckBox
                                Accessible.name: permissionRow.modelData + " " + ["read", "write", root.facts.directory ? "enter" : "execute"][index]
                                Accessible.checked: checked
                                Accessible.onPressAction: toggle()
                                Accessible.onToggleAction: toggle()
                                function toggle() {
                                    if (!root.editable) return
                                    if (root.isMulti) root.multiToggle(bit)
                                    else root.modeText = Permissions.toggle(root.modeText, bit)
                                    forceActiveFocus()
                                }
                                Keys.onSpacePressed: toggle()
                                Keys.onTabPressed: function(event) { root.stepFocus((event.modifiers & Qt.ShiftModifier) !== 0) }
                                Keys.onBacktabPressed: root.stepFocus(true)
                                Flea.CheckBox {
                                    x: root.boxLead(checkbox.index, width)
                                    y: Math.round((parent.height - height) / 2)
                                    // A bit differing across the files shows a bar until it is clicked.
                                    value: root.isMulti ? root.multiValue(bit) : checkbox.checked ? "on" : "off"
                                    focused: checkbox.activeFocus
                                    // A disabled row stays checked, so the box dims and keeps its value.
                                    available: root.editable
                                }
                                HoverHandler { id: boxHover }
                                TapHandler { id: boxTap; gesturePolicy: TapHandler.ReleaseWithinBounds; onTapped: checkbox.toggle() }
                            }
                        }
                    }
                }
                Item {
                    width: parent.width
                    height: Theme.spacing.rowPaddingY + root.railHalf + Theme.spacing.hairline
                    visible: !root.isMulti
                    Rule { y: root.railHalf }
                }
                Row {
                    width: parent.width
                    height: root.controlHeight
                    spacing: Theme.spacing.gap
                    // Permissions040: for several items Octal drops out.
                    visible: !root.isMulti
                    Text { width: root.labelWidth; anchors.verticalCenter: parent.verticalCenter; text: "Octal"; textFormat: Text.PlainText; color: Theme.color.foreground; font { family: Theme.font.family; pixelSize: Theme.font.body } }
                    Rectangle {
                        id: octalBox
                        width: body.holderWidth - root.labelWidth - parent.spacing
                        height: root.fieldHeight
                        y: Math.round((parent.height - height) / 2)
                        color: Theme.color.background
                        // A focused field is its own frame in the accent, and the error role where its line reports an error, as RenameField draws it.
                        border.color: octal.activeFocus ? (errorLabel.visible && !root.busy ? Theme.color.error : Theme.color.accent) : Theme.color.mutedFrame
                        TextInput {
                            id: octal
                            anchors.fill: parent
                            anchors.leftMargin: Theme.spacing.gap
                            anchors.rightMargin: Theme.spacing.gap
                            verticalAlignment: TextInput.AlignVCenter
                            text: root.modeText
                            readOnly: !root.editable
                            activeFocusOnTab: !root.isMulti
                            enabled: root.editable && !root.isMulti
                            Accessible.name: "Octal mode"
                            color: root.editable ? Theme.color.foreground : Theme.color.muted
                            font { family: Theme.font.family; pixelSize: Theme.font.body }
                            clip: true
                            selectByMouse: true
                            Keys.onTabPressed: function(event) { root.stepFocus((event.modifiers & Qt.ShiftModifier) !== 0) }
                            Keys.onBacktabPressed: root.stepFocus(true)
                            onTextEdited: root.modeText = text
                            onAccepted: if (root.editable && root.modeValue >= 0) applyButton.forceActiveFocus()
                        }
                    }
                }
                Repeater {
                    model: ["Owner", "Group"]
                    Row {
                        required property string modelData
                        required property int index
                        // Permissions040: for several items Owner and Group drop out.
                        visible: !root.isMulti
                        width: body.holderWidth
                        height: root.controlHeight
                        spacing: Theme.spacing.gap
                        Text { width: root.labelWidth; anchors.verticalCenter: parent.verticalCenter; text: parent.modelData; textFormat: Text.PlainText; color: Theme.color.foreground; font { family: Theme.font.family; pixelSize: Theme.font.body } }
                        Text { width: body.holderWidth - root.labelWidth - identity.width - 2 * parent.spacing; anchors.verticalCenter: parent.verticalCenter; text: root.facts.ok ? ((parent.index === 0 ? root.facts.owner : root.facts.group) || "Unknown") + " · read-only" : ""; textFormat: Text.PlainText; elide: Text.ElideRight; color: Theme.color.foreground; font { family: Theme.font.family; pixelSize: Theme.font.body } }
                        Text { id: identity; anchors.verticalCenter: parent.verticalCenter; text: root.facts.ok ? (parent.index === 0 ? "uid " + root.facts.uid : "gid " + root.facts.gid) : ""; textFormat: Text.PlainText; color: Theme.color.foreground; font { family: Theme.font.family; pixelSize: Theme.font.caption } }
                    }
                }
                Rectangle {
                    width: parent.width
                    height: directoryScope.implicitHeight + 2 * Theme.spacing.gap
                    // Permissions040: for several items the scope box drops out.
                    visible: root.facts.directory === true && !root.isMulti
                    color: Theme.color.background
                    border.width: Theme.spacing.hairline
                    border.color: Theme.color.mutedFrame
                    Flea.Glyph { id: scopeMark; x: Theme.spacing.gap; y: Theme.spacing.gap; width: Theme.font.bodySmall; height: width; name: "check"; color: Theme.color.accent }
                    Text {
                        id: directoryScope
                        x: scopeMark.x + scopeMark.width + Theme.spacing.gap
                        y: Theme.spacing.gap
                        width: parent.width - x - Theme.spacing.gap
                        text: "Scope: this folder only. Items inside it keep their permissions."
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
                        ? (root.isMulti ? (root.multiPending > 0 ? "Reading permissions…" : "Applying permissions…")
                                        : (root.facts.ok ? "Applying permissions…" : "Reading permissions…"))
                        : root.facts.ok && root.modeValue < 0 ? "Enter three octal digits or a leading-zero four-digit mode." : "")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: root.busy ? Theme.color.muted : Theme.color.errorOnSurface
                    font { family: Theme.font.family; pixelSize: Theme.font.caption }
                }
                Item {
                    width: parent.width
                    height: root.isMulti ? root.multiGap : Theme.settings.railPaddingY + Theme.spacing.gap + Theme.spacing.hairline
                    Rule { y: Theme.settings.railPaddingY; visible: !root.isMulti }
                }
                Text {
                    text: "WILL CHANGE"
                    visible: !root.isMulti
                    bottomPadding: Math.round(Theme.spacing.rowPaddingY / 2)
                    textFormat: Text.PlainText
                    color: Theme.color.foreground
                    font { family: Theme.font.family; pixelSize: Theme.font.caption; letterSpacing: Theme.font.caption / 10 }
                }
                Text {
                    id: changeSummary
                    width: parent.width
                    // For several items the grid and Apply are the whole card, so the preview drops out.
                    visible: !root.isMulti
                    text: (root.facts.ok && !root.facts.reason && root.modeValue >= 0
                        ? Format.fill(Permissions.WORDS.requestedMode, Permissions.octal(root.modeValue)) : "No changes to apply")
                        + "\nPath " + root.path.replace(/\//g, "/\u200b")
                        + (root.facts.reason ? "\n" + Format.fill(Permissions.WORDS.currentMode, root.facts.mode) : "")
                    textFormat: Text.PlainText
                    wrapMode: Text.WordWrap
                    color: Theme.color.accent
                    font { family: Theme.font.family; pixelSize: Theme.font.caption }
                }
                Text {
                    id: scopeLabel
                    width: parent.width
                    // Fixed line boxes put the spare height under the glyphs, so the top padding centres them as CSS line-height does.
                    height: lineCount * SettingsMetrics.hintLine
                    lineHeight: SettingsMetrics.hintLine
                    lineHeightMode: Text.FixedHeight
                    topPadding: root.captionLead
                    // A box whose bit differs across the files shows a bar until it is clicked.
                    text: root.isMulti ? Permissions.mixedNote() : root.scopeText
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Theme.color.foreground
                    font { family: Theme.font.family; pixelSize: Theme.font.caption }
                }
            }
            buttons: Column {
                width: parent.width
                Item { width: parent.width; height: root.isMulti ? root.multiGap + root.buttonLead : Theme.settings.railPaddingY + Theme.spacing.hairline }
                Row {
                    anchors.right: parent.right
                    spacing: Theme.spacing.gap
                    Flea.DialogButton {
                        id: cancelButton
                        label: "Cancel"
                        available: !root.applyLocked
                        tabHandle: true
                        tabStop: true
                        onTabbed: function(from, back) { root.stepFocus(back) }
                        onActivated: root.close()
                    }
                    Flea.DialogButton {
                        id: applyButton
                        label: "Apply"
                        primary: true
                        // The multi card has no octal to validate, so Apply enables on the grid alone.
                        available: root.canApply
                        tabHandle: true
                        tabStop: true
                        onTabbed: function(from, back) { root.stepFocus(back) }
                        onActivated: root.apply()
                    }
                }
            }
        }
    }
}
