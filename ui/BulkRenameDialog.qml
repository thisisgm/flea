import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import "." as Flea
import "js/BulkRename.js" as BulkRename

// Bulk rename card: Find/Replace, one editable line per new name, live reasons, one Rename and one undo.
FocusScope {
    id: root
    anchors.fill: parent
    visible: opened || opacity > 0
    // Closed means no input, even while the fade-out is still drawn.
    enabled: opened
    opacity: 0
    property bool opened: false
    property var oldNames: []
    property var newNames: []
    property var sourcePaths: []
    property string folder: ""
    property string find: ""
    property string replace: ""
    property var owner: null
    readonly property real cardWant: Theme.card.sheet
    readonly property int cardPadding: Theme.card.padding
    // 255 name bytes plus one newline per selected name, the TUI batch's own bound.
    readonly property int nameLineBytes: 256
    // The two half-width columns share one gap, so both halves measure the same remainder.
    readonly property int columnGap: 14
    // The new-names stack breathes less than a column gap, the way the old-names one does.
    readonly property int stackGap: Theme.card.labelGap
    // The names frame's sides, wider than its top and bottom for the caret; those take the old column's rowsInset.
    readonly property int frameSide: Theme.spacing.gap
    // A new name and its reason sit one side-inset apart.
    readonly property int reasonGap: Theme.spacing.gap
    // The mktemp leaf's fixed stem, so the cleanup guard measures the template it cleans.
    readonly property string scratchStem: "flea-rename-"
    property string terminalBase: ""
    property string terminalDir: ""
    property string terminalFile: ""
    property bool terminalActive: false
    property string terminalError: ""
    readonly property var cardItem: card
    // The model is a count; edits mutate one element and bump namesVersion, so a keystroke never rebuilds rows or steals the caret.
    property int namesVersion: 0
    property int focusedLine: -1
    readonly property var allReasons: { root.namesVersion; return BulkRename.reasons(root.newNames) }
    readonly property var counts: BulkRename.counts(root.oldNames, root.newNames, root.allReasons)
    readonly property string footerText: BulkRename.footer(counts.renamed, counts.unchanged, counts.invalid)
    readonly property string renameText: BulkRename.renameLabel(counts.renamed)
    readonly property bool canRename: root.opened && counts.invalid === 0 && counts.renamed > 0
    // The control holding the keyboard, so the capture case asserts Tab order without OCR.
    readonly property string focusedControl: editButton.activeFocus ? "Edit in terminal"
        : cancelButton.activeFocus ? "Cancel" : renameButton.activeFocus ? "Rename"
        : findField.focused ? "Find" : replaceField.focused ? "Replace"
        : namesScope.activeFocus && root.focusedLine >= 0 ? "line " + (root.focusedLine + 1) : ""
    // Every Tab stop routes through the pure order in BulkRename.js, so Shift+Tab walks back the same way.
    function stepFocus(current, back) {
        var stop = BulkRename.nextStop(current, back, root.oldNames.length)
        var why = back ? Qt.BacktabFocusReason : Qt.TabFocusReason
        if (stop === "find") findField.takeFocus()
        else if (stop === "replace") replaceField.takeFocus()
        else if (stop === "edit") editButton.forceActiveFocus(why)
        else if (stop === "cancel") cancelButton.forceActiveFocus(why)
        else if (stop === "rename") renameButton.forceActiveFocus(why)
        else root.focusNameLine(Number(stop.substring(5)) - 1, why)
    }
    // Return/Up/Down move the caret between name lines, stopping at the ends, as a text editor's lines do.
    function focusNameLine(index, why, col) { if (index >= 0 && index < nameLines.count) nameLines.itemAt(index).takeFocus(why, col) }
    function reasonAt(index) { return index < root.allReasons.length ? root.allReasons[index] : "" }
    function changedAt(index) { root.namesVersion; return String(root.newNames[index]) !== String(root.oldNames[index]) }
    function open(paths, holder) {
        owner = holder
        sourcePaths = paths.slice()
        folder = String(paths[0]).substring(0, String(paths[0]).lastIndexOf("/")) || "/"
        oldNames = paths.map(function (p) { return String(p).substring(String(p).lastIndexOf("/") + 1) })
        newNames = oldNames.slice()
        find = ""
        replace = ""
        terminalBase = ""
        terminalDir = ""
        terminalFile = ""
        terminalActive = false
        terminalError = ""
        opened = true
        findField.takeFocus()
    }
    function close() {
        if (terminalActive) cleanupTerminal()
        opened = false
        if (owner) owner.forceActiveFocus()
    }
    function applyFind() {
        newNames = BulkRename.applyFindReplace(root.oldNames, root.find, root.replace)
    }
    function commit() {
        if (!root.canRename) return
        var backend = owner.backend
        var paths = root.sourcePaths.slice()
        var names = root.newNames.slice()
        root.close()
        backend.bulkRename(paths, names)
    }
    // Edit in terminal: names go to a private mktemp dir, $EDITOR opens the file, and the card reads it back on exit.
    function editInTerminal() {
        root.terminalError = ""
        if (root.terminalActive) root.cleanupTerminal()
        root.terminalBase = Quickshell.env("TMPDIR") || "/tmp"
        mktemp.command = ["mktemp", "-d", root.terminalBase + "/" + root.scratchStem + "XXXXXX"]
        mktemp.running = true
    }
    // $EDITOR over the names file in a terminal through xdg-terminal-exec, the file riding as argv and never in the shell string.
    function launchEditor() {
        var editor = Quickshell.env("VISUAL") || Quickshell.env("EDITOR") || ""
        if (!editor || editChild.running) {
            root.terminalError = "Bulk rename could not open an editor."
            return
        }
        editChild.command = ["xdg-terminal-exec", "--dir=" + root.folder, "--",
            "/bin/sh", "-c", "exec " + editor + " \"$1\"", "flea-bulk-rename", root.terminalFile]
        editChild.running = true
    }
    // A scratch dir is the base plus one flea-rename leaf, so a cleanup never walks elsewhere.
    function isScratchDir(dir) {
        if (terminalBase.length === 0 || dir.length <= terminalBase.length + root.scratchStem.length + 1) return false
        if (dir.substring(0, terminalBase.length + 1) !== terminalBase + "/") return false
        var leaf = dir.substring(terminalBase.length + 1)
        if (leaf.indexOf("/") >= 0 || leaf.indexOf("..") >= 0) return false
        return leaf.substring(0, root.scratchStem.length) === root.scratchStem
    }
    // Sample input: "first.png\nsecond.png\n", one name per line with a single trailing newline.
    function reloadTerminal() {
        terminalFileView.reload()
        terminalFileView.waitForJob()
        var text = terminalFileView.text()
        if (text.length > root.oldNames.length * root.nameLineBytes) {
            terminalError = "Bulk rename refused: the edited names exceed the filename limits."
            return
        }
        var lines = String(text).split("\n")
        if (lines.length > 0 && lines[lines.length - 1] === "") lines.pop()
        if (lines.length !== root.oldNames.length) {
            terminalError = "Bulk rename refused: the number of names changed."
            return
        }
        terminalError = ""
        newNames = lines
    }
    function cleanupTerminal() {
        terminalActive = false
        if (root.terminalDir.length > 0 && isScratchDir(root.terminalDir)) {
            cleanup.command = ["rm", "-rf", root.terminalDir]
            cleanup.running = true
        }
        terminalDir = ""
        terminalFile = ""
    }
    Keys.onPressed: function (event) {
        if (event.key === Qt.Key_Escape) { root.close(); event.accepted = true; return }
        if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) { event.accepted = true; return }
        if (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)) { event.accepted = true; return }
        event.accepted = true
    }
    Flea.Reveal { target: root; shown: root.opened; lift: card }
    Process {
        id: mktemp
        stderr: StdioCollector { waitForEnd: true }
        stdout: StdioCollector { waitForEnd: true }
        onExited: function (code) {
            var dir = String(mktemp.stdout.text).trim()
            if (!root.opened) {
                if (code === 0 && root.isScratchDir(dir)) { cleanup.command = ["rm", "-rf", dir]; cleanup.running = true }
                return
            }
            if (code !== 0 || !root.isScratchDir(dir)) { root.terminalError = "Bulk rename could not make its edit directory."; return }
            root.terminalDir = dir
            root.terminalFile = dir + "/names"
            // The file is created at 0600 here, so the editor's own save keeps it private.
            prime.command = ["install", "-m", "0600", "/dev/null", root.terminalFile]
            prime.running = true
        }
    }
    // install names the mode at creation, the way the TUI batch's exclusive create does.
    Process {
        id: prime
        onExited: function (code) {
            if (!root.opened) { root.cleanupTerminal(); return }
            if (code !== 0) { root.terminalError = "Bulk rename could not make its edit file."; root.cleanupTerminal(); return }
            terminalFileView.path = root.terminalFile
            terminalFileView.setText(root.newNames.join("\n") + "\n")
            terminalFileView.waitForJob()
            root.terminalActive = true
            root.launchEditor()
        }
    }
    Process {
        id: editChild
        onExited: function (code) {
            if (!root.terminalActive) return
            if (code === 0) root.reloadTerminal()
            else root.terminalError = "Bulk rename could not open an editor."
        }
    }
    Process {
        id: cleanup
    }
    FileView {
        id: terminalFileView
        watchChanges: root.terminalActive
        onFileChanged: if (root.terminalActive) root.reloadTerminal()
    }
    Connections {
        target: findField.input
        function onTextChanged() { root.find = findField.input.text; root.applyFind() }
    }
    Connections {
        target: replaceField.input
        function onTextChanged() { root.replace = replaceField.input.text; root.applyFind() }
    }
    Flea.DialogScrim {
        onDismissed: root.close()
    }
    Rectangle {
        id: card
        x: Theme.cardOrigin(root.width, width)
        y: Theme.cardOrigin(root.height, height)
        width: Theme.cardSpan(root.cardWant, root.width - 2 * Theme.card.inset)
        height: Theme.cardSpan(body.wanted + 2 * root.cardPadding + 2 * Theme.spacing.hairline, root.height - 2 * Theme.card.inset)
        color: Theme.color.surface
        border.color: Theme.color.mutedFrame
        border.width: Theme.spacing.hairline
        radius: Style.cornerRadius
        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onWheel: function (wheel) { wheel.accepted = true }
        }
        Flea.CardFrame {
            id: body
            bleed: Theme.focusClearance
            anchors.fill: parent
            anchors.leftMargin: root.cardPadding + Theme.spacing.hairline - Theme.focusClearance
            anchors.rightMargin: root.cardPadding + Theme.spacing.hairline - Theme.focusClearance
            anchors.topMargin: root.cardPadding + Theme.spacing.hairline - Theme.focusClearance
            anchors.bottomMargin: root.cardPadding + Theme.spacing.hairline - Theme.focusClearance
            title: Flea.DialogHeader { width: parent.width; text: BulkRename.title(root.oldNames.length) }
            buttons: Row {
                anchors.right: parent.right
                spacing: Theme.spacing.gap
                // Tab reaches every button, the BulkRename board's callout 6: Qt's chain skips a button without activeFocusOnTab.
                Flea.DialogButton { id: editButton; label: "Edit in terminal"; activeFocusOnTab: true; tabHandle: true; onTabbed: function (from, back) { root.stepFocus("edit", back) }; onActivated: root.editInTerminal() }
                Flea.DialogButton { id: cancelButton; label: "Cancel"; activeFocusOnTab: true; tabHandle: true; onTabbed: function (from, back) { root.stepFocus("cancel", back) }; onActivated: root.close() }
                Flea.DialogButton { id: renameButton; label: root.renameText; primary: true; available: root.canRename; activeFocusOnTab: true; tabHandle: true; onTabbed: function (from, back) { root.stepFocus("rename", back) }; onActivated: root.commit() }
            }
            Column {
                width: parent.width
                spacing: Theme.spacing.gap
                Row {
                    width: parent.width
                    spacing: root.columnGap
                    Flea.DialogField {
                        id: findField
                        width: (parent.width - root.columnGap) / 2
                        label: "Find"
                        onAccepted: root.commit()
                        // A field reports Tab instead of moving, so the card keeps the keyboard inside itself.
                        onTabbed: function (from, back) { root.stepFocus("find", back) }
                    }
                    Flea.DialogField {
                        id: replaceField
                        width: (parent.width - root.columnGap) / 2
                        label: "Replace"
                        onAccepted: root.commit()
                        onTabbed: function (from, back) { root.stepFocus("replace", back) }
                    }
                }
                Row {
                    width: parent.width
                    spacing: root.columnGap
                    Flea.BulkOldNames {
                        id: oldColumn
                        oldNames: root.oldNames
                        columnGap: root.columnGap
                        headGap: root.stackGap
                    }
                    Column {
                        width: (parent.width - root.columnGap) / 2
                        spacing: root.stackGap
                        Text {
                            text: "NEW NAMES"
                            color: Theme.color.muted
                            font.family: Theme.font.family
                            font.pixelSize: Theme.font.caption
                            textFormat: Text.PlainText
                        }
                        Rectangle {
                            id: namesBox
                            width: parent.width
                            color: Theme.color.background
                            // One editor, so the frame follows the caret however many lines it holds.
                            border.color: namesScope.activeFocus ? Theme.color.accent : Theme.color.mutedFrame
                            border.width: Theme.spacing.hairline
                            height: root.oldNames.length * Theme.fileRowHeight + 2 * oldColumn.rowsInset
                            // One focus scope around the name lines, so the frame binds live focus state with no counter to drift.
                            FocusScope {
                                id: namesScope
                                anchors.fill: parent
                                anchors.topMargin: oldColumn.rowsInset
                                anchors.bottomMargin: oldColumn.rowsInset
                                anchors.leftMargin: root.frameSide
                                anchors.rightMargin: root.frameSide
                                Column {
                                    anchors.fill: parent
                                    spacing: 0
                                    Repeater {
                                        id: nameLines
                                        model: root.oldNames.length
                                        delegate: Row {
                                            id: nameRow
                                            required property int index
                                            function takeFocus(why, col) { if (col !== undefined) nameField.cursorPosition = Math.min(col, nameField.length); nameField.forceActiveFocus(why) }
                                            width: parent.width
                                            height: Theme.fileRowHeight
                                            spacing: root.reasonGap
                                            TextInput {
                                                id: nameField
                                                width: parent.width - reason.width - parent.spacing
                                                anchors.verticalCenter: parent.verticalCenter
                                                text: BulkRename.nameAt(root.newNames, nameRow.index)
                                                color: root.reasonAt(nameRow.index) ? Theme.color.errorOnSurface
                                                    : root.changedAt(nameRow.index) ? Theme.color.foreground : Theme.color.muted
                                                selectionColor: Theme.color.accent
                                                font.family: Theme.font.family
                                                font.pixelSize: Theme.font.body
                                                clip: true
                                                onActiveFocusChanged: if (activeFocus) root.focusedLine = nameRow.index
                                                Keys.onTabPressed: function (event) { root.stepFocus("line " + (nameRow.index + 1), false); event.accepted = true }
                                                Keys.onBacktabPressed: function (event) { root.stepFocus("line " + (nameRow.index + 1), true); event.accepted = true }
                                                onTextChanged: {
                                                    if (root.newNames[nameRow.index] === nameField.text) return
                                                    root.newNames[nameRow.index] = nameField.text
                                                    root.namesVersion++
                                                }
                                                Keys.onReturnPressed: function (event) { root.focusNameLine(nameRow.index + 1, Qt.OtherFocusReason); event.accepted = true }
                                                Keys.onEnterPressed: function (event) { root.focusNameLine(nameRow.index + 1, Qt.OtherFocusReason); event.accepted = true }
                                                Keys.onUpPressed: function (event) { root.focusNameLine(nameRow.index - 1, Qt.OtherFocusReason, nameField.cursorPosition); event.accepted = true }
                                                Keys.onDownPressed: function (event) { root.focusNameLine(nameRow.index + 1, Qt.OtherFocusReason, nameField.cursorPosition); event.accepted = true }
                                            }
                                            Text {
                                                id: reason
                                                anchors.verticalCenter: parent.verticalCenter
                                                text: root.reasonAt(nameRow.index)
                                                textFormat: Text.PlainText
                                                color: Theme.color.errorOnSurface
                                                font.family: Theme.font.family
                                                font.pixelSize: Theme.font.caption
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Text {
                    width: parent.width
                    text: root.terminalError.length > 0 ? root.terminalError : root.footerText
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    color: root.terminalError.length > 0 ? Theme.color.errorOnSurface : Theme.color.foreground
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                }
            }
        }
    }
}
