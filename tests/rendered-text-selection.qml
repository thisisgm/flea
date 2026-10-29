//@ pragma ShellId flea-rendered-text-selection-test
import QtQuick
import QtTest
import Quickshell
import "flea" as Flea

ShellRoot {
    id: root
    property int stage: 0
    property int failures: 0
    property real beforeScroll: 0
    property var textItem: null
    function check(name, ok, detail) {
        console.log("RENDERSELECT " + (ok ? "PASS " : "FAIL ") + name + (detail ? " " + detail : ""))
        if (!ok) root.failures++
    }
    function drag(item, from, to) {
        keys.mousePress(item, from.x + 1, from.y + from.height / 2, Qt.LeftButton, Qt.NoModifier, -1)
        keys.mouseMove(item, to.x + 1, to.y + to.height / 2, -1, Qt.LeftButton, Qt.NoModifier)
        keys.mouseRelease(item, to.x + 1, to.y + to.height / 2, Qt.LeftButton, Qt.NoModifier, -1)
    }
    function pasted() { receiver.text = ""; receiver.paste(); return receiver.text }
    Window {
        visible: true
        width: 620
        height: 320
        Flea.PreviewMarkdown {
            id: doc
            anchors.fill: parent
            active: true
            path: Quickshell.env("RENDER_SELECTION_FILE")
            size: 4000
        }
        Flea.CodeLines {
            id: code
            anchors.fill: parent
            active: false
            visible: false
            fromText: true
            text: Array(100).fill("Line of plain text with <literal> & Unicode café.").join("\n")
            scrolls: true
        }
        Text { id: lateHost; visible: false; text: "alpha"; textFormat: Text.PlainText }
        Flea.PreviewSelectionDocument { id: lateEditor; visible: false; host: lateHost }
        TextEdit { id: receiver; visible: false; textFormat: TextEdit.PlainText }
        TestEvent { id: keys }
    }
    Timer {
        interval: 160
        running: true
        repeat: true
        onTriggered: {
            switch (root.stage++) {
            case 0:
                if (!doc.contentReady) { root.stage--; break }
                var texts = doc.renderedSelection.texts()
                root.check("rendered selection keeps native documents lazy", texts.length > 2 && texts.every(function(t) { return t.selectionEditor === null }))
                lateHost.text = '<font color="#112233">alpha<br>beta</font>'
                root.textItem = texts[0]
                var p = root.textItem.mapToItem(doc.renderedSelection, 8, root.textItem.topPadding + 8)
                keys.mouseClick(doc.renderedSelection, p.x, p.y, Qt.LeftButton, Qt.NoModifier, -1)
                break
            case 1:
                lateHost.textFormat = Text.StyledText
                break
            case 2:
                root.check("late format-only change refreshes copied text", lateEditor.getText(0, lateEditor.length) === "alpha\u2028beta")
                lateHost.lineHeightMode = Text.FixedHeight
                lateHost.lineHeight = 30
                break
            case 3:
                var fixedStep = lateEditor.positionToRectangle(lateEditor.length - 1).y - lateEditor.positionToRectangle(0).y
                root.check("line-height change refreshes selection layout", Math.abs(fixedStep - 30) <= 1, "step=" + fixedStep)
                lateHost.lineHeightMode = Text.ProportionalHeight
                break
            case 4:
                root.check("line-height mode change refreshes selection layout", lateEditor.positionToRectangle(lateEditor.length - 1).y - lateEditor.positionToRectangle(0).y > 30)
                var t = root.textItem, edit = t.selectionEditor
                root.check("press builds native rendered text", edit !== null && edit.getText(0, edit.length).indexOf("bold") >= 0)
                root.check("selection baseline matches rendered ink", Math.abs(t.drawnBaseline - edit.baselineOffset) <= 1, "host=" + t.drawnBaseline + " editor=" + edit.baselineOffset)
                root.check("selection preserves literal CSS-like text", edit.getText(0, edit.length).indexOf("color: red; margin-top:8px;") >= 0)
                root.beforeScroll = doc.viewport.contentY
                root.drag(edit, edit.positionToRectangle(2), edit.positionToRectangle(40))
                root.check("long drag selects rendered words instead of scrolling", doc.renderedSelection.selectedText.length > 15 && doc.viewport.contentY === root.beforeScroll, doc.renderedSelection.selectedText)
                keys.keyClick(Qt.Key_C, Qt.ControlModifier, -1)
                root.check("Ctrl+C copies rendered words without Markdown markup", root.pasted() === doc.renderedSelection.selectedText && root.pasted().indexOf("<strong>") < 0 && root.pasted().indexOf("**") < 0)
                var texts2 = doc.renderedSelection.texts(), next = texts2[1], nextEdit = doc.renderedSelection.editorFor(next)
                var start = edit.positionToRectangle(4), end = nextEdit.positionToRectangle(12)
                var a = edit.mapToItem(doc.renderedSelection, start.x, start.y), b = nextEdit.mapToItem(doc.renderedSelection, end.x, end.y)
                root.drag(doc.renderedSelection, Qt.rect(a.x,a.y,1,start.height), Qt.rect(b.x,b.y,1,end.height))
                keys.keyClick(Qt.Key_C, Qt.ControlModifier, -1)
                root.check("selection and copying span rendered paragraphs", doc.renderedSelection.selected.length >= 2 && root.pasted().indexOf("\n") >= 0 && doc.viewport.contentY === root.beforeScroll)
                root.check("document remains rendered after selection", doc.shownView === "rendered")
                doc.renderedSelection.clear()
                root.check("clearing drops the old selection", edit.selectedText === "" && doc.renderedSelection.selectedText === "")
                keys.mouseWheel(doc, doc.width / 2, doc.height / 2, Qt.NoButton, Qt.NoModifier, 0, -120, -1)
                root.check("wheel still scrolls rendered Markdown", doc.viewport.contentY > root.beforeScroll)
                doc.visible = false
                doc.active = false
                code.active = true
                code.visible = true
                break
            case 5:
                var route = null
                for (var k = 0; k < code.children.length; k++)
                    if (code.children[k].texts !== undefined) route = code.children[k]
                var rows = route.texts(), first = route.editorFor(rows[2]), last = route.editorFor(rows[5])
                var from = first.positionToRectangle(4), to = last.positionToRectangle(18)
                var cp = first.mapToItem(route, from.x, from.y), cq = last.mapToItem(route, to.x, to.y)
                var cy = code.viewport.contentY
                root.drag(route, Qt.rect(cp.x,cp.y,1,from.height), Qt.rect(cq.x,cq.y,1,to.height))
                keys.keyClick(Qt.Key_C, Qt.ControlModifier, -1)
                root.check("column text selects across lines without dragging scroll", route.selectedText.indexOf("\n") >= 0 && code.viewport.contentY === cy)
                root.check("column text copies decoded literal content", root.pasted() === route.selectedText && root.pasted().indexOf("<literal> & Unicode café.") >= 0)
                console.log("RENDERSELECT DONE failures=" + root.failures)
                Qt.quit()
            }
        }
    }
}
