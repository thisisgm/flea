//@ pragma ShellId flea-preview-text-actions-test
import QtQuick
import QtTest
import Quickshell
import "flea" as Flea

// Drive the native editor and clipboard, including selection spanning line breaks.
ShellRoot {
    id: root
    property int failures: 0
    property int stage: 0
    readonly property string sample: "First & <plain>\nSecond café 😀\nThird line\n"
    function check(name, ok) {
        console.log("TEXTACTIONS " + (ok ? "PASS " : "FAIL ") + name)
        if (!ok) root.failures++
    }
    function findActions(item) {
        if (typeof item.copyAll === "function" && item.selectControl !== undefined) return item
        for (var i = 0; i < item.children.length; i++) {
            var found = root.findActions(item.children[i])
            if (found !== null) return found
        }
        return null
    }
    function click(item) { keys.mouseClick(item, item.width / 2, item.height / 2, Qt.LeftButton, Qt.NoModifier, -1) }
    function editor() { return actions.selectionItem.editor }
    function clipboardText() {
        receiver.text = ""
        receiver.paste()
        return receiver.text
    }
    Window {
        id: win
        visible: true
        width: 640
        height: 400
        Item {
            id: host
            anchors.fill: parent
            Flea.PreviewTextActions {
                id: actions
                target: host
                available: true
                sourceText: root.sample
            }
        }
        Loader { id: quick; anchors.fill: parent; active: false; sourceComponent: plainPreview }
        Component {
            id: plainPreview
            Flea.PreviewText { active: true; path: Quickshell.env("TEXT_ACTIONS_FILE"); size: 100 }
        }
        Component {
            id: markdownPreview
            Flea.MarkdownPane { active: true; path: Quickshell.env("TEXT_ACTIONS_FILE"); size: 100 }
        }
        TextEdit { id: receiver; visible: false; textFormat: TextEdit.PlainText }
        TestEvent { id: keys }
    }
    Timer {
        interval: 120
        running: true
        repeat: true
        onTriggered: {
            switch (root.stage++) {
            case 0:
                root.check("selection document stays lazy", actions.selectionItem === null)
                actions.selecting = true
                break
            case 1:
                root.check("source is plain, read-only and selectable", root.editor().readOnly && root.editor().selectByMouse && root.editor().textFormat === TextEdit.PlainText && root.editor().text === root.sample)
                var e = root.editor(), a = e.positionToRectangle(2), b = e.positionToRectangle(22)
                keys.mousePress(e, a.x + 1, a.y + a.height / 2, Qt.LeftButton, Qt.NoModifier, -1)
                keys.mouseMove(e, b.x + 1, b.y + b.height / 2, -1, Qt.LeftButton, Qt.NoModifier)
                keys.mouseRelease(e, b.x + 1, b.y + b.height / 2, Qt.LeftButton, Qt.NoModifier, -1)
                root.check("pointer selects across lines", e.selectedText.indexOf("\n") >= 0)
                e.select(2, 22)
                keys.keyClick(Qt.Key_C, Qt.ControlModifier, -1)
                root.check("Ctrl+C copies only selected text", root.clipboardText() === root.sample.substring(2, 22))
                e.cursorPosition = 0
                keys.keyClick(Qt.Key_Right, Qt.ShiftModifier, -1)
                root.check("Shift+Arrow selects a source-text range", e.selectedText === root.sample.charAt(0))
                keys.keyClick(Qt.Key_C, Qt.ControlModifier, -1)
                root.check("keyboard-selected range copies", root.clipboardText() === root.sample.charAt(0))
                keys.keyClick(Qt.Key_A, Qt.ControlModifier, -1)
                root.check("Ctrl+A selects the full document", e.selectedText === root.sample)
                keys.keyClick(Qt.Key_X, Qt.NoModifier, -1)
                keys.keyClick(Qt.Key_Backspace, Qt.NoModifier, -1)
                root.check("typing and deletion cannot edit source", e.text === root.sample)
                actions.copyAll()
                root.check("copy all preserves markup, Unicode and final newline", root.clipboardText() === root.sample && actions.copied)
                actions.path = "/another.md"
                root.check("navigation clears selection and feedback", actions.selectionItem === null && !actions.copied && !actions.copyPending)
                actions.sourceText = null
                actions.path = Quickshell.env("TEXT_ACTIONS_FILE")
                actions.copyAll()
                break
            case 2:
                root.check("on-demand file copy keeps all bytes", actions.copied && root.clipboardText() === root.sample)
                actions.selecting = true
                break
            case 3:
                root.check("file selection loads full source", actions.selectionItem !== null && root.editor().text === root.sample)
                actions.available = false
                root.check("unavailable preview releases selection", actions.selectionItem === null && !actions.copyPending)
                actions.path = "/missing-text-actions-file"
                actions.available = true
                actions.copyAll()
                break
            case 4:
                root.check("read failure never reports copied", actions.failure !== "" && !actions.copied && !actions.copyPending)
                actions.available = false
                actions.maxBytes = 2
                actions.path = Quickshell.env("TEXT_ACTIONS_FILE")
                actions.available = true
                actions.copyAll()
                break
            case 5:
                root.check("grown file is refused by actual byte size", actions.failure === "This file is too large to preview." && !actions.copied)
                root.check("read stops after the size gate plus one byte", actions.readBytes === actions.maxBytes + 1)
                actions.available = false
                actions.maxBytes = 1024
                actions.sourceText = "abc\u0000def"
                actions.available = true
                actions.copyAll()
                root.check("binary source cannot report copied", actions.failure === "This is a binary file." && !actions.copied)
                actions.available = false
                actions.sourceText = "😀"
                actions.maxBytes = 3
                actions.available = true
                actions.copyAll()
                root.check("source limit counts UTF-8 bytes", actions.failure === "This file is too large to preview." && !actions.copied)
                actions.available = false
                actions.sourceText = ""
                actions.available = true
                actions.copyAll()
                root.check("empty document never falsely reports copied", !actions.copied && !actions.copyPending)
                quick.active = true
                break
            case 6:
                if (!quick.item || quick.item.status !== "ready") { root.stage--; break }
                root.check("plain text is selectable without a mode button", root.findActions(quick.item).selectionItem !== null && !root.findActions(quick.item).selectControl.visible)
                break
            case 7:
                var plain = root.findActions(quick.item)
                root.check("text header opens its own selectable source", plain.selectionItem !== null && plain.selectionItem.editor.text === root.sample && plain.selectionItem.editor.activeFocus)
                root.click(plain.copyControl)
                root.check("text header copies the whole source", root.clipboardText() === root.sample && plain.copied)
                root.click(plain.selectControl)
                quick.sourceComponent = markdownPreview
                break
            case 8:
                if (!quick.item || !quick.item.contentReady) { root.stage--; break }
                root.click(root.findActions(quick.item).selectControl)
                break
            case 9:
                var md = root.findActions(quick.item)
                root.check("Markdown header opens its own selectable source", md.selectionItem !== null && md.selectionItem.editor.text === root.sample && md.selectionItem.editor.activeFocus)
                root.click(md.copyControl)
                root.check("Markdown header copies raw source", root.clipboardText() === root.sample && md.copied)
                quick.active = false
                actions.available = false
                actions.sourceText = Array(120).fill("Long selectable line with Unicode café 😀.").join("\n")
                actions.maxBytes = 1048576
                actions.available = true
                actions.selecting = true
                break
            case 10:
                var longEdit = root.editor(), from = longEdit.positionToRectangle(270), to = longEdit.positionToRectangle(8)
                var top = actions.selectionItem.viewport.contentY
                keys.mousePress(longEdit, from.x + 1, from.y + from.height / 2, Qt.LeftButton, Qt.NoModifier, -1)
                keys.mouseMove(longEdit, to.x + 1, to.y + to.height / 2, -1, Qt.LeftButton, Qt.NoModifier)
                keys.mouseRelease(longEdit, to.x + 1, to.y + to.height / 2, Qt.LeftButton, Qt.NoModifier, -1)
                root.check("overflowing text drag selects instead of scrolling", longEdit.selectedText.length > 200 && actions.selectionItem.viewport.contentY === top)
                keys.mouseWheel(actions.selectionItem, actions.selectionItem.width / 2, actions.selectionItem.height / 2, Qt.NoButton, Qt.NoModifier, 0, -120, -1)
                root.check("wheel still scrolls selectable text", actions.selectionItem.viewport.contentY > top)
                actions.available = false
                actions.sourceText = null
                actions.path = Quickshell.env("TEXT_ACTIONS_BIG")
                actions.maxBytes = 256 * 1024
                actions.available = true
                actions.copyAll()
                break
            case 11:
                root.check("remote gate bounds the actual read", actions.failure === "This file is too large to preview." && actions.readBytes === 256 * 1024 + 1 && !actions.copied)
                actions.available = false
                actions.maxBytes = 1048576
                actions.available = true
                actions.copyAll()
                break
            case 12:
                root.check("local gate bounds the actual read", actions.failure === "This file is too large to preview." && actions.readBytes === 1048577 && !actions.copied)
                console.log("TEXTACTIONS DONE failures=" + root.failures)
                Qt.quit()
                break
            }
        }
    }
}
